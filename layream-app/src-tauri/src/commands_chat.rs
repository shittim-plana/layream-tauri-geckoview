use layream_core::gca;
use layream_core::mistral;
use layream_core::retry;
use layream_core::vertex_api::{
    self, Content, GenerateRequest, GenerationConfig, Part, ThinkingConfig,
    VertexTool, default_safety_settings,
};
use layream_core::voyage;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tauri::{Emitter, Manager, State};

use crate::commands_auth::{AuthState, ensure_gca_token, ensure_vertex_token, load_gca_project};
use crate::persistence;

/// A single chat message sent from the frontend to the chat API commands.
/// The frontend maps "char" → "model" before sending, so `role` is one of
/// "user" | "model" | "system".
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatApiMessage {
    pub role: String,
    pub text: String,
}

pub struct StreamCancelState {
    pub token: Mutex<Option<retry::CancelToken>>,
}

impl Default for StreamCancelState {
    fn default() -> Self {
        Self { token: Mutex::new(None) }
    }
}

pub struct StreamBufferState {
    pub buffer: Arc<Mutex<Vec<String>>>,
}

impl Default for StreamBufferState {
    fn default() -> Self {
        Self { buffer: Arc::new(Mutex::new(Vec::new())) }
    }
}

/// In-memory request-log buffer plus an optional file sink (REFACTOR_HYPA §6).
///
/// `logs` remains the authoritative source for the Logs UI and keeps its
/// MAX_LOGS cap — persistence is purely additive (§1.1). When `persist` is
/// true, each entry is also appended to a JSONL file under `data_dir`.
///
/// `persist` defaults to false: the prior behavior (volatile, in-memory only)
/// is preserved unless the user explicitly opts in (§1.4). `data_dir` is
/// resolved once in `setup` (the app handle isn't available at `Default`
/// construction time) and is `None` until then.
pub struct RequestLogState {
    pub logs: Mutex<Vec<Value>>,
    pub persist: Mutex<bool>,
    pub data_dir: Mutex<Option<PathBuf>>,
}

impl Default for RequestLogState {
    fn default() -> Self {
        Self {
            logs: Mutex::new(Vec::new()),
            persist: Mutex::new(false),
            data_dir: Mutex::new(None),
        }
    }
}

#[tauri::command(rename_all = "snake_case")]
pub fn poll_stream_chunks(state: State<'_, StreamBufferState>) -> Result<Vec<String>, String> {
    let mut guard = state.buffer.lock().map_err(|e| format!("lock poisoned: {e}"))?;
    let chunks: Vec<String> = guard.drain(..).collect();
    Ok(chunks)
}

#[tauri::command(rename_all = "snake_case")]
pub fn cancel_chat(state: State<'_, StreamCancelState>) -> Result<(), String> {
    let guard = state.token.lock().map_err(|e| format!("lock poisoned: {e}"))?;
    if let Some(token) = guard.as_ref() {
        token.store(true, std::sync::atomic::Ordering::Relaxed);
    }
    Ok(())
}

fn build_system_instruction(prompt: &Option<String>) -> Option<Content> {
    prompt.as_ref()
        .filter(|s| !s.trim().is_empty())
        .map(|s| Content {
            role: "user".to_string(),
            parts: vec![Part { text: Some(s.clone()), thought: None, inline_data: None }],
        })
}


fn messages_to_contents(messages: &[ChatApiMessage]) -> Vec<Content> {
    messages.iter().map(|m| {
        let api_role = match m.role.as_str() { "char" => "model", r => r };
        Content {
            role: api_role.to_string(),
            parts: vec![Part { text: Some(m.text.clone()), thought: None, inline_data: None }],
        }
    }).collect()
}

fn messages_to_chat_messages(messages: &[ChatApiMessage]) -> Vec<mistral::ChatMessage> {
    messages.iter().map(|m| {
        let mistral_role = match m.role.as_str() { "model" | "char" => "assistant", r => r };
        mistral::ChatMessage {
            role: mistral_role.to_string(),
            content: m.text.clone(),
            tool_calls: None,
            tool_call_id: None,
        }
    }).collect()
}

