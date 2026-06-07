//! HyPA backend — hierarchical prompt augmentation commands.
//!
//! Wire format uses camelCase keys (`chatMemos`, `isImportant` preserved via
//! serde rename) for interoperability with external preset/memory exports, plus
//! Layream extensions for `pinBoost` and `invalidated`.
//!
//! Embeddings are stored as `Vec<f64>` to match the JSON wire format (JSON numbers
//! decode to f64 by default) and to compose with `layream_core::voyage::cosine_similarity`
//! without precision conversion. This is intentional — see §3-A (no demote/promote).

use layream_core::gca::{self, GCA_OAUTH_CLIENT_ID};
use layream_core::hypa::{self, HypaData, HypaSettings, Summary};
use layream_core::mistral;
use layream_core::vertex_api::{
    self, Content, GenerateRequest, GenerationConfig, Part, SafetySetting,
};
use layream_core::vertex_auth::{
    self, OAuthCredentials, LAYREAM_REDIRECT_URI, VERTEX_CLIENT_ID,
};
use layream_core::voyage;
use serde_json::Value;
use tauri::State;
use tokio::sync::Mutex;

use crate::commands_auth::AuthState;
use crate::persistence;

// === Concurrency state ===
//
// All hypa_* commands that read or write hypa.json acquire this lock at the
// start of their critical section. This serializes load-mutate-save sequences
// across concurrent invocations (e.g. pin + invalidate fired together) so the
// second writer cannot overwrite the first writer's update.
//
// Read-only commands (`hypa_load_all`, `hypa_search`) also acquire the lock to
// guarantee read-after-write consistency — a frontend that just awaited
// `hypa_pin_message` must observe the pin in a subsequent read.
//
// `tokio::sync::Mutex` (not std::sync::Mutex) is required because the guard
// is held across `.await` points in async commands.
pub struct HypaState {
    pub lock: Mutex<()>,
}

impl Default for HypaState {
    fn default() -> Self {
        Self {
            lock: Mutex::new(()),
        }
    }
}

// === Constants (§9-B: no magic numbers) ===

/// Increment applied to `pinBoost` when a chat message is pinned/unpinned.
const PIN_BOOST_INCREMENT: f64 = 0.5;

/// The body-independent overhead of `layream_core::hypa::build_memory_block`'s
/// wrapper. MUST mirror that function's `format!` string with an empty body —
/// it produces `<Past Events Summary>\n{body}\n</Past Events Summary>`, so the
/// fixed cost is the same string with `{body}` empty. `hypa_select_block`
/// reserves this many tokens before budgeting summaries, so the wrapped block
/// stays within the caller's token budget (REFACTOR_HYPA §선택동작세부). If the
/// wrapper format in `build_memory_block` changes, update this string too —
/// they are the one fact that must agree (§4.1).
const MEMORY_BLOCK_WRAPPER: &str = "<Past Events Summary>\n\n</Past Events Summary>";

/// Default safety settings — block none. Mirrors `commands.rs::build_safety_settings`.
fn build_safety_settings() -> Vec<SafetySetting> {
    [
        "HARM_CATEGORY_HARASSMENT",
        "HARM_CATEGORY_HATE_SPEECH",
        "HARM_CATEGORY_SEXUALLY_EXPLICIT",
        "HARM_CATEGORY_DANGEROUS_CONTENT",
        "HARM_CATEGORY_CIVIC_INTEGRITY",
    ]
    .iter()
    .map(|cat| SafetySetting {
        category: cat.to_string(),
        threshold: "BLOCK_NONE".to_string(),
    })
    .collect()
}

// === Schema ===
//
// `Summary` / `HypaData` / `HypaSettings` live in `layream_core::hypa` — the
// single definition site (§4.1) shared by the core selection pipeline
// (`select_memories`, `build_memory_block`) and this Tauri command layer. The
// duplicate definitions that once lived here were deleted; `use` above is the
// only binding. Wire format uses camelCase keys (`chatMemos` / `isImportant` /
// `pinBoost`) for external interoperability, with the `embedding` /
// `invalidated` extensions.

