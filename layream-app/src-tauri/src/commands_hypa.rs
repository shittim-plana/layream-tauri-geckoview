//! HyPA backend — hierarchical prompt augmentation commands.
//!
//! Schema is RisuAI HypaV3 compatible (`chatMemos`, `isImportant` fields preserved
//! via serde rename) with new-arona-bot extensions for `pinBoost` and `invalidated`.
//!
//! Embeddings are stored as `Vec<f64>` to match the JSON wire format (JSON numbers
//! decode to f64 by default) and to compose with `layream_core::voyage::cosine_similarity`
//! without precision conversion. This is intentional — see §3-A (no demote/promote).

use layream_core::gca::{self, GCA_OAUTH_CLIENT_ID};
use layream_core::mistral;
use layream_core::vertex_api::{
    self, Content, GenerateRequest, GenerationConfig, Part, SafetySetting,
};
use layream_core::vertex_auth::{
    self, OAuthCredentials, LAYREAM_REDIRECT_URI, VERTEX_CLIENT_ID,
};
use layream_core::voyage;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::State;
use tokio::sync::Mutex;

use crate::commands::AuthState;
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
/// Mirrors `PIN_BOOST_INCREMENT` in new-arona-bot pin-message route.
const PIN_BOOST_INCREMENT: f64 = 0.5;

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

// === Schema (RisuAI HypaV3 compatible + new-arona-bot extensions) ===

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Summary {
    pub text: String,

    /// Chat message ids covered by this summary. Field name preserved for
    /// RisuAI HypaV3 wire compatibility.
    #[serde(rename = "chatMemos", default)]
    pub chat_memos: Vec<String>,

    /// Optional embedding vector used by `hypa_search`. f64 to match JSON
    /// number decoding and `voyage::cosine_similarity`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub embedding: Option<Vec<f64>>,

    /// Whether this summary should always be included regardless of similarity.
    /// Field name preserved for RisuAI HypaV3 wire compatibility.
    #[serde(rename = "isImportant", default)]
    pub is_important: bool,

    /// Score boost applied when any covered chat is pinned (new-arona-bot).
    #[serde(rename = "pinBoost", default)]
    pub pin_boost: f64,

    /// True once a covered chat has been deleted (new-arona-bot).
    #[serde(default)]
    pub invalidated: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct HypaData {
    #[serde(default)]
    pub summaries: Vec<Summary>,

    /// Future fields (display state, settings cache) live here without breaking
    /// roundtrip compatibility — captured into `extra` and re-emitted unchanged.
    #[serde(flatten)]
    pub extra: serde_json::Map<String, Value>,
}

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
    // This matches how RisuAI HypaV3 prompts the model with the conversation
    // followed by an instruction. We use a single user message containing the
    // instruction + serialized window (kept simple — §6-D).
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
/// Mirrors `tryParseJsonObject` in new-arona-bot summarizer.ts.
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

/// Cosine similarity search over stored summaries. Pin-boosted, invalidation-aware.
///
/// Returns top-K entries as `{ index, score, summary }` sorted by score desc.
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

    let mut scored: Vec<(usize, f64, &Summary)> = data
        .summaries
        .iter()
        .enumerate()
        .filter(|(_, s)| !s.invalidated)
        .filter_map(|(i, s)| {
            let emb = s.embedding.as_ref()?;
            if emb.len() != query_embedding.len() {
                // §3-A: dimension mismatch is a fabrication risk — skip rather
                // than silently zero-fill.
                return None;
            }
            let cos = voyage::cosine_similarity(&query_embedding, emb);
            let combined = cos + s.pin_boost;
            Some((i, combined, s))
        })
        .collect();

    // Sort by combined score desc.
    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    scored.truncate(top_k);

    let result: Vec<Value> = scored
        .into_iter()
        .map(|(i, score, s)| {
            serde_json::json!({
                "index": i,
                "score": score,
                "summary": s,
            })
        })
        .collect();

    Ok(Value::Array(result))
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
            // Pinning an invalidated summary is a no-op (mirrors the SQL filter
            // `WHERE invalidated = false` in new-arona-bot pin-message route).
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

/// Mark every summary covering `chat_id` as invalidated. Mirrors new-arona-bot
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
///   - `hypa_search` skips them via `filter(|(_, s)| !s.invalidated)`.
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
        // §10-A: completeness — RisuAI compatible field names present.
        assert!(v.get("chatMemos").is_some(), "expected chatMemos key");
        assert!(v.get("isImportant").is_some(), "expected isImportant key");
        assert!(v.get("pinBoost").is_some(), "expected pinBoost key");
        // §10-A: soundness — Rust snake_case names absent in wire format.
        assert!(v.get("chat_memos").is_none());
        assert!(v.get("is_important").is_none());
        assert!(v.get("pin_boost").is_none());
    }

    #[test]
    fn summary_deserialization_accepts_risuai_shape() {
        // §1: RisuAI HypaV3 minimum shape.
        let raw = r#"{"text":"abc","chatMemos":["m1"],"isImportant":true}"#;
        let s: Summary = serde_json::from_str(raw).unwrap();
        assert_eq!(s.text, "abc");
        assert_eq!(s.chat_memos, vec!["m1".to_string()]);
        assert!(s.is_important);
        // Defaults for new-arona-bot fields.
        assert_eq!(s.pin_boost, 0.0);
        assert!(!s.invalidated);
        assert!(s.embedding.is_none());
    }

    #[test]
    fn parse_summary_text_handles_json_and_raw() {
        assert_eq!(parse_summary_text(r#"{"summary":"x"}"#), "x");
        assert_eq!(parse_summary_text("plain text"), "plain text");
        assert_eq!(parse_summary_text("noise {\"summary\":\"y\"} tail"), "y");
        assert_eq!(parse_summary_text(""), "");
    }

    #[test]
    fn hypa_data_roundtrip_preserves_extra_fields() {
        // §1-C / §10-B: future fields must roundtrip unchanged.
        let raw = serde_json::json!({
            "summaries": [],
            "settings": { "memoryTokensRatio": 0.2 }
        });
        let data: HypaData = serde_json::from_value(raw.clone()).unwrap();
        let back = serde_json::to_value(&data).unwrap();
        assert_eq!(back.get("settings"), raw.get("settings"));
    }
}
