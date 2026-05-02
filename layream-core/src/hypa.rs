use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::voyage;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Summary {
    pub text: String,
    pub chat_memos: Vec<String>,
    pub is_important: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HypaData {
    pub summaries: Vec<Summary>,
}

impl Default for HypaData {
    fn default() -> Self {
        Self {
            summaries: Vec::new(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct HypaSettings {
    pub memory_tokens_ratio: f64,
    pub recent_memory_ratio: f64,
    pub similar_memory_ratio: f64,
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

#[derive(Debug, Clone)]
pub struct EmbeddedSummary {
    pub index: usize,
    pub text: String,
    pub embedding: Vec<f64>,
    pub is_important: bool,
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
    pub recent: Vec<usize>,
    pub similar: Vec<usize>,
    pub random: Vec<usize>,
}

pub fn select_memories(
    data: &HypaData,
    embedded: &[EmbeddedSummary],
    query_embeddings: &[Vec<f64>],
    query_weights: &[f64],
    token_budget: usize,
    estimate_tokens: impl Fn(&str) -> usize,
    settings: &HypaSettings,
) -> SelectionResult {
    let mut used = HashSet::new();
    let mut important = Vec::new();
    let mut recent = Vec::new();
    let mut similar = Vec::new();
    let mut random = Vec::new();
    let mut remaining = token_budget;

    // Phase 1: Important summaries
    for (i, summary) in data.summaries.iter().enumerate() {
        if !summary.is_important {
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

    // Phase 3: Similar summaries (cosine similarity + composite concordance)
    let similar_budget =
        ((remaining as f64) * settings.similar_memory_ratio) as usize + recent_realloc;
    let mut similar_used = 0;

    if !query_embeddings.is_empty() && !embedded.is_empty() {
        let mut scores: HashMap<usize, f64> = HashMap::new();

        for (qi, query_emb) in query_embeddings.iter().enumerate() {
            let weight = query_weights.get(qi).copied().unwrap_or(1.0);
            let mut chunk_scores: Vec<(usize, f64)> = embedded
                .iter()
                .filter(|e| !used.contains(&e.index))
                .map(|e| (e.index, voyage::cosine_similarity(query_emb, &e.embedding)))
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

    fn make_data(n: usize) -> HypaData {
        HypaData {
            summaries: (0..n)
                .map(|i| Summary {
                    text: format!("Summary {}", i),
                    chat_memos: vec![format!("memo_{}", i)],
                    is_important: i == 0,
                })
                .collect(),
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
            &[],
            1000,
            |s| s.len(),
            &HypaSettings::default(),
        );
        assert!(result.important.contains(&0));
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