// === Internal helpers ===

/// Read hypa.json from disk into `HypaData`. Returns default if file missing.
fn load_hypa_data(app: &tauri::AppHandle) -> Result<HypaData, String> {
    let data_dir = persistence::get_data_dir(app)?;
    let raw = persistence::load_hypa(&data_dir)?;
    serde_json::from_value(raw).map_err(|e| format!("hypa.json parse error: {}", e))
}

/// Persist `HypaData` to hypa.json (pretty-printed via persistence::save_hypa).
fn save_hypa_data(app: &tauri::AppHandle, data: &HypaData) -> Result<(), String> {
    let data_dir = persistence::get_data_dir(app)?;
    let value = serde_json::to_value(data).map_err(|e| e.to_string())?;
    persistence::save_hypa(&data_dir, &value)
}

/// Extract a string field from settings, returning a typed error on absence.
fn settings_str<'a>(settings: &'a Value, key: &str) -> Result<&'a str, String> {
    settings
        .get(key)
        .and_then(|v| v.as_str())
        .ok_or_else(|| format!("settings.{} required (string)", key))
}

/// Extract an optional string field.
fn settings_str_opt<'a>(settings: &'a Value, key: &str) -> Option<&'a str> {
    settings.get(key).and_then(|v| v.as_str())
}

/// Token estimate for budgeting. Mirrors the frontend `trimToContextWindow`
/// idiom (`chars * 4` for the inverse) and `commands_chat.rs` logging
/// (`len / 4`): ~4 chars per token. Stated explicitly here (§1.4) so the
/// budget math in `select_memories` callers has one named convention.
fn estimate_tokens(text: &str) -> usize {
    text.len() / 4
}

/// Map a frontend HyPA settings JSON object onto `HypaSettings`.
///
/// `HypaSettings` is `#[serde(rename_all = "camelCase", default)]`, so this
/// deserializes the external/Layream camelCase keys directly and fills any
/// omitted key from `Default` — a preset that carries only a subset of ratios
/// still maps cleanly (§1.1). Unknown keys in `settings` (provider/model/etc.)
/// are ignored by serde. A malformed value (e.g. a string where a ratio is
/// expected) surfaces as an error rather than a silent default (§5.1).
fn hypa_settings_from_value(settings: &Value) -> Result<HypaSettings, String> {
    serde_json::from_value(settings.clone())
        .map_err(|e| format!("invalid HyPA settings: {}", e))
}

/// Refresh a token if expired, persist if rotated. Returns the valid token.
async fn refresh_token(
    client: &reqwest::Client,
    creds: &OAuthCredentials,
    state: &State<'_, AuthState>,
    is_gca: bool,
    app: &tauri::AppHandle,
) -> Result<vertex_auth::Tokens, String> {
    let current = if is_gca {
        state
            .gca_tokens
            .lock()
            .map_err(|e| format!("lock poisoned: {e}"))?
            .clone()
            .ok_or("GCA not connected")?
    } else {
        state
            .vertex_tokens
            .lock()
            .map_err(|e| format!("lock poisoned: {e}"))?
            .clone()
            .ok_or("Vertex AI not connected")?
    };

    let valid = vertex_auth::get_valid_token(client, creds, &current)
        .await
        .map_err(|e| e.to_string())?;

    if valid.access_token != current.access_token {
        if is_gca {
            *state.gca_tokens.lock().map_err(|e| format!("lock poisoned: {e}"))? = Some(valid.clone());
        } else {
            *state.vertex_tokens.lock().map_err(|e| format!("lock poisoned: {e}"))? = Some(valid.clone());
        }
        state.persist_tokens(app);
    }
    Ok(valid)
}

