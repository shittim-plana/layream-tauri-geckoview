use layream_core::cbs::highlighter;
use layream_core::cbs::parser::{CbsContext, evaluate};
use layream_core::charx;
use layream_core::gca;
use layream_core::mistral;
use layream_core::preset;
use layream_core::types::BotPreset;
use layream_core::vertex_api::{
    self, Content, GenerateRequest, GenerationConfig, Part, SafetySetting, ThinkingConfig,
    VertexTool,
};
use layream_core::vertex_auth::{
    self, OAuthCredentials, PkceChallenge, Tokens, VERTEX_CLIENT_ID, LAYREAM_REDIRECT_URI,
};
use layream_core::gca::GCA_OAUTH_CLIENT_ID;
use layream_core::voyage;
use serde_json::Value;
use std::sync::Mutex;
use tauri::{Emitter, State};

use crate::persistence;

pub struct AuthState {
    pub vertex_pkce: Mutex<Option<PkceChallenge>>,
    pub vertex_tokens: Mutex<Option<Tokens>>,
    pub gca_pkce: Mutex<Option<PkceChallenge>>,
    pub gca_tokens: Mutex<Option<Tokens>>,
}

impl Default for AuthState {
    fn default() -> Self {
        Self {
            vertex_pkce: Mutex::new(None),
            vertex_tokens: Mutex::new(None),
            gca_pkce: Mutex::new(None),
            gca_tokens: Mutex::new(None),
        }
    }
}

impl AuthState {
    pub fn persist_tokens(&self, app: &tauri::AppHandle) {
        if let Ok(data_dir) = persistence::get_data_dir(app) {
            let vertex = self.vertex_tokens.lock().unwrap().clone();
            let gca = self.gca_tokens.lock().unwrap().clone();
            let _ = persistence::save_tokens(&data_dir, &vertex, &gca);
        }
    }

    pub fn load_persisted_tokens(&self, app: &tauri::AppHandle) {
        if let Ok(data_dir) = persistence::get_data_dir(app) {
            if let Ok((vertex, gca)) = persistence::load_tokens(&data_dir) {
                *self.vertex_tokens.lock().unwrap() = vertex;
                *self.gca_tokens.lock().unwrap() = gca;
            }
        }
    }
}

