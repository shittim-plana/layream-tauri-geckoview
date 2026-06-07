use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::voyage;

/// A single HyPA summary. Single definition site (§4.1) for both the core
/// selection pipeline and the Tauri `commands_hypa` layer.
///
/// Wire format uses camelCase keys (`chatMemos`, `isImportant` preserved via
/// serde rename) for interoperability with external preset/memory exports, plus
/// Layream extensions (`pinBoost`, `invalidated`). camelCase rename is
/// load-bearing: without it, existing hypa.json / external presets fail to
/// deserialize (§1.1).
///
/// Embeddings are stored as `Vec<f64>` to match the JSON wire format (JSON
/// numbers decode to f64) and to compose with `voyage::cosine_similarity`
/// without precision conversion. External exports carry no embedding — it is
/// `Option` so import yields `None` and the embedding is recomputed on first
/// use.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Summary {
    pub text: String,

    /// Chat message ids covered by this summary. Field name preserved for
    /// external wire compatibility.
    #[serde(rename = "chatMemos", default)]
    pub chat_memos: Vec<String>,

    /// Optional embedding vector used by the similarity phase / `hypa_search`.
    /// Absent in external exports → `None` on import, recomputed on first use.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub embedding: Option<Vec<f64>>,

    /// Whether this summary should always be included regardless of similarity
    /// (Phase 1). Field name preserved for external wire compatibility.
    #[serde(rename = "isImportant", default)]
    pub is_important: bool,

    /// Score boost applied when a covered chat is pinned (Layream extension).
    /// Drives the separate guaranteed-budget pin phase — distinct from
    /// `is_important`.
    #[serde(rename = "pinBoost", default)]
    pub pin_boost: f64,

    /// True once a covered chat has been deleted (Layream extension). Excluded
    /// in Phase 0 before any selection runs.
    #[serde(default)]
    pub invalidated: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct HypaData {
    #[serde(default)]
    pub summaries: Vec<Summary>,

    /// Roundtrip-preserved extra fields (e.g. external `metrics` /
    /// `modalSettings`, future display state). Captured here and re-emitted
    /// unchanged so an import→export cycle is lossless (§1.3 density
    /// preservation).
    #[serde(flatten)]
    pub extra: serde_json::Map<String, Value>,
}

/// HyPA selection settings. serde derive + camelCase so preset import/export
/// round-trips against the external settings-key schema. Every field is
/// `#[serde(default)]` so a preset that omits a key (external presets omit the
/// Layream-internal `randomMemoryRatio`) still deserializes (§1.1).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct HypaSettings {
    pub memory_tokens_ratio: f64,
    pub recent_memory_ratio: f64,
    pub similar_memory_ratio: f64,
    /// Layream-internal extension (not an external key). Drives Phase 4 budget.
    pub random_memory_ratio: f64,
    pub max_chats_per_summary: usize,
    pub preserve_orphaned_memory: bool,
}

impl Default for HypaSettings {
    fn default() -> Self {
        Self {
            memory_tokens_ratio: 0.2,
            recent_memory_ratio: 0.3,
            similar_memory_ratio: 0.5,
            random_memory_ratio: 0.2,
            max_chats_per_summary: 6,
            preserve_orphaned_memory: false,
        }
    }
}

pub fn clean_orphaned(data: &mut HypaData, current_memos: &HashSet<String>) {
    data.summaries.retain(|summary| {
        summary
            .chat_memos
            .iter()
            .all(|memo| current_memos.contains(memo))
    });
}

pub struct SelectionResult {
    pub selected: Vec<usize>,
    pub important: Vec<usize>,
    pub pinned: Vec<usize>,
    pub recent: Vec<usize>,
    pub similar: Vec<usize>,
    pub random: Vec<usize>,
}