/// Invoke a provider with a non-streaming request. Returns the generated text.
///
/// Provider selection from settings:
/// - `provider`: "vertex" | "gca" | "mistral" (required)
/// - `model`: model id (required)
/// - `projectId`, `region`: required when provider == "vertex"
/// - `apiKey`: required when provider == "mistral"
/// - `temperature`, `maxTokens`, `topP`, `topK`: optional generation config
async fn invoke_provider(
    settings: &Value,
    contents: Vec<Content>,
    state: &State<'_, AuthState>,
    app: &tauri::AppHandle,
) -> Result<String, String> {
    let provider = settings_str(settings, "provider")?;
    let model = settings_str(settings, "model")?;

    let temperature = settings
        .get("temperature")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.3);
    let max_tokens = settings
        .get("maxTokens")
        .and_then(|v| v.as_u64())
        .map(|v| v as u32)
        .unwrap_or(2048);
    let top_p = settings.get("topP").and_then(|v| v.as_f64());
    let top_k = settings
        .get("topK")
        .and_then(|v| v.as_u64())
        .map(|v| v as u32);

    let gen_config = GenerationConfig {
        max_output_tokens: max_tokens,
        temperature,
        thinking_config: None,
        top_p,
        top_k,
        frequency_penalty: None,
        presence_penalty: None,
        response_mime_type: None,
        response_schema: None,
    };

    let client = reqwest::Client::new();

    match provider {
        "vertex" => {
            let request = GenerateRequest {
                contents,
                system_instruction: None,
                safety_settings: build_safety_settings(),
                generation_config: gen_config,
                tools: None,
            };
            let project_id = settings_str(settings, "projectId")?;
            let region = settings_str(settings, "region")?;
            let creds = OAuthCredentials {
                client_id: VERTEX_CLIENT_ID.to_string(),
                client_secret: None,
                redirect_uri: LAYREAM_REDIRECT_URI.to_string(),
            };
            let tokens = refresh_token(&client, &creds, state, false, app).await?;
            vertex_api::generate_non_streaming(
                &client,
                &tokens.access_token,
                project_id,
                region,
                model,
                &request,
            )
            .await
            .map_err(|e| e.to_string())
        }
        "gca" => {
            let request = GenerateRequest {
                contents,
                system_instruction: None,
                safety_settings: build_safety_settings(),
                generation_config: gen_config,
                tools: None,
            };
            let creds = OAuthCredentials {
                client_id: GCA_OAUTH_CLIENT_ID.to_string(),
                client_secret: None,
                redirect_uri: LAYREAM_REDIRECT_URI.to_string(),
            };
            let tokens = refresh_token(&client, &creds, state, true, app).await?;
            gca::generate_non_streaming(&client, &tokens.access_token, model, None, &request)
                .await
                .map_err(|e| e.to_string())
        }
        "mistral" => {
            let api_key = settings_str(settings, "apiKey")?;

            // Convert Vertex Content → Mistral ChatMessage. Each Content has
            // role + parts[].text; we concatenate part texts (hypa only ever
            // sends a single part per content, but concatenation is lossless).
            let messages: Vec<mistral::ChatMessage> = contents
                .iter()
                .map(|c| mistral::ChatMessage {
                    role: c.role.clone(),
                    content: c
                        .parts
                        .iter()
                        .filter_map(|p| p.text.as_deref())
                        .collect::<Vec<_>>()
                        .join(""),
                    tool_calls: None,
                    tool_call_id: None,
                })
                .collect();

            let mistral_request = mistral::ChatRequest {
                model: model.to_string(),
                messages,
                temperature: Some(temperature),
                top_p,
                max_tokens: Some(max_tokens),
                stream: Some(false),
                frequency_penalty: None,
                presence_penalty: None,
                stop: None,
                random_seed: None,
                response_format: None,
                reasoning_effort: None,
                tools: None,
                tool_choice: None,
            };

            let response = mistral::chat(&client, api_key, &mistral_request)
                .await
                .map_err(|e| e.to_string())?;

            response
                .choices
                .first()
                .and_then(|c| c.message.as_ref())
                .map(|m| m.content.clone())
                .ok_or_else(|| "No response content from Mistral".to_string())
        }
        other => Err(format!(
            "settings.provider must be 'vertex', 'gca', or 'mistral', got '{}'",
            other
        )),
    }
}