#[tauri::command]
pub fn load_preset(name: String, data: Vec<u8>) -> Result<Value, String> {
    let p = preset::read_preset(&name, &data).map_err(|e| e.to_string())?;
    serde_json::to_value(&p).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn export_preset(preset: Value, format: String) -> Result<Value, String> {
    let p: BotPreset = serde_json::from_value(preset).map_err(|e| e.to_string())?;
    let fmt = match format.as_str() {
        "risup" => preset::ExportFormat::Risup,
        _ => preset::ExportFormat::Json,
    };
    let (data, ext) = preset::export_preset(&p, fmt).map_err(|e| e.to_string())?;
    Ok(serde_json::json!({
        "data": data,
        "ext": ext,
    }))
}

#[tauri::command]
pub fn load_character(name: String, data: Vec<u8>) -> Result<Value, String> {
    let ch = charx::read_character(&name, &data).map_err(|e| e.to_string())?;
    let card_json = match &ch.card {
        Some(charx::CardData::V2(card)) => serde_json::to_value(card).ok(),
        Some(charx::CardData::OldTavern(card)) => serde_json::to_value(card).ok(),
        None => None,
    };
    Ok(serde_json::json!({
        "card": card_json,
        "assetCount": ch.assets.len(),
        "hasModule": ch.module_data.is_some(),
    }))
}

#[tauri::command]
pub fn evaluate_cbs(input: String, char_name: String, user_name: String) -> String {
    let mut ctx = CbsContext {
        char_name,
        user_name,
        ..Default::default()
    };
    evaluate(&input, &mut ctx)
}

fn build_safety_settings() -> Vec<SafetySetting> {
    ["HARM_CATEGORY_HARASSMENT", "HARM_CATEGORY_HATE_SPEECH",
     "HARM_CATEGORY_SEXUALLY_EXPLICIT", "HARM_CATEGORY_DANGEROUS_CONTENT",
     "HARM_CATEGORY_CIVIC_INTEGRITY"]
        .iter()
        .map(|cat| SafetySetting {
            category: cat.to_string(),
            threshold: "BLOCK_NONE".to_string(),
        })
        .collect()
}

fn messages_to_contents(messages: &[Value]) -> Vec<Content> {
    messages.iter().filter_map(|m| {
        let role = m.get("role")?.as_str()?;
        let text = m.get("text")?.as_str()?;
        Some(Content {
            role: role.to_string(),
            parts: vec![Part { text: Some(text.to_string()), thought: None, inline_data: None }],
        })
    }).collect()
}

fn messages_to_chat_messages(messages: &[Value]) -> Vec<mistral::ChatMessage> {
    messages.iter().filter_map(|m| {
        let role = m.get("role")?.as_str()?;
        let text = m.get("text")?.as_str()?;
        let mistral_role = match role { "model" | "char" => "assistant", _ => role };
        Some(mistral::ChatMessage {
            role: mistral_role.to_string(),
            content: text.to_string(),
            tool_calls: None,
            tool_call_id: None,
        })
    }).collect()
}

const MAX_LOGS: usize = 200;

fn log_api_call(log_state: &State<'_, RequestLogState>, provider: &str, model: &str, status: &str, duration_ms: u128) {
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let mut logs = log_state.logs.lock().unwrap();
    logs.push(serde_json::json!({
        "timestamp": timestamp,
        "provider": provider,
        "model": model,
        "status": status,
        "duration_ms": duration_ms,
    }));
    if logs.len() > MAX_LOGS {
        let excess = logs.len() - MAX_LOGS;
        logs.drain(..excess);
    }
}

#[tauri::command]
pub async fn chat_vertex(
    messages: Vec<Value>,
    model: String,
    project_id: String,
    region: String,
    temperature: f64,
    max_tokens: u32,
    top_p: Option<f64>,
    top_k: Option<u32>,
    thinking_budget: Option<i32>,
    tools_google_search: bool,
    tools_code_execution: bool,
    state: State<'_, AuthState>,
    log_state: State<'_, RequestLogState>,
    app: tauri::AppHandle,
) -> Result<String, String> {
    let tokens = {
        let guard = state.vertex_tokens.lock().unwrap();
        guard.clone().ok_or("Vertex AI not connected")?
    };
    let client = reqwest::Client::new();
    let creds = OAuthCredentials {
        client_id: VERTEX_CLIENT_ID.to_string(),
        client_secret: None,
        redirect_uri: LAYREAM_REDIRECT_URI.to_string(),
    };
    let valid_tokens = vertex_auth::get_valid_token(&client, &creds, &tokens)
        .await.map_err(|e| e.to_string())?;
    if valid_tokens.access_token != tokens.access_token {
        *state.vertex_tokens.lock().unwrap() = Some(valid_tokens.clone());
        state.persist_tokens(&app);
    }

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
        system_instruction: None,
        safety_settings: build_safety_settings(),
        generation_config: GenerationConfig {
            max_output_tokens: max_tokens,
            temperature,
            thinking_config,
            top_p,
            top_k,
        },
        tools: if tools.is_empty() { None } else { Some(tools) },
    };

    let app_clone = app.clone();
    let on_chunk = move |text: &str| {
        let _ = app_clone.emit("chat-chunk", text);
    };

    let start = std::time::Instant::now();
    let result = vertex_api::stream_generate(
        &client, &valid_tokens.access_token, &project_id, &region, &model, &request, on_chunk,
    ).await;
    let elapsed = start.elapsed().as_millis();

    match &result {
        Ok(_) => log_api_call(&log_state, "vertex", &model, "ok", elapsed),
        Err(e) => log_api_call(&log_state, "vertex", &model, &format!("error: {}", e), elapsed),
    }

    result.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn chat_gca(
    messages: Vec<Value>,
    model: String,
    temperature: f64,
    max_tokens: u32,
    top_p: Option<f64>,
    top_k: Option<u32>,
    thinking_level: Option<String>,
    tools_google_search: bool,
    tools_google_maps: bool,
    tools_url_context: bool,
    tools_code_execution: bool,
    state: State<'_, AuthState>,
    log_state: State<'_, RequestLogState>,
    app: tauri::AppHandle,
) -> Result<String, String> {
    let tokens = {
        let guard = state.gca_tokens.lock().unwrap();
        guard.clone().ok_or("GCA not connected")?
    };
    let client = reqwest::Client::new();
    let creds = OAuthCredentials {
        client_id: GCA_OAUTH_CLIENT_ID.to_string(),
        client_secret: None,
        redirect_uri: LAYREAM_REDIRECT_URI.to_string(),
    };
    let valid_tokens = vertex_auth::get_valid_token(&client, &creds, &tokens)
        .await.map_err(|e| e.to_string())?;
    if valid_tokens.access_token != tokens.access_token {
        *state.gca_tokens.lock().unwrap() = Some(valid_tokens.clone());
        state.persist_tokens(&app);
    }

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
        system_instruction: None,
        safety_settings: build_safety_settings(),
        generation_config: GenerationConfig {
            max_output_tokens: max_tokens,
            temperature,
            thinking_config,
            top_p,
            top_k,
        },
        tools: if tools.is_empty() { None } else { Some(tools) },
    };

    let app_clone = app.clone();
    let on_chunk = move |text: &str| {
        let _ = app_clone.emit("chat-chunk", text);
    };

    let start = std::time::Instant::now();
    let result = gca::stream_generate(
        &client, &valid_tokens.access_token, &model, &request, on_chunk,
    ).await;
    let elapsed = start.elapsed().as_millis();

    match &result {
        Ok(_) => log_api_call(&log_state, "gca", &model, "ok", elapsed),
        Err(e) => log_api_call(&log_state, "gca", &model, &format!("error: {}", e), elapsed),
    }

    result.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn chat_mistral(
    messages: Vec<Value>,
    model: String,
    api_key: String,
    temperature: f64,
    max_tokens: u32,
    top_p: Option<f64>,
    frequency_penalty: Option<f64>,
    presence_penalty: Option<f64>,
    reasoning_effort: Option<String>,
    log_state: State<'_, RequestLogState>,
    app: tauri::AppHandle,
) -> Result<String, String> {
    let request = mistral::ChatRequest {
        model: model.clone(),
        messages: messages_to_chat_messages(&messages),
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
    let app_clone = app.clone();
    let on_chunk = move |text: &str| {
        let _ = app_clone.emit("chat-chunk", text);
    };

    let start = std::time::Instant::now();
    let result = mistral::chat_stream(&client, &api_key, &request, on_chunk).await;
    let elapsed = start.elapsed().as_millis();

    match &result {
        Ok(_) => log_api_call(&log_state, "mistral", &model, "ok", elapsed),
        Err(e) => log_api_call(&log_state, "mistral", &model, &format!("error: {}", e), elapsed),
    }

    result.map_err(|e| e.to_string())
}

#[tauri::command]
pub fn vertex_oauth_start(state: State<'_, AuthState>) -> Result<String, String> {
    let pkce = vertex_auth::generate_pkce();
    let creds = OAuthCredentials {
        client_id: VERTEX_CLIENT_ID.to_string(),
        client_secret: None,
        redirect_uri: LAYREAM_REDIRECT_URI.to_string(),
    };
    let url = vertex_auth::build_auth_url(&creds, Some(&pkce));
    *state.vertex_pkce.lock().unwrap() = Some(pkce);
    Ok(url)
}

#[tauri::command]
pub fn gca_oauth_start(state: State<'_, AuthState>) -> Result<String, String> {
    let pkce = vertex_auth::generate_pkce();
    let creds = OAuthCredentials {
        client_id: GCA_OAUTH_CLIENT_ID.to_string(),
        client_secret: None,
        redirect_uri: LAYREAM_REDIRECT_URI.to_string(),
    };
    let url = vertex_auth::build_auth_url(&creds, Some(&pkce));
    *state.gca_pkce.lock().unwrap() = Some(pkce);
    Ok(url)
}

#[tauri::command]
pub async fn vertex_oauth_callback(
    code: String,
    state: State<'_, AuthState>,
    app: tauri::AppHandle,
) -> Result<String, String> {
    let pkce = state.vertex_pkce.lock().unwrap().take()
        .ok_or("No pending PKCE challenge")?;
    let creds = OAuthCredentials {
        client_id: VERTEX_CLIENT_ID.to_string(),
        client_secret: None,
        redirect_uri: LAYREAM_REDIRECT_URI.to_string(),
    };
    let client = reqwest::Client::new();
    let tokens = vertex_auth::exchange_code(&client, &creds, &code, Some(&pkce.verifier))
        .await
        .map_err(|e| e.to_string())?;
    *state.vertex_tokens.lock().unwrap() = Some(tokens);
    state.persist_tokens(&app);
    Ok("Vertex AI connected".into())
}

#[tauri::command]
pub async fn gca_oauth_callback(
    code: String,
    state: State<'_, AuthState>,
    app: tauri::AppHandle,
) -> Result<String, String> {
    let pkce = state.gca_pkce.lock().unwrap().take()
        .ok_or("No pending PKCE challenge")?;
    let creds = OAuthCredentials {
        client_id: GCA_OAUTH_CLIENT_ID.to_string(),
        client_secret: None,
        redirect_uri: LAYREAM_REDIRECT_URI.to_string(),
    };
    let client = reqwest::Client::new();
    let tokens = vertex_auth::exchange_code(&client, &creds, &code, Some(&pkce.verifier))
        .await
        .map_err(|e| e.to_string())?;
    *state.gca_tokens.lock().unwrap() = Some(tokens);
    state.persist_tokens(&app);
    Ok("GCA connected".into())
}

#[tauri::command]
pub fn vertex_oauth_status(state: State<'_, AuthState>) -> Value {
    let tokens = state.vertex_tokens.lock().unwrap();
    match tokens.as_ref() {
        Some(t) if !t.is_expired() => serde_json::json!({
            "connected": true,
            "expired": false,
        }),
        Some(_) => serde_json::json!({
            "connected": true,
            "expired": true,
        }),
        None => serde_json::json!({
            "connected": false,
        }),
    }
}

#[tauri::command]
pub fn gca_oauth_status(state: State<'_, AuthState>) -> Value {
    let tokens = state.gca_tokens.lock().unwrap();
    match tokens.as_ref() {
        Some(t) if !t.is_expired() => serde_json::json!({
            "connected": true,
            "expired": false,
        }),
        Some(_) => serde_json::json!({
            "connected": true,
            "expired": true,
        }),
        None => serde_json::json!({
            "connected": false,
        }),
    }
}

#[tauri::command]
pub async fn vertex_list_projects(
    state: State<'_, AuthState>,
    app: tauri::AppHandle,
) -> Result<Value, String> {
    let tokens = {
        let guard = state.vertex_tokens.lock().unwrap();
        guard.clone().ok_or("Not connected")?
    };
    let client = reqwest::Client::new();
    let creds = OAuthCredentials {
        client_id: VERTEX_CLIENT_ID.to_string(),
        client_secret: None,
        redirect_uri: LAYREAM_REDIRECT_URI.to_string(),
    };
    let valid_tokens = vertex_auth::get_valid_token(&client, &creds, &tokens)
        .await.map_err(|e| e.to_string())?;
    if valid_tokens.access_token != tokens.access_token {
        *state.vertex_tokens.lock().unwrap() = Some(valid_tokens.clone());
        state.persist_tokens(&app);
    }
    let projects = layream_core::vertex_auth::list_gcp_projects(&client, &valid_tokens.access_token)
        .await
        .map_err(|e| e.to_string())?;
    serde_json::to_value(&projects).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn vertex_oauth_disconnect(state: State<'_, AuthState>, app: tauri::AppHandle) -> String {
    *state.vertex_tokens.lock().unwrap() = None;
    *state.vertex_pkce.lock().unwrap() = None;
    state.persist_tokens(&app);
    "Disconnected".into()
}

#[tauri::command]
pub fn gca_oauth_disconnect(state: State<'_, AuthState>, app: tauri::AppHandle) -> String {
    *state.gca_tokens.lock().unwrap() = None;
    *state.gca_pkce.lock().unwrap() = None;
    state.persist_tokens(&app);
    "Disconnected".into()
}

#[tauri::command]
pub async fn mistral_list_models(api_key: String) -> Result<Value, String> {
    let client = reqwest::Client::new();
    let models = layream_core::mistral::list_models(&client, &api_key)
        .await
        .map_err(|e| e.to_string())?;
    serde_json::to_value(&models).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn vertex_list_models(
    region: String,
    state: State<'_, AuthState>,
    app: tauri::AppHandle,
) -> Result<Value, String> {
    let tokens = {
        let guard = state.vertex_tokens.lock().unwrap();
        guard.clone().ok_or("Vertex AI not connected")?
    };
    let client = reqwest::Client::new();
    let creds = OAuthCredentials {
        client_id: VERTEX_CLIENT_ID.to_string(),
        client_secret: None,
        redirect_uri: LAYREAM_REDIRECT_URI.to_string(),
    };
    let valid_tokens = vertex_auth::get_valid_token(&client, &creds, &tokens)
        .await.map_err(|e| e.to_string())?;
    if valid_tokens.access_token != tokens.access_token {
        *state.vertex_tokens.lock().unwrap() = Some(valid_tokens.clone());
        state.persist_tokens(&app);
    }
    let models = layream_core::vertex_api::list_models(&client, &valid_tokens.access_token, &region)
        .await
        .map_err(|e| e.to_string())?;
    serde_json::to_value(&models).map_err(|e| e.to_string())
}

pub struct RequestLogState {
    pub logs: Mutex<Vec<Value>>,
}

impl Default for RequestLogState {
    fn default() -> Self {
        Self { logs: Mutex::new(Vec::new()) }
    }
}

#[tauri::command]
pub fn get_request_logs(state: State<'_, RequestLogState>) -> Vec<Value> {
    state.logs.lock().unwrap().clone()
}

#[tauri::command]
pub fn clear_request_logs(state: State<'_, RequestLogState>) {
    state.logs.lock().unwrap().clear();
}

#[tauri::command]
pub fn highlight_cbs(input: String) -> Value {
    let tokens = highlighter::highlight(&input);
    let diagnostics = highlighter::check_blocks(&input);
    serde_json::json!({
        "tokens": tokens.iter().map(|t| {
            serde_json::json!({
                "start": t.start,
                "end": t.end,
                "kind": match t.kind {
                    highlighter::TokenKind::Control => "control",
                    highlighter::TokenKind::Macro => "macro",
                    highlighter::TokenKind::Variable => "variable",
                    highlighter::TokenKind::Bracket => "bracket",
                },
                "depth": t.depth,
            })
        }).collect::<Vec<_>>(),
        "diagnostics": diagnostics.iter().map(|d| {
            serde_json::json!({ "line": d.line, "message": d.message })
        }).collect::<Vec<_>>(),
    })
}

#[tauri::command]
pub fn cmd_save_settings(settings: Value, app: tauri::AppHandle) -> Result<(), String> {
    let data_dir = persistence::get_data_dir(&app)?;
    persistence::save_settings(&data_dir, &settings)
}

#[tauri::command]
pub fn cmd_load_settings(app: tauri::AppHandle) -> Result<Value, String> {
    let data_dir = persistence::get_data_dir(&app)?;
    persistence::load_settings(&data_dir)
}

#[tauri::command]
pub async fn embed_vertex(
    texts: Vec<String>,
    model: String,
    project_id: String,
    region: String,
    state: State<'_, AuthState>,
    app: tauri::AppHandle,
) -> Result<Vec<Vec<f64>>, String> {
    let tokens = {
        let guard = state.vertex_tokens.lock().unwrap();
        guard.clone().ok_or("Vertex AI not connected")?
    };
    let client = reqwest::Client::new();
    let creds = OAuthCredentials {
        client_id: VERTEX_CLIENT_ID.to_string(),
        client_secret: None,
        redirect_uri: LAYREAM_REDIRECT_URI.to_string(),
    };
    let valid_tokens = vertex_auth::get_valid_token(&client, &creds, &tokens)
        .await.map_err(|e| e.to_string())?;
    if valid_tokens.access_token != tokens.access_token {
        *state.vertex_tokens.lock().unwrap() = Some(valid_tokens.clone());
        state.persist_tokens(&app);
    }

    let text_refs: Vec<&str> = texts.iter().map(|s| s.as_str()).collect();
    vertex_api::batch_embed_contents(
        &client, &valid_tokens.access_token, &project_id, &region, &model, &text_refs,
    ).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn embed_voyage(
    texts: Vec<String>,
    model: String,
    api_key: String,
) -> Result<Vec<Vec<f64>>, String> {
    let client = reqwest::Client::new();
    voyage::embed(&client, &api_key, &texts, &model, None)
        .await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn gca_load_code_assist(
    state: State<'_, AuthState>,
    app: tauri::AppHandle,
) -> Result<String, String> {
    let tokens = {
        let guard = state.gca_tokens.lock().unwrap();
        guard.clone().ok_or("GCA not connected")?
    };
    let client = reqwest::Client::new();
    let creds = OAuthCredentials {
        client_id: GCA_OAUTH_CLIENT_ID.to_string(),
        client_secret: None,
        redirect_uri: LAYREAM_REDIRECT_URI.to_string(),
    };
    let valid_tokens = vertex_auth::get_valid_token(&client, &creds, &tokens)
        .await.map_err(|e| e.to_string())?;
    if valid_tokens.access_token != tokens.access_token {
        *state.gca_tokens.lock().unwrap() = Some(valid_tokens.clone());
        state.persist_tokens(&app);
    }
    gca::load_code_assist(&client, &valid_tokens.access_token)
        .await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn gca_check_opt_out(
    state: State<'_, AuthState>,
    app: tauri::AppHandle,
) -> Result<bool, String> {
    let tokens = {
        let guard = state.gca_tokens.lock().unwrap();
        guard.clone().ok_or("GCA not connected")?
    };
    let client = reqwest::Client::new();
    let creds = OAuthCredentials {
        client_id: GCA_OAUTH_CLIENT_ID.to_string(),
        client_secret: None,
        redirect_uri: LAYREAM_REDIRECT_URI.to_string(),
    };
    let valid_tokens = vertex_auth::get_valid_token(&client, &creds, &tokens)
        .await.map_err(|e| e.to_string())?;
    if valid_tokens.access_token != tokens.access_token {
        *state.gca_tokens.lock().unwrap() = Some(valid_tokens.clone());
        state.persist_tokens(&app);
    }
    gca::check_and_opt_out(&client, &valid_tokens.access_token)
        .await.map_err(|e| e.to_string())
}

#[tauri::command]
pub fn cmd_save_hypa(summaries: Value, app: tauri::AppHandle) -> Result<(), String> {
    let data_dir = persistence::get_data_dir(&app)?;
    persistence::save_hypa(&data_dir, &summaries)
}

#[tauri::command]
pub fn cmd_load_hypa(app: tauri::AppHandle) -> Result<Value, String> {
    let data_dir = persistence::get_data_dir(&app)?;
    persistence::load_hypa(&data_dir)
}