pub fn select_memories(
    data: &HypaData,
    query_embeddings: &[Vec<f64>],
    query_weights: &[f64],
    token_budget: usize,
    estimate_tokens: impl Fn(&str) -> usize,
    settings: &HypaSettings,
) -> SelectionResult {
    let mut used = HashSet::new();
    let mut important = Vec::new();
    let mut pinned = Vec::new();
    let mut recent = Vec::new();
    let mut similar = Vec::new();
    let mut random = Vec::new();
    let mut remaining = token_budget;

    // Phase 0: exclude invalidated summaries from every subsequent phase.
    // Marking them `used` up front keeps the per-phase `used.contains` guards
    // as the single exclusion mechanism — no phase can select an invalidated
    // index.
    for (i, summary) in data.summaries.iter().enumerate() {
        if summary.invalidated {
            used.insert(i);
        }
    }

    // Phase 1: Important summaries — always included while budget allows.
    // (External wire compatible.) Distinct from the pin phase below.
    for (i, summary) in data.summaries.iter().enumerate() {
        if used.contains(&i) || !summary.is_important {
            continue;
        }
        let tokens = estimate_tokens(&summary.text);
        if tokens > remaining {
            break;
        }
        important.push(i);
        used.insert(i);
        remaining -= tokens;
    }

    // Pin phase (Layream extension): pin_boost summaries get a guaranteed slot
    // before the similarity phase can spend the budget, so a pinned summary is
    // never truncated by losing a similarity ranking. This is a SEPARATE bucket
    // from `is_important` (REFACTOR_HYPA note 4 — do not merge). Highest
    // pin_boost first so the most-pinned win when budget is tight.
    let mut pin_candidates: Vec<(usize, f64)> = data
        .summaries
        .iter()
        .enumerate()
        .filter(|(i, s)| !used.contains(i) && s.pin_boost > 0.0)
        .map(|(i, s)| (i, s.pin_boost))
        .collect();
    pin_candidates
        .sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    for (i, _boost) in pin_candidates {
        let tokens = estimate_tokens(&data.summaries[i].text);
        if tokens > remaining {
            continue;
        }
        pinned.push(i);
        used.insert(i);
        remaining -= tokens;
    }

    // Phase 2: Recent summaries
    let recent_budget =
        ((remaining as f64) * settings.recent_memory_ratio) as usize;
    let mut recent_used = 0;
    for i in (0..data.summaries.len()).rev() {
        if used.contains(&i) {
            continue;
        }
        let tokens = estimate_tokens(&data.summaries[i].text);
        if recent_used + tokens > recent_budget {
            break;
        }
        recent.push(i);
        used.insert(i);
        recent_used += tokens;
    }
    let recent_realloc = recent_budget.saturating_sub(recent_used);
    remaining -= recent_used;

    // Phase 3: Similar summaries (cosine similarity + RRF concordance).
    // Embeddings are read directly from `Summary.embedding`; summaries with no
    // embedding (e.g. freshly imported external data) are silently absent from
    // the ranking until their embedding is recomputed.
    let similar_budget =
        ((remaining as f64) * settings.similar_memory_ratio) as usize + recent_realloc;
    let mut similar_used = 0;

    let embedded: Vec<(usize, &Vec<f64>)> = data
        .summaries
        .iter()
        .enumerate()
        .filter(|(i, _)| !used.contains(i))
        .filter_map(|(i, s)| s.embedding.as_ref().map(|e| (i, e)))
        .collect();

    if !query_embeddings.is_empty() && !embedded.is_empty() {
        let mut scores: HashMap<usize, f64> = HashMap::new();

        for (qi, query_emb) in query_embeddings.iter().enumerate() {
            let weight = query_weights.get(qi).copied().unwrap_or(1.0);
            let mut chunk_scores: Vec<(usize, f64)> = embedded
                .iter()
                .map(|(idx, emb)| (*idx, voyage::cosine_similarity(query_emb, emb)))
                .collect();
            chunk_scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

            for (rank, (idx, _score)) in chunk_scores.iter().enumerate() {
                let rrf = 1.0 / (60.0 + rank as f64);
                *scores.entry(*idx).or_insert(0.0) += rrf * weight;
            }
        }

        let mut ranked: Vec<(usize, f64)> = scores.into_iter().collect();
        ranked.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        for (idx, _score) in ranked {
            if used.contains(&idx) || idx >= data.summaries.len() {
                continue;
            }
            let tokens = estimate_tokens(&data.summaries[idx].text);
            if similar_used + tokens > similar_budget {
                continue;
            }
            similar.push(idx);
            used.insert(idx);
            similar_used += tokens;
        }
    }
    let similar_realloc = similar_budget.saturating_sub(similar_used);
    remaining -= similar_used;

    // Phase 4: Random summaries
    let random_budget =
        ((remaining as f64) * settings.random_memory_ratio) as usize + similar_realloc;
    let mut random_used = 0;
    let mut unused_indices: Vec<usize> = (0..data.summaries.len())
        .filter(|i| !used.contains(i))
        .collect();

    use rand::seq::SliceRandom;
    unused_indices.shuffle(&mut rand::rng());

    for idx in unused_indices {
        let tokens = estimate_tokens(&data.summaries[idx].text);
        if random_used + tokens > random_budget {
            continue;
        }
        random.push(idx);
        used.insert(idx);
        random_used += tokens;
    }

    // Sort all selected by chronological order
    let mut selected: Vec<usize> = important
        .iter()
        .chain(pinned.iter())
        .chain(recent.iter())
        .chain(similar.iter())
        .chain(random.iter())
        .copied()
        .collect();
    selected.sort();
    selected.dedup();

    SelectionResult {
        selected,
        important,
        pinned,
        recent,
        similar,
        random,
    }
}