/// Compute an embedding for a single text via Vertex embedding API.
///
/// Embedding settings (separate from chat provider):
/// - `embedProvider`: "vertex" (only supported here; voyage requires API key, see hypa_search caller)
/// - `embedModel`: model id (e.g. "text-embedding-004")
/// - `embedProjectId`, `embedRegion`: vertex project/region for embeddings
///
/// If any embedding field is missing, returns Ok(None) — caller decides whether
/// to skip embedding or error out.
async fn embed_text_vertex(
    text: &str,
    settings: &Value,
    state: &State<'_, AuthState>,
    app: &tauri::AppHandle,
) -> Result<Option<Vec<f64>>, String> {
    let model = match settings_str_opt(settings, "embedModel") {
        Some(m) => m,
        None => return Ok(None),
    };
    let project_id = match settings_str_opt(settings, "embedProjectId") {
        Some(p) => p,
        None => return Ok(None),
    };
    let region = match settings_str_opt(settings, "embedRegion") {
        Some(r) => r,
        None => return Ok(None),
    };

    let client = reqwest::Client::new();
    let creds = OAuthCredentials {
        client_id: VERTEX_CLIENT_ID.to_string(),
        client_secret: None,
        redirect_uri: LAYREAM_REDIRECT_URI.to_string(),
    };
    let tokens = refresh_token(&client, &creds, state, false, app).await?;

    let mut embeddings = vertex_api::batch_embed_contents(
        &client,
        &tokens.access_token,
        project_id,
        region,
        model,
        &[text],
    )
    .await
    .map_err(|e| e.to_string())?;

    Ok(embeddings.pop())
}

// === Commands ===

/// Generate a summary from a window of messages.
///
/// Caller is responsible for persisting the returned `Summary` (does not write
/// hypa.json — see `hypa_save_all`). This separation lets the frontend assign
/// `chatMemos` from its own message ids before saving.
#[tauri::command(rename_all = "snake_case")]
pub async fn hypa_summarize(
    messages: Vec<Value>,
    settings: Value,
    state: State<'_, AuthState>,
    hypa_state: State<'_, HypaState>,
    app: tauri::AppHandle,
) -> Result<Value, String> {
    // Lock not strictly required (no hypa.json write here — caller persists via
    // hypa_save_all), but acquired for read-after-write consistency with any
    // ongoing mutation: a summary just generated should reflect any concurrent
    // pin/invalidate when the caller subsequently saves.
    let _guard = hypa_state.lock.lock().await;

    if messages.is_empty() {
        return Err("hypa_summarize: messages must be non-empty".into());
    }

    let summary_prompt = settings_str(&settings, "summaryPrompt")?;

    // Build prompt: prepend system instruction as the first user-role content.
    // The model is prompted with the conversation followed by an instruction.
    // We use a single user message containing the instruction + serialized
    // window (kept simple — §6-D).
    let mut serialized = String::new();
    for m in &messages {
        let role = m.get("role").and_then(|v| v.as_str()).unwrap_or("user");
        let text = m.get("text").and_then(|v| v.as_str()).unwrap_or("");
        serialized.push_str(&format!("{}: {}\n", role, text));
    }
    let prompt = format!("{}\n\n<conversation>\n{}</conversation>", summary_prompt, serialized);

    let contents = vec![Content {
        role: "user".to_string(),
        parts: vec![Part {
            text: Some(prompt),
            thought: None,
            inline_data: None,
        }],
    }];

    let raw = invoke_provider(&settings, contents, &state, &app).await?;

    // Parse — accept JSON object with `summary` field, otherwise treat raw as text.
    let summary_text = parse_summary_text(&raw);

    // Compute embedding (best-effort — None if embed settings not provided).
    let embedding = embed_text_vertex(&summary_text, &settings, &state, &app).await?;

    let summary = Summary {
        text: summary_text,
        chat_memos: Vec::new(),
        embedding,
        is_important: false,
        pin_boost: 0.0,
        invalidated: false,
    };

    serde_json::to_value(&summary).map_err(|e| e.to_string())
}