const MAX_LOGS: usize = 200;

fn truncate_str(s: &str, max: usize) -> String {
    if s.len() <= max { return s.to_string(); }
    let mut end = max;
    while end > 0 && !s.is_char_boundary(end) { end -= 1; }
    format!("{}...", &s[..end])
}

fn log_api_call(
    log_state: &State<'_, RequestLogState>, provider: &str, model: &str,
    status: &str, duration_ms: u128,
    request_preview: &str, response_preview: &str, estimated_tokens: u32,
) {
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let entry = serde_json::json!({
        "timestamp": timestamp,
        "provider": provider,
        "model": model,
        "status": status,
        "duration_ms": duration_ms,
        "request_preview": request_preview,
        "response_preview": response_preview,
        "estimated_tokens": estimated_tokens,
    });

    // In-memory ring buffer (authoritative for the Logs UI, capped at MAX_LOGS).
    // A poisoned lock is best-effort-skipped as before: a logging failure must
    // not break the chat call that produced the log.
    if let Ok(mut logs) = log_state.logs.lock() {
        logs.push(entry.clone());
        if logs.len() > MAX_LOGS {
            let excess = logs.len() - MAX_LOGS;
            logs.drain(..excess);
        }
    }

    // Optional file sink. Only when the user opted in and the data dir has been
    // resolved (set in setup). An append failure is logged, not swallowed
    // (§5.1), and never propagated into the chat result — persistence is a
    // side concern, not part of the call's contract.
    let should_persist = log_state.persist.lock().map(|g| *g).unwrap_or(false);
    if should_persist {
        let dir = log_state.data_dir.lock().ok().and_then(|g| g.clone());
        if let Some(dir) = dir {
            if let Err(e) = persistence::append_log(&dir, &entry) {
                log::warn!("failed to append request log to file: {e}");
            }
        } else {
            log::warn!("request log persistence enabled but data dir is not resolved yet");
        }
    }
}