pub fn build_memory_block(data: &HypaData, selected: &[usize]) -> String {
    let texts: Vec<&str> = selected
        .iter()
        .filter_map(|&i| data.summaries.get(i).map(|s| s.text.as_str()))
        .collect();

    if texts.is_empty() {
        return String::new();
    }

    format!(
        "<Past Events Summary>\n{}\n</Past Events Summary>",
        texts.join("\n\n")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_summary(i: usize) -> Summary {
        Summary {
            text: format!("Summary {}", i),
            chat_memos: vec![format!("memo_{}", i)],
            embedding: None,
            is_important: i == 0,
            pin_boost: 0.0,
            invalidated: false,
        }
    }

    fn make_data(n: usize) -> HypaData {
        HypaData {
            summaries: (0..n).map(make_summary).collect(),
            extra: Default::default(),
        }
    }

    #[test]
    fn clean_orphaned_removes_missing() {
        let mut data = make_data(3);
        let current: HashSet<String> = ["memo_0", "memo_2"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        clean_orphaned(&mut data, &current);
        assert_eq!(data.summaries.len(), 2);
        assert_eq!(data.summaries[0].text, "Summary 0");
        assert_eq!(data.summaries[1].text, "Summary 2");
    }

    #[test]
    fn select_important_first() {
        let data = make_data(5);
        let result = select_memories(
            &data,
            &[],
            &[],
            1000,
            |s| s.len(),
            &HypaSettings::default(),
        );
        assert!(result.important.contains(&0));
    }

    #[test]
    fn phase0_excludes_invalidated_from_all_phases() {
        // Index 0 is important AND invalidated → invalidation wins; index 2 is
        // pinned AND invalidated → also excluded. Neither may appear anywhere.
        let mut data = make_data(4);
        data.summaries[0].invalidated = true; // also is_important (i == 0)
        data.summaries[2].pin_boost = 1.0;
        data.summaries[2].invalidated = true;
        let result = select_memories(
            &data,
            &[],
            &[],
            10_000,
            |s| s.len(),
            &HypaSettings::default(),
        );
        assert!(!result.selected.contains(&0), "invalidated+important excluded");
        assert!(!result.selected.contains(&2), "invalidated+pinned excluded");
        assert!(!result.important.contains(&0));
        assert!(!result.pinned.contains(&2));
    }

    #[test]
    fn pin_is_separate_bucket_from_important() {
        // Pinned-but-not-important → `pinned` only; important-but-not-pinned →
        // `important` only. Buckets disjoint (REFACTOR_HYPA note 4).
        let mut data = make_data(3);
        data.summaries[2].pin_boost = 0.5; // index 0 is important via make_summary
        let result = select_memories(
            &data,
            &[],
            &[],
            10_000,
            |s| s.len(),
            &HypaSettings::default(),
        );
        assert!(result.important.contains(&0));
        assert!(!result.pinned.contains(&0));
        assert!(result.pinned.contains(&2));
        assert!(!result.important.contains(&2));
        assert!(result.important.iter().all(|i| !result.pinned.contains(i)));
    }

    #[test]
    fn pinned_survives_tight_budget_over_similar() {
        // Budget fits exactly one 4-char summary. The pin phase claims it before
        // the similarity phase can — pin is guaranteed, not truncated.
        let data = HypaData {
            summaries: vec![
                Summary {
                    text: "aaaa".into(),
                    chat_memos: vec!["m_sim".into()],
                    embedding: Some(vec![1.0, 0.0]),
                    is_important: false,
                    pin_boost: 0.0,
                    invalidated: false,
                },
                Summary {
                    text: "bbbb".into(),
                    chat_memos: vec!["m_pin".into()],
                    embedding: Some(vec![0.0, 1.0]),
                    is_important: false,
                    pin_boost: 1.0,
                    invalidated: false,
                },
            ],
            extra: Default::default(),
        };
        // Query aligned with index 0 (unpinned) — similarity would prefer it,
        // but the only slot is already spent on the pinned summary.
        let query = vec![vec![1.0, 0.0]];
        let result = select_memories(
            &data,
            &query,
            &[1.0],
            4,
            |s| s.len(),
            &HypaSettings {
                recent_memory_ratio: 0.0,
                similar_memory_ratio: 1.0,
                random_memory_ratio: 0.0,
                ..HypaSettings::default()
            },
        );
        assert!(result.pinned.contains(&1), "pinned summary guaranteed");
        assert!(!result.similar.contains(&0), "no budget left for similar");
    }

    #[test]
    fn empty_query_runs_non_similar_phases() {
        // D2 graceful degradation: when no query embedding is available (e.g.
        // assemblePrompt called without ChatView's precomputed embedding), the
        // similar phase is skipped but important/pinned/recent/random still run.
        // Index 0 is important (make_summary), index 2 is pinned; both carry
        // embeddings, yet with no query they must still be selected via their
        // own phases — proving similarity is not a precondition for selection.
        let mut data = make_data(4);
        data.summaries[0].embedding = Some(vec![1.0, 0.0]);
        data.summaries[2].pin_boost = 1.0;
        data.summaries[2].embedding = Some(vec![0.0, 1.0]);
        let result = select_memories(
            &data,
            &[], // no query embeddings
            &[],
            10_000,
            |s| s.len(),
            &HypaSettings::default(),
        );
        assert!(result.similar.is_empty(), "no query \u{2192} no similar phase");
        assert!(result.important.contains(&0), "important still selected");
        assert!(result.pinned.contains(&2), "pinned still selected");
        assert!(result.selected.contains(&0));
        assert!(result.selected.contains(&2));
    }

    #[test]
    fn summary_serialization_uses_camelcase_keys() {
        let s = Summary {
            text: "hello".into(),
            chat_memos: vec!["a".into(), "b".into()],
            embedding: Some(vec![0.1, 0.2]),
            is_important: true,
            pin_boost: 0.5,
            invalidated: false,
        };
        let v = serde_json::to_value(&s).unwrap();
        assert!(v.get("chatMemos").is_some(), "expected chatMemos key");
        assert!(v.get("isImportant").is_some(), "expected isImportant key");
        assert!(v.get("pinBoost").is_some(), "expected pinBoost key");
        assert!(v.get("chat_memos").is_none());
        assert!(v.get("is_important").is_none());
        assert!(v.get("pin_boost").is_none());
    }

    #[test]
    fn summary_deserialization_accepts_external_shape() {
        // External minimum shape: no embedding / pinBoost / invalidated.
        let raw = r#"{"text":"abc","chatMemos":["m1"],"isImportant":true}"#;
        let s: Summary = serde_json::from_str(raw).unwrap();
        assert_eq!(s.text, "abc");
        assert_eq!(s.chat_memos, vec!["m1".to_string()]);
        assert!(s.is_important);
        assert_eq!(s.pin_boost, 0.0);
        assert!(!s.invalidated);
        assert!(s.embedding.is_none());
    }

    #[test]
    fn summary_embedding_roundtrips_when_present() {
        let with_emb = Summary {
            embedding: Some(vec![0.5, 0.25]),
            ..make_summary(0)
        };
        let v = serde_json::to_value(&with_emb).unwrap();
        assert!(v.get("embedding").is_some());
        let back: Summary = serde_json::from_value(v).unwrap();
        assert_eq!(back.embedding, Some(vec![0.5, 0.25]));
    }

    #[test]
    fn hypa_data_roundtrip_preserves_extra_fields() {
        // External metrics / modalSettings must survive an import→export cycle.
        let raw = serde_json::json!({
            "summaries": [],
            "metrics": { "count": 3 },
            "modalSettings": { "foo": "bar" }
        });
        let data: HypaData = serde_json::from_value(raw.clone()).unwrap();
        let back = serde_json::to_value(&data).unwrap();
        assert_eq!(back.get("metrics"), raw.get("metrics"));
        assert_eq!(back.get("modalSettings"), raw.get("modalSettings"));
    }

    #[test]
    fn settings_deserialize_tolerates_missing_random_ratio() {
        // External presets omit randomMemoryRatio (Layream-internal) → must default.
        let raw = serde_json::json!({
            "memoryTokensRatio": 0.3,
            "recentMemoryRatio": 0.4,
            "similarMemoryRatio": 0.6,
            "maxChatsPerSummary": 8,
            "preserveOrphanedMemory": true
        });
        let s: HypaSettings = serde_json::from_value(raw).unwrap();
        assert_eq!(s.memory_tokens_ratio, 0.3);
        assert_eq!(s.max_chats_per_summary, 8);
        assert!(s.preserve_orphaned_memory);
        assert_eq!(
            s.random_memory_ratio,
            HypaSettings::default().random_memory_ratio
        );
    }

    #[test]
    fn build_memory_block_format() {
        let data = make_data(3);
        let block = build_memory_block(&data, &[0, 2]);
        assert!(block.contains("<Past Events Summary>"));
        assert!(block.contains("Summary 0"));
        assert!(block.contains("Summary 2"));
        assert!(!block.contains("Summary 1"));
    }

    #[test]
    fn empty_selection() {
        let data = HypaData::default();
        let block = build_memory_block(&data, &[]);
        assert!(block.is_empty());
    }
}