/// Parse model output as either a JSON object with `summary` field or raw text.
fn parse_summary_text(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    if let Ok(v) = serde_json::from_str::<Value>(trimmed) {
        if let Some(s) = v.get("summary").and_then(|s| s.as_str()) {
            return s.to_string();
        }
    }
    // Try slicing first '{' to last '}'.
    if let (Some(start), Some(end)) = (trimmed.find('{'), trimmed.rfind('}')) {
        if end > start {
            if let Ok(v) = serde_json::from_str::<Value>(&trimmed[start..=end]) {
                if let Some(s) = v.get("summary").and_then(|s| s.as_str()) {
                    return s.to_string();
                }
            }
        }
    }
    trimmed.to_string()
}

/// Per-`top_k` token allowance used by the `hypa_search` top_k↔token_budget
/// adapter. `select_memories` budgets in tokens, not item counts; this maps a
/// caller's requested `top_k` onto a token budget large enough to admit roughly
/// that many summaries. Stated explicitly (§1.4) — it is the single named
/// conversion between the two units. Generous on purpose: a slightly larger
/// budget lets `select_memories` rank freely, then the result is truncated back
/// to `top_k` so the contract (`top_k` items, score-sorted) holds exactly.
const SEARCH_TOKENS_PER_TOP_K: usize = 512;

/// Similarity search over stored summaries, routed through the core 4-phase
/// `select_memories` pipeline (important → pinned → recent → similar → random),
/// then truncated to `top_k`.
///
/// Adapter: `token_budget = top_k * SEARCH_TOKENS_PER_TOP_K`. `select_memories`
/// is the selection authority (external wire compatible); this command no
/// longer runs its own 1-phase cosine ranking. For the frontend contract the
/// per-item `score` is recomputed as `cosine(query, embedding) + pin_boost`
/// over the selected summaries — same formula the old 1-phase path returned, so
/// `{ index, score, summary }` is preserved and the result stays score-sorted.
#[tauri::command(rename_all = "snake_case")]
pub async fn hypa_search(
    query_embedding: Vec<f64>,
    top_k: usize,
    hypa_state: State<'_, HypaState>,
    app: tauri::AppHandle,
) -> Result<Value, String> {
    // Read-only — lock acquired for read-after-write consistency only.
    let _guard = hypa_state.lock.lock().await;

    if query_embedding.is_empty() {
        return Err("hypa_search: query_embedding must be non-empty".into());
    }
    if top_k == 0 {
        return Ok(Value::Array(Vec::new()));
    }

    let data = load_hypa_data(&app)?;

    // top_k → token_budget adapter (the only place the two units meet).
    let token_budget = top_k.saturating_mul(SEARCH_TOKENS_PER_TOP_K);

    // Single query → single-element query_embeddings with unit weight. The
    // similar phase reads embeddings directly off each Summary; a summary whose
    // embedding dimension mismatches the query simply contributes cosine 0.0
    // here at score time (it can still be selected by another phase).
    let selection = hypa::select_memories(
        &data,
        std::slice::from_ref(&query_embedding),
        &[1.0],
        token_budget,
        estimate_tokens,
        // hypa_search takes no settings arg (the search top_k is the only knob);
        // default ratios drive the internal phase budgets.
        &HypaSettings::default(),
    );

    // Recompute display scores over the selected summaries (contract: score
    // desc). cosine is defined only when dimensions match — otherwise the
    // similarity contribution is 0.0 (pin_boost still counts), mirroring the
    // old path's skip-on-mismatch without dropping a summary the pipeline
    // already chose.
    let mut scored: Vec<(usize, f64)> = selection
        .selected
        .iter()
        .map(|&i| {
            let s = &data.summaries[i];
            let cos = match s.embedding.as_ref() {
                Some(emb) if emb.len() == query_embedding.len() => {
                    voyage::cosine_similarity(&query_embedding, emb)
                }
                _ => 0.0,
            };
            (i, cos + s.pin_boost)
        })
        .collect();

    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    scored.truncate(top_k);

    let result: Vec<Value> = scored
        .into_iter()
        .map(|(i, score)| {
            serde_json::json!({
                "index": i,
                "score": score,
                "summary": data.summaries[i],
            })
        })
        .collect();

    Ok(Value::Array(result))
}