#[tauri::command(rename_all = "snake_case")]
pub async fn chat_vertex(
    messages: Vec<ChatApiMessage>,
    system_prompt: Option<String>,
    model: String,
    project_id: String,
    region: String,
    temperature: f64,
    max_tokens: u32,
    top_p: Option<f64>,
    top_k: Option<u32>,
    frequency_penalty: Option<f64>,
    presence_penalty: Option<f64>,
    thinking_budget: Option<i32>,
    tools_google_search: bool,
    tools_code_execution: bool,
    state: State<'_, AuthState>,
    cancel_state: State<'_, StreamCancelState>,
    log_state: State<'_, RequestLogState>,
    app: tauri::AppHandle,
) -> Result<String, String> {
    let cancel = retry::new_cancel_token();
    *cancel_state.token.lock().map_err(|e| format!("lock poisoned: {e}"))? = Some(cancel.clone());

    let (client, access_token) = ensure_vertex_token(&state, &app).await?;

    let thinking_config = thinking_budget.map(|b| ThinkingConfig::Budget { thinking_budget: b });

    let mut tools = Vec::new();
    if tools_google_search {
        tools.push(VertexTool::GoogleSearch(serde_json::Map::new()));
    }
    if tools_code_execution {
        tools.push(VertexTool::CodeExecution(serde_json::Map::new()));
    }

    let request = GenerateRequest {
        contents: messages_to_contents(&messages),
        system_instruction: build_system_instruction(&system_prompt),
        safety_settings: default_safety_settings(),
        generation_config: GenerationConfig {
            max_output_tokens: max_tokens,
            temperature,
            thinking_config,
            top_p,
            top_k,
            frequency_penalty,
            presence_penalty,
            response_mime_type: None,
            response_schema: None,
        },
        tools: if tools.is_empty() { None } else { Some(tools) },
    };

    if let Ok(mut buf) = app.state::<StreamBufferState>().buffer.lock() { buf.clear(); }
    let buffer_clone = app.state::<StreamBufferState>().buffer.clone();
    let app_clone = app.clone();
    let on_chunk = move |text: &str| {
        let _ = app_clone.emit("chat-chunk", text);
        if let Ok(mut buf) = buffer_clone.lock() { buf.push(text.to_string()); }
    };

    let start = std::time::Instant::now();
    let result = vertex_api::stream_generate(
        &client, &access_token, &project_id, &region, &model, &request, on_chunk,
        Some(cancel),
    ).await;
    let elapsed = start.elapsed().as_millis();

    let sp_preview = truncate_str(system_prompt.as_deref().unwrap_or(""), 200);
    match &result {
        Ok(text) => log_api_call(&log_state, "vertex", &model, "ok", elapsed, &sp_preview, &truncate_str(text, 500), (text.len() as u32) / 4),
        Err(e) => log_api_call(&log_state, "vertex", &model, &format!("error: {}", e), elapsed, &sp_preview, "", 0),
    }

    result.map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub async fn chat_gca(
    messages: Vec<ChatApiMessage>,
    system_prompt: Option<String>,
    model: String,
    temperature: f64,
    max_tokens: u32,
    top_p: Option<f64>,
    top_k: Option<u32>,
    frequency_penalty: Option<f64>,
    presence_penalty: Option<f64>,
    thinking_level: Option<String>,
    tools_google_search: bool,
    tools_google_maps: bool,
    tools_url_context: bool,
    tools_code_execution: bool,
    state: State<'_, AuthState>,
    cancel_state: State<'_, StreamCancelState>,
    log_state: State<'_, RequestLogState>,
    app: tauri::AppHandle,
) -> Result<String, String> {
    let cancel = retry::new_cancel_token();
    *cancel_state.token.lock().map_err(|e| format!("lock poisoned: {e}"))? = Some(cancel.clone());

    let (client, access_token) = ensure_gca_token(&state, &app).await?;

    let thinking_config = thinking_level
        .filter(|l| l != "none")
        .map(|l| ThinkingConfig::Level { thinking_level: l });

    let mut tools = Vec::new();
    if tools_google_search {
        tools.push(VertexTool::GoogleSearch(serde_json::Map::new()));
    }
    if tools_google_maps {
        tools.push(VertexTool::GoogleMaps(serde_json::Map::new()));
    }
    if tools_url_context {
        tools.push(VertexTool::UrlContext(serde_json::Map::new()));
    }
    if tools_code_execution {
        tools.push(VertexTool::CodeExecution(serde_json::Map::new()));
    }

    let request = GenerateRequest {
        contents: messages_to_contents(&messages),
        system_instruction: build_system_instruction(&system_prompt),
        safety_settings: default_safety_settings(),
        generation_config: GenerationConfig {
            max_output_tokens: max_tokens,
            temperature,
            thinking_config,
            top_p,
            top_k,
            frequency_penalty,
            presence_penalty,
            response_mime_type: None,
            response_schema: None,
        },
        tools: if tools.is_empty() { None } else { Some(tools) },
    };

    if let Ok(mut buf) = app.state::<StreamBufferState>().buffer.lock() { buf.clear(); }
    let buffer_clone = app.state::<StreamBufferState>().buffer.clone();
    let app_clone = app.clone();
    let on_chunk = move |text: &str| {
        let _ = app_clone.emit("chat-chunk", text);
        if let Ok(mut buf) = buffer_clone.lock() { buf.push(text.to_string()); }
    };

    // load gcaProject from settings (graceful fallback to None)
    let gca_project = load_gca_project(&app);

    let start = std::time::Instant::now();
    let result = gca::stream_generate(
        &client, &access_token, &model, gca_project.as_deref(), &request, on_chunk,
        Some(cancel),
    ).await;
    let elapsed = start.elapsed().as_millis();

    match &result {
        Ok(text) => log_api_call(&log_state, "gca", &model, "ok", elapsed, &truncate_str(system_prompt.as_deref().unwrap_or(""), 200), &truncate_str(text, 500), (text.len() as u32) / 4),
        Err(e) => log_api_call(&log_state, "gca", &model, &format!("error: {}", e), elapsed, "", "", 0),
    }

    result.map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub async fn chat_mistral(
    messages: Vec<ChatApiMessage>,
    system_prompt: Option<String>,
    model: String,
    api_key: String,
    temperature: f64,
    max_tokens: u32,
    top_p: Option<f64>,
    frequency_penalty: Option<f64>,
    presence_penalty: Option<f64>,
    reasoning_effort: Option<String>,
    cancel_state: State<'_, StreamCancelState>,
    log_state: State<'_, RequestLogState>,
    app: tauri::AppHandle,
) -> Result<String, String> {
    let cancel = retry::new_cancel_token();
    *cancel_state.token.lock().map_err(|e| format!("lock poisoned: {e}"))? = Some(cancel.clone());

    let mut chat_msgs = messages_to_chat_messages(&messages);
    if let Some(sp) = &system_prompt {
        if !sp.trim().is_empty() {
            chat_msgs.insert(0, mistral::ChatMessage {
                role: "system".to_string(),
                content: sp.clone(),
                tool_calls: None,
                tool_call_id: None,
            });
        }
    }
    let request = mistral::ChatRequest {
        model: model.clone(),
        messages: chat_msgs,
        temperature: Some(temperature),
        top_p,
        max_tokens: Some(max_tokens),
        stream: Some(true),
        frequency_penalty,
        presence_penalty,
        stop: None,
        random_seed: None,
        response_format: None,
        reasoning_effort,
        tools: None,
        tool_choice: None,
    };

    let client = reqwest::Client::new();
    if let Ok(mut buf) = app.state::<StreamBufferState>().buffer.lock() { buf.clear(); }
    let buffer_clone = app.state::<StreamBufferState>().buffer.clone();
    let app_clone = app.clone();
    let on_chunk = move |text: &str| {
        let _ = app_clone.emit("chat-chunk", text);
        if let Ok(mut buf) = buffer_clone.lock() { buf.push(text.to_string()); }
    };

    let start = std::time::Instant::now();
    let result = mistral::chat_stream(&client, &api_key, &request, on_chunk, Some(cancel)).await;
    let elapsed = start.elapsed().as_millis();

    match &result {
        Ok(text) => log_api_call(&log_state, "mistral", &model, "ok", elapsed, &truncate_str(system_prompt.as_deref().unwrap_or(""), 200), &truncate_str(text, 500), (text.len() as u32) / 4),
        Err(e) => log_api_call(&log_state, "mistral", &model, &format!("error: {}", e), elapsed, "", "", 0),
    }

    result.map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub async fn mistral_list_models(api_key: String) -> Result<Value, String> {
    let client = reqwest::Client::new();
    let models = layream_core::mistral::list_models(&client, &api_key)
        .await
        .map_err(|e| e.to_string())?;
    serde_json::to_value(&models).map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub async fn vertex_list_models(
    region: String,
    state: State<'_, AuthState>,
    app: tauri::AppHandle,
) -> Result<Value, String> {
    let (client, access_token) = ensure_vertex_token(&state, &app).await?;
    let models = layream_core::vertex_api::list_models(&client, &access_token, &region)
        .await
        .map_err(|e| e.to_string())?;
    serde_json::to_value(&models).map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub fn get_request_logs(state: State<'_, RequestLogState>) -> Result<Vec<Value>, String> {
    // When persisting, the file is the durable record across restarts; the
    // in-memory buffer only holds the current session (capped at MAX_LOGS).
    // Returning the file contents surfaces the full opted-in history. When not
    // persisting, return the in-memory buffer exactly as before (§1.1).
    let persisting = *state.persist.lock().map_err(|e| format!("lock poisoned: {e}"))?;
    if persisting {
        if let Some(dir) = state.data_dir.lock().map_err(|e| format!("lock poisoned: {e}"))?.clone() {
            return persistence::read_logs(&dir);
        }
    }
    Ok(state.logs.lock().map_err(|e| format!("lock poisoned: {e}"))?.clone())
}

#[tauri::command(rename_all = "snake_case")]
pub fn clear_request_logs(state: State<'_, RequestLogState>) -> Result<(), String> {
    // Clear both sinks so a Clear from the UI is total regardless of which one
    // get_request_logs is currently reading. The file is cleared even when
    // persistence is off, so a stale file from a prior session is not orphaned.
    state.logs.lock().map_err(|e| format!("lock poisoned: {e}"))?.clear();
    if let Some(dir) = state.data_dir.lock().map_err(|e| format!("lock poisoned: {e}"))?.clone() {
        persistence::clear_logs(&dir)?;
    }
    Ok(())
}

/// Returns whether request-log file persistence is currently enabled.
#[tauri::command(rename_all = "snake_case")]
pub fn get_log_persistence(state: State<'_, RequestLogState>) -> Result<bool, String> {
    Ok(*state.persist.lock().map_err(|e| format!("lock poisoned: {e}"))?)
}

/// Enables or disables request-log file persistence and persists the choice to
/// settings.json so it survives a restart. Errors propagate (§5.1).
#[tauri::command(rename_all = "snake_case")]
pub fn set_log_persistence(
    enabled: bool,
    state: State<'_, RequestLogState>,
    app: tauri::AppHandle,
) -> Result<(), String> {
    *state.persist.lock().map_err(|e| format!("lock poisoned: {e}"))? = enabled;

    // Persist the toggle into settings.json under "logPersistence" so the next
    // launch restores it (loaded in setup). Merge into the existing settings
    // object rather than overwriting, to preserve every other key.
    let data_dir = persistence::get_data_dir(&app)?;
    let mut settings = persistence::load_settings(&data_dir)?;
    if let Some(obj) = settings.as_object_mut() {
        obj.insert("logPersistence".to_string(), Value::Bool(enabled));
    } else {
        settings = serde_json::json!({ "logPersistence": enabled });
    }
    persistence::save_settings(&data_dir, &settings)
}

#[tauri::command(rename_all = "snake_case")]
pub async fn embed_vertex(
    texts: Vec<String>,
    model: String,
    project_id: String,
    region: String,
    state: State<'_, AuthState>,
    app: tauri::AppHandle,
) -> Result<Vec<Vec<f64>>, String> {
    let (client, access_token) = ensure_vertex_token(&state, &app).await?;

    let text_refs: Vec<&str> = texts.iter().map(|s| s.as_str()).collect();
    vertex_api::batch_embed_contents(
        &client, &access_token, &project_id, &region, &model, &text_refs,
    ).await.map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub async fn embed_voyage(
    texts: Vec<String>,
    model: String,
    api_key: String,
) -> Result<Vec<Vec<f64>>, String> {
    let client = reqwest::Client::new();
    voyage::embed(&client, &api_key, &texts, &model, None)
        .await.map_err(|e| e.to_string())
}

const USER_MSG_SYSTEM_PROMPT: &str =
    "Given this roleplay conversation context, generate a short, natural next user message. \
     Reply ONLY with the message text, nothing else.";

const USER_MSG_MAX_TOKENS: u32 = 256;
const USER_MSG_TEMPERATURE: f64 = 1.0;

#[tauri::command(rename_all = "snake_case")]
pub async fn generate_user_message(
    context: Vec<ChatApiMessage>,
    provider: String,
    model: String,
    region: Option<String>,
    project_id: Option<String>,
    api_key: Option<String>,
    persona: Option<String>,
    response_schema: Option<Value>,
    state: State<'_, AuthState>,
    app: tauri::AppHandle,
) -> Result<String, String> {
    let system_prompt = match persona.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        Some(p) => format!("{}\n\nPersona:\n{}", USER_MSG_SYSTEM_PROMPT, p),
        None => USER_MSG_SYSTEM_PROMPT.to_string(),
    };
    let response_mime_type = response_schema.as_ref().map(|_| "application/json".to_string());

    match provider.as_str() {
        "vertex" => {
            let project_id = project_id.ok_or("project_id required for vertex")?;
            let region = region.as_deref().unwrap_or("us-central1");

            let (client, access_token) = ensure_vertex_token(&state, &app).await?;

            let request = GenerateRequest {
                contents: messages_to_contents(&context),
                system_instruction: Some(Content {
                    role: "user".to_string(),
                    parts: vec![Part {
                        text: Some(system_prompt.clone()),
                        thought: None,
                        inline_data: None,
                    }],
                }),
                safety_settings: default_safety_settings(),
                generation_config: GenerationConfig {
                    max_output_tokens: USER_MSG_MAX_TOKENS,
                    temperature: USER_MSG_TEMPERATURE,
                    thinking_config: None,
                    top_p: None,
                    top_k: None,
                    frequency_penalty: None,
                    presence_penalty: None,
                    response_mime_type: response_mime_type.clone(),
                    response_schema: response_schema.clone(),
                },
                tools: None,
            };

            vertex_api::generate_non_streaming(
                &client,
                &access_token,
                &project_id,
                region,
                &model,
                &request,
            )
            .await
            .map_err(|e| e.to_string())
        }
        "gca" => {
            let (client, access_token) = ensure_gca_token(&state, &app).await?;

            let request = GenerateRequest {
                contents: messages_to_contents(&context),
                system_instruction: Some(Content {
                    role: "user".to_string(),
                    parts: vec![Part {
                        text: Some(system_prompt.clone()),
                        thought: None,
                        inline_data: None,
                    }],
                }),
                safety_settings: default_safety_settings(),
                generation_config: GenerationConfig {
                    max_output_tokens: USER_MSG_MAX_TOKENS,
                    temperature: USER_MSG_TEMPERATURE,
                    thinking_config: None,
                    top_p: None,
                    top_k: None,
                    frequency_penalty: None,
                    presence_penalty: None,
                    response_mime_type: response_mime_type.clone(),
                    response_schema: response_schema.clone(),
                },
                tools: None,
            };

            // load gcaProject from settings (graceful fallback to None)
            let gca_project = load_gca_project(&app);

            gca::generate_non_streaming(
                &client,
                &access_token,
                &model,
                gca_project.as_deref(),
                &request,
            )
            .await
            .map_err(|e| e.to_string())
        }
        "mistral" => {
            let api_key = api_key.ok_or("api_key required for mistral")?;

            let mut messages = messages_to_chat_messages(&context);
            messages.insert(0, mistral::ChatMessage {
                role: "system".to_string(),
                content: system_prompt.clone(),
                tool_calls: None,
                tool_call_id: None,
            });

            let response_format = response_schema.as_ref().map(|schema| mistral::ResponseFormat {
                format_type: "json_schema".to_string(),
                json_schema: Some(mistral::JsonSchemaSpec {
                    name: "user_message".to_string(),
                    schema: schema.clone(),
                    strict: true,
                }),
            });

            let request = mistral::ChatRequest {
                model: model.clone(),
                messages,
                temperature: Some(USER_MSG_TEMPERATURE),
                top_p: None,
                max_tokens: Some(USER_MSG_MAX_TOKENS),
                stream: Some(false),
                frequency_penalty: None,
                presence_penalty: None,
                stop: None,
                random_seed: None,
                response_format,
                reasoning_effort: None,
                tools: None,
                tool_choice: None,
            };

            let client = reqwest::Client::new();
            let response = mistral::chat(&client, &api_key, &request)
                .await
                .map_err(|e| e.to_string())?;

            response
                .choices
                .first()
                .and_then(|c| c.message.as_ref())
                .map(|m| m.content.clone())
                .ok_or_else(|| "No response content from Mistral".to_string())
        }
        other => Err(format!("Unsupported provider: {}", other)),
    }
}