/// Build the `<Past Events Summary>` memory block via the core 4-phase
/// `select_memories` pipeline. Replaces the assemblePrompt full-dump path.
///
/// Args:
/// - `query_embeddings` / `query_weights`: conversation query vectors. EMPTY is
///   valid — `select_memories` then skips the similar phase and still runs
///   important / pinned / recent / random (D2 graceful degradation). Callers
///   without an embedding (autopilot, non-chat) pass `[]`.
/// - `token_budget`: caller-computed budget (`floor(maxContext * memoryTokensRatio)`
///   minus wrapper cost). The frontend owns maxContext, so it is passed in.
/// - `settings`: HyPA settings JSON (camelCase ratios) → `HypaSettings`.
///
/// Returns the block string (empty when nothing is selected).
#[tauri::command(rename_all = "snake_case")]
pub async fn hypa_select_block(
    query_embeddings: Vec<Vec<f64>>,
    query_weights: Vec<f64>,
    token_budget: usize,
    settings: Value,
    hypa_state: State<'_, HypaState>,
    app: tauri::AppHandle,
) -> Result<String, String> {
    // Read-only — lock acquired for read-after-write consistency only.
    let _guard = hypa_state.lock.lock().await;

    if token_budget == 0 {
        return Ok(String::new());
    }

    let data = load_hypa_data(&app)?;
    let hypa_settings = hypa_settings_from_value(&settings)?;

    // Reserve the wrapper cost so the wrapped block fits the caller's budget:
    // build_memory_block adds `<Past Events Summary>…</Past Events Summary>`
    // after selection, which select_memories does not account for on its own.
    // saturating_sub: a budget smaller than the wrapper yields 0 → nothing is
    // selected → build_memory_block returns "" (no wrapper emitted), so we
    // never overshoot.
    let summary_budget = token_budget.saturating_sub(estimate_tokens(MEMORY_BLOCK_WRAPPER));

    let selection = hypa::select_memories(
        &data,
        &query_embeddings,
        &query_weights,
        summary_budget,
        estimate_tokens,
        &hypa_settings,
    );

    Ok(hypa::build_memory_block(&data, &selection.selected))
}

/// Toggle pin on a chat message — adjusts `pinBoost` for every summary covering it.
#[tauri::command(rename_all = "snake_case")]
pub async fn hypa_pin_message(
    chat_id: String,
    is_pinned: bool,
    hypa_state: State<'_, HypaState>,
    app: tauri::AppHandle,
) -> Result<u32, String> {
    // Critical section: load → mutate → save. Without this lock, two pin
    // toggles fired together both load the same state and the second save
    // overwrites the first.
    let _guard = hypa_state.lock.lock().await;

    let mut data = load_hypa_data(&app)?;
    let mut affected: u32 = 0;

    for s in data.summaries.iter_mut() {
        if !s.chat_memos.iter().any(|m| m == &chat_id) {
            continue;
        }
        if s.invalidated {
            // Pinning an invalidated summary is a no-op (invalidated summaries
            // are excluded from selection).
            continue;
        }
        if is_pinned {
            s.pin_boost += PIN_BOOST_INCREMENT;
        } else {
            s.pin_boost = (s.pin_boost - PIN_BOOST_INCREMENT).max(0.0);
        }
        affected += 1;
    }

    save_hypa_data(&app, &data)?;
    Ok(affected)
}

/// Set the `isImportant` flag on a single summary by index.
///
/// Atomic load → mutate-one-field → save under the `HypaState` lock — the same
/// pattern as `hypa_pin_message`. This exists instead of routing the toggle
/// through `hypa_save_all` because `hypa_save_all` writes a *whole-array*
/// snapshot the frontend computed earlier; a concurrent `hypa_pin_message` /
/// `hypa_invalidate_summary` (which mutate disk directly) would be clobbered by
/// that stale snapshot (§1.1). A single-field update preserves the other
/// summaries' on-disk state, including pins/invalidation set concurrently.
///
/// `is_important` (external-compat, always-include in Phase 1) is distinct from
/// `pin_boost` (Layream message-pin, guaranteed budget) — this command touches
/// only the former.
///
/// Returns the resulting flag value (echoes `is_important`) so the caller can
/// confirm. Errors if `index` is out of range — silently ignoring a bad index
/// would let the frontend believe a toggle landed when it did not (§5.1).
#[tauri::command(rename_all = "snake_case")]
pub async fn hypa_toggle_important(
    index: usize,
    is_important: bool,
    hypa_state: State<'_, HypaState>,
    app: tauri::AppHandle,
) -> Result<bool, String> {
    // Critical section: load → mutate → save (see hypa_pin_message).
    let _guard = hypa_state.lock.lock().await;

    let mut data = load_hypa_data(&app)?;

    let len = data.summaries.len();
    let summary = data.summaries.get_mut(index).ok_or_else(|| {
        format!("hypa_toggle_important: index {} out of range ({} summaries)", index, len)
    })?;
    summary.is_important = is_important;

    save_hypa_data(&app, &data)?;
    Ok(is_important)
}

/// Mark every summary covering `chat_id` as invalidated. Applied as a
/// delete-message cascade.
///
/// Soft-delete semantics: `chatMemos` is **not** modified — the audit trail
/// (which messages this summary used to cover) is preserved even after those
/// messages are deleted from the chat. This is intentional and load-bearing
/// for two reasons:
///   1. It distinguishes "summary covers no messages" (clean state) from
///      "summary covered messages that were deleted" (invalidated state).
///      `hypa_cleanup` only removes the former.
///   2. A summary covering exactly one (now-deleted) message would otherwise
///      be silently swept by `hypa_cleanup`, defeating invalidation as a
///      durable record.
///
/// Filtering of invalidated summaries happens at read time:
///   - `hypa_search` / `hypa_select_block` skip them because both route through
///     `select_memories`, whose Phase 0 excludes every invalidated summary from
///     all subsequent phases.
///   - `hypa_pin_message` skips them via the `if s.invalidated { continue; }`
///     guard, so pinning a deleted message is a no-op (the audit trail
///     `chatMemos` still matches `chat_id`, but the invalidated flag wins).
#[tauri::command(rename_all = "snake_case")]
pub async fn hypa_invalidate_summary(
    chat_id: String,
    hypa_state: State<'_, HypaState>,
    app: tauri::AppHandle,
) -> Result<u32, String> {
    // Critical section: load → mutate → save (see hypa_pin_message).
    let _guard = hypa_state.lock.lock().await;

    let mut data = load_hypa_data(&app)?;
    let mut affected: u32 = 0;

    for s in data.summaries.iter_mut() {
        if s.invalidated {
            continue;
        }
        if s.chat_memos.iter().any(|m| m == &chat_id) {
            s.invalidated = true;
            affected += 1;
        }
    }

    save_hypa_data(&app, &data)?;
    Ok(affected)
}

/// Drop summaries whose `chatMemos` is empty. Returns the number deleted.
///
/// Note: invalidated summaries are **not** dropped here — they retain their
/// `chatMemos` (audit trail of covered messages) per `hypa_invalidate_summary`
/// soft-delete semantics. Cleanup targets only summaries whose `chatMemos`
/// was never populated (e.g. a summarize call that was rolled back before
/// chat ids were assigned).
#[tauri::command(rename_all = "snake_case")]
pub async fn hypa_cleanup(
    hypa_state: State<'_, HypaState>,
    app: tauri::AppHandle,
) -> Result<u32, String> {
    // Critical section: load → mutate → save.
    let _guard = hypa_state.lock.lock().await;

    let mut data = load_hypa_data(&app)?;
    let before = data.summaries.len();
    data.summaries.retain(|s| !s.chat_memos.is_empty());
    let deleted = (before - data.summaries.len()) as u32;
    save_hypa_data(&app, &data)?;
    Ok(deleted)
}

/// Load all of hypa.json as raw JSON. Returns `{ "summaries": [] }` if file missing.
#[tauri::command(rename_all = "snake_case")]
pub async fn hypa_load_all(
    hypa_state: State<'_, HypaState>,
    app: tauri::AppHandle,
) -> Result<Value, String> {
    // Read-only — lock acquired for read-after-write consistency only.
    let _guard = hypa_state.lock.lock().await;

    let data_dir = persistence::get_data_dir(&app)?;
    persistence::load_hypa(&data_dir)
}

/// Save the entire hypa.json. Pretty-printed (atomic write — see persistence::save_hypa).
#[tauri::command(rename_all = "snake_case")]
pub async fn hypa_save_all(
    summaries: Value,
    hypa_state: State<'_, HypaState>,
    app: tauri::AppHandle,
) -> Result<(), String> {
    // Critical section: this is a wholesale replace. Without the lock, an
    // interleaved pin/invalidate could be silently overwritten by a stale
    // save_all snapshot the frontend computed before the mutation.
    let _guard = hypa_state.lock.lock().await;

    let data_dir = persistence::get_data_dir(&app)?;
    persistence::save_hypa(&data_dir, &summaries)
}

#[cfg(test)]
mod tests {
    use super::*;

    // `Summary` / `HypaData` serde roundtrip tests live in `layream_core::hypa`
    // (single definition site, §4.1) — the duplicates that lived here were
    // removed with the duplicate type definitions. What remains below covers
    // logic local to this command layer.

    #[test]
    fn parse_summary_text_handles_json_and_raw() {
        assert_eq!(parse_summary_text(r#"{"summary":"x"}"#), "x");
        assert_eq!(parse_summary_text("plain text"), "plain text");
        assert_eq!(parse_summary_text("noise {\"summary\":\"y\"} tail"), "y");
        assert_eq!(parse_summary_text(""), "");
    }

    #[test]
    fn estimate_tokens_uses_four_chars_per_token() {
        assert_eq!(estimate_tokens(""), 0);
        assert_eq!(estimate_tokens("abc"), 0); // 3 / 4
        assert_eq!(estimate_tokens("abcd"), 1); // 4 / 4
        assert_eq!(estimate_tokens(&"x".repeat(40)), 10);
    }

    #[test]
    fn hypa_settings_from_value_maps_camelcase_and_defaults() {
        // External/Layream preset shape: camelCase ratios, omits randomMemoryRatio
        // (Layream-internal) and carries unrelated provider keys → both tolerated.
        let v = serde_json::json!({
            "memoryTokensRatio": 0.3,
            "recentMemoryRatio": 0.4,
            "similarMemoryRatio": 0.6,
            "maxChatsPerSummary": 8,
            "preserveOrphanedMemory": true,
            "provider": "vertex",
            "model": "gemini"
        });
        let s = hypa_settings_from_value(&v).unwrap();
        assert_eq!(s.memory_tokens_ratio, 0.3);
        assert_eq!(s.recent_memory_ratio, 0.4);
        assert_eq!(s.similar_memory_ratio, 0.6);
        assert_eq!(s.max_chats_per_summary, 8);
        assert!(s.preserve_orphaned_memory);
        // Omitted Layream-internal key falls back to default.
        assert_eq!(
            s.random_memory_ratio,
            HypaSettings::default().random_memory_ratio
        );
    }

    #[test]
    fn hypa_settings_from_value_rejects_wrong_type() {
        // A ratio supplied as a string is malformed → surfaced, not silently
        // defaulted (§5.1).
        let v = serde_json::json!({ "memoryTokensRatio": "lots" });
        assert!(hypa_settings_from_value(&v).is_err());
    }
}
