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
use layream_core::gca::{GCA_OAUTH_CLIENT_ID, GCA_OAUTH_CLIENT_SECRET, GCA_OAUTH_SCOPE};
use layream_core::voyage;
use serde_json::Value;
use std::sync::Mutex;
use tauri::{Emitter, Manager, State};

use std::collections::HashMap;

use crate::persistence;

pub struct CharacterAssetsState {
    pub assets: Mutex<HashMap<String, Vec<u8>>>,
}

impl Default for CharacterAssetsState {
    fn default() -> Self {
        Self { assets: Mutex::new(HashMap::new()) }
    }
}

pub struct AuthState {
    pub vertex_pkce: Mutex<Option<PkceChallenge>>,
    pub vertex_tokens: Mutex<Option<Tokens>>,
    pub gca_tokens: Mutex<Option<Tokens>>,
}

impl Default for AuthState {
    fn default() -> Self {
        Self {
            vertex_pkce: Mutex::new(None),
            vertex_tokens: Mutex::new(None),
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

#[tauri::command(rename_all = "snake_case")]
pub async fn load_preset(name: String, data: Vec<u8>) -> Result<Value, String> {
    tokio::task::spawn_blocking(move || {
        let p = preset::read_preset(&name, &data).map_err(|e| e.to_string())?;
        serde_json::to_value(&p).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command(rename_all = "snake_case")]
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

#[tauri::command(rename_all = "snake_case")]
pub async fn load_character(
    name: String,
    data: Vec<u8>,
    asset_state: State<'_, CharacterAssetsState>,
) -> Result<Value, String> {
    let result = tokio::task::spawn_blocking(move || -> Result<_, String> {
        let ch = charx::read_character(&name, &data).map_err(|e| e.to_string())?;
        let card_json = match &ch.card {
            Some(charx::CardData::V2(card)) => serde_json::to_value(card).ok(),
            Some(charx::CardData::OldTavern(card)) => serde_json::to_value(card).ok(),
            None => None,
        };
        let asset_list: Vec<Value> = ch.assets.iter().map(|(name, data)| {
            serde_json::json!({
                "name": name,
                "size": data.len(),
            })
        }).collect();
        let json = serde_json::json!({
            "card": card_json,
            "assetCount": ch.assets.len(),
            "assetList": asset_list,
            "hasModule": ch.module_data.is_some(),
        });
        Ok((json, ch.assets))
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| e.to_string())?;

    *asset_state.assets.lock().unwrap() = result.1;
    Ok(result.0)
}

#[tauri::command(rename_all = "snake_case")]
pub async fn parse_risum(data: Vec<u8>) -> Result<Value, String> {
    tokio::task::spawn_blocking(move || {
        preset::parse_risum_data(&data).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command(rename_all = "snake_case")]
pub async fn load_preset_from_path(name: String, temp_name: String, app: tauri::AppHandle) -> Result<Value, String> {
    let data_dir = persistence::get_data_dir(&app)?;
    let path = data_dir.join(&temp_name);
    let data = std::fs::read(&path).map_err(|e| format!("read {}: {}", path.display(), e))?;
    let _ = std::fs::remove_file(&path);
    tokio::task::spawn_blocking(move || -> Result<Value, String> {
        let p = preset::read_preset(&name, &data).map_err(|e| e.to_string())?;
        serde_json::to_value(&p).map_err(|e| e.to_string())
    }).await.map_err(|e| e.to_string())?
    .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub async fn load_character_from_path(
    name: String, temp_name: String,
    asset_state: State<'_, CharacterAssetsState>, app: tauri::AppHandle,
) -> Result<Value, String> {
    let data_dir = persistence::get_data_dir(&app)?;
    let path = data_dir.join(&temp_name);
    let data = std::fs::read(&path).map_err(|e| format!("read {}: {}", path.display(), e))?;
    let _ = std::fs::remove_file(&path);
    let result = tokio::task::spawn_blocking(move || -> Result<_, String> {
        let ch = charx::read_character(&name, &data).map_err(|e| e.to_string())?;
        let card_json = match &ch.card {
            Some(charx::CardData::V2(card)) => serde_json::to_value(card).ok(),
            Some(charx::CardData::OldTavern(card)) => serde_json::to_value(card).ok(),
            None => None,
        };
        let asset_list: Vec<Value> = ch.assets.iter().map(|(n, d)| serde_json::json!({"name": n, "size": d.len()})).collect();
        let json = serde_json::json!({"card": card_json, "assetCount": ch.assets.len(), "assetList": asset_list, "hasModule": ch.module_data.is_some()});
        Ok((json, ch.assets))
    }).await.map_err(|e| e.to_string())?
    .map_err(|e| e.to_string())?;
    *asset_state.assets.lock().unwrap() = result.1;
    Ok(result.0)
}

#[tauri::command(rename_all = "snake_case")]
pub async fn parse_risum_from_path(temp_name: String, app: tauri::AppHandle) -> Result<Value, String> {
    let data_dir = persistence::get_data_dir(&app)?;
    let path = data_dir.join(&temp_name);
    let data = std::fs::read(&path).map_err(|e| format!("read {}: {}", path.display(), e))?;
    let _ = std::fs::remove_file(&path);
    tokio::task::spawn_blocking(move || -> Result<Value, String> {
        preset::parse_risum_data(&data).map_err(|e| e.to_string())
    }).await.map_err(|e| e.to_string())?
    .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
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
        let api_role = match role { "char" => "model", _ => role };
        Some(Content {
            role: api_role.to_string(),
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

#[tauri::command(rename_all = "snake_case")]
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
            response_mime_type: None,
            response_schema: None,
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

#[tauri::command(rename_all = "snake_case")]
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
            response_mime_type: None,
            response_schema: None,
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

#[tauri::command(rename_all = "snake_case")]
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

#[tauri::command(rename_all = "snake_case")]
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

#[tauri::command(rename_all = "snake_case")]
pub async fn gca_oauth_start(
    state: State<'_, AuthState>,
    app: tauri::AppHandle,
) -> Result<String, String> {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .map_err(|e| format!("Failed to bind loopback server: {}", e))?;
    let port = listener.local_addr().map_err(|e| e.to_string())?.port();
    let redirect_uri = format!("http://localhost:{}/oauth2callback", port);

    let auth_url = format!(
        "https://accounts.google.com/o/oauth2/v2/auth?client_id={}&redirect_uri={}&response_type=code&scope={}&access_type=offline&prompt=select_account+consent",
        vertex_auth::urlencoded(GCA_OAUTH_CLIENT_ID),
        vertex_auth::urlencoded(&redirect_uri),
        vertex_auth::urlencoded(GCA_OAUTH_SCOPE),
    );

    let app_clone = app.clone();
    let redirect_for_exchange = redirect_uri.clone();
    tokio::spawn(async move {
        let accept_result = tokio::time::timeout(
            std::time::Duration::from_secs(300),
            listener.accept(),
        ).await;
        let (stream, _) = match accept_result {
            Ok(Ok(v)) => v,
            _ => return,
        };
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let mut stream = stream;
        let mut buf = vec![0u8; 4096];
        let n = match stream.read(&mut buf).await {
            Ok(n) => n,
            _ => return,
        };
        let request = String::from_utf8_lossy(&buf[..n]);
        let code = match extract_code_from_request(&request) {
            Some(c) => c,
            None => return,
        };
        let client = reqwest::Client::new();
        let creds = OAuthCredentials {
            client_id: GCA_OAUTH_CLIENT_ID.to_string(),
            client_secret: Some(GCA_OAUTH_CLIENT_SECRET.to_string()),
            redirect_uri: redirect_for_exchange,
        };
        match vertex_auth::exchange_code(&client, &creds, &code, None).await {
            Ok(tokens) => {
                use tauri::Manager;
                let auth: tauri::State<'_, AuthState> = app_clone.state();
                *auth.gca_tokens.lock().unwrap() = Some(tokens);
                auth.persist_tokens(&app_clone);
                let _ = app_clone.emit("gca-auth-complete", "ok");
                let _ = stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: text/html\r\n\r\n<html><body><h2>GCA Connected!</h2><p>You can close this tab.</p></body></html>").await;
            }
            Err(e) => {
                let _ = app_clone.emit("gca-auth-complete", format!("error: {}", e));
                let body = format!("<html><body><h2>Error</h2><p>{}</p></body></html>", e);
                let _ = stream.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: text/html\r\n\r\n{}", body).as_bytes()).await;
            }
        }
    });

    Ok(auth_url)
}

fn extract_code_from_request(request: &str) -> Option<String> {
    let first_line = request.lines().next()?;
    let path = first_line.split_whitespace().nth(1)?;
    let query = path.split('?').nth(1)?;
    for param in query.split('&') {
        let mut kv = param.splitn(2, '=');
        if kv.next()? == "code" {
            return Some(kv.next()?.to_string());
        }
    }
    None
}

#[tauri::command(rename_all = "snake_case")]
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

#[tauri::command(rename_all = "snake_case")]
pub async fn gca_oauth_callback(
    code: String,
    state: State<'_, AuthState>,
    app: tauri::AppHandle,
) -> Result<String, String> {
    let creds = OAuthCredentials {
        client_id: GCA_OAUTH_CLIENT_ID.to_string(),
        client_secret: Some(GCA_OAUTH_CLIENT_SECRET.to_string()),
        redirect_uri: LAYREAM_REDIRECT_URI.to_string(),
    };
    let client = reqwest::Client::new();
    let tokens = vertex_auth::exchange_code(&client, &creds, &code, None)
        .await
        .map_err(|e| e.to_string())?;
    *state.gca_tokens.lock().unwrap() = Some(tokens);
    state.persist_tokens(&app);
    Ok("GCA connected".into())
}

#[tauri::command(rename_all = "snake_case")]
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

#[tauri::command(rename_all = "snake_case")]
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

#[tauri::command(rename_all = "snake_case")]
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

#[tauri::command(rename_all = "snake_case")]
pub fn vertex_oauth_disconnect(state: State<'_, AuthState>, app: tauri::AppHandle) -> String {
    *state.vertex_tokens.lock().unwrap() = None;
    *state.vertex_pkce.lock().unwrap() = None;
    state.persist_tokens(&app);
    "Disconnected".into()
}

#[tauri::command(rename_all = "snake_case")]
pub fn gca_oauth_disconnect(state: State<'_, AuthState>, app: tauri::AppHandle) -> String {
    *state.gca_tokens.lock().unwrap() = None;
    state.persist_tokens(&app);
    "Disconnected".into()
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

#[tauri::command(rename_all = "snake_case")]
pub fn get_request_logs(state: State<'_, RequestLogState>) -> Vec<Value> {
    state.logs.lock().unwrap().clone()
}

#[tauri::command(rename_all = "snake_case")]
pub fn clear_request_logs(state: State<'_, RequestLogState>) {
    state.logs.lock().unwrap().clear();
}

#[tauri::command(rename_all = "snake_case")]
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

#[tauri::command(rename_all = "snake_case")]
pub fn cmd_save_settings(settings: Value, app: tauri::AppHandle) -> Result<(), String> {
    let data_dir = persistence::get_data_dir(&app)?;
    persistence::save_settings(&data_dir, &settings)
}

#[tauri::command(rename_all = "snake_case")]
pub fn cmd_load_settings(app: tauri::AppHandle) -> Result<Value, String> {
    let data_dir = persistence::get_data_dir(&app)?;
    persistence::load_settings(&data_dir)
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

#[tauri::command(rename_all = "snake_case")]
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

#[tauri::command(rename_all = "snake_case")]
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

#[tauri::command(rename_all = "snake_case")]
pub async fn open_url(app: tauri::AppHandle, url: String) -> Result<(), String> {
    #[cfg(target_os = "android")]
    {
        let handle = app.state::<crate::browser::BrowserHandle<tauri::Wry>>();
        handle
            .0
            .run_mobile_plugin::<()>("openBrowser", url)
            .map_err(|e| e.to_string())
    }
    #[cfg(not(target_os = "android"))]
    {
        use tauri_plugin_shell::ShellExt;
        #[allow(deprecated)]
        app.shell().open(&url, None).map_err(|e| e.to_string())
    }
}

#[tauri::command(rename_all = "snake_case")]
pub async fn start_streaming(app: tauri::AppHandle, text: Option<String>) -> Result<(), String> {
    #[cfg(target_os = "android")]
    {
        let handle = app.state::<crate::streaming_service::StreamingServiceHandle<tauri::Wry>>();
        let payload = text.unwrap_or_else(|| "AI 응답 수신 중...".to_string());
        handle
            .0
            .run_mobile_plugin::<()>("startStreaming", payload)
            .map_err(|e| e.to_string())
    }
    #[cfg(not(target_os = "android"))]
    {
        let _ = app;
        let _ = text;
        Ok(())
    }
}

#[tauri::command(rename_all = "snake_case")]
pub async fn stop_streaming(app: tauri::AppHandle) -> Result<(), String> {
    #[cfg(target_os = "android")]
    {
        let handle = app.state::<crate::streaming_service::StreamingServiceHandle<tauri::Wry>>();
        handle
            .0
            .run_mobile_plugin::<()>("stopStreaming", ())
            .map_err(|e| e.to_string())
    }
    #[cfg(not(target_os = "android"))]
    {
        let _ = app;
        Ok(())
    }
}

#[tauri::command(rename_all = "snake_case")]
pub async fn update_notification(app: tauri::AppHandle, text: String) -> Result<(), String> {
    #[cfg(target_os = "android")]
    {
        let handle = app.state::<crate::streaming_service::StreamingServiceHandle<tauri::Wry>>();
        handle
            .0
            .run_mobile_plugin::<()>("updateNotification", text)
            .map_err(|e| e.to_string())
    }
    #[cfg(not(target_os = "android"))]
    {
        let _ = app;
        let _ = text;
        Ok(())
    }
}

#[tauri::command(rename_all = "snake_case")]
pub fn cmd_save_current_preset(preset: Value, app: tauri::AppHandle) -> Result<(), String> {
    let data_dir = persistence::get_data_dir(&app)?;
    persistence::save_current_preset(&data_dir, &preset)
}

#[tauri::command(rename_all = "snake_case")]
pub fn cmd_load_current_preset(app: tauri::AppHandle) -> Result<Value, String> {
    let data_dir = persistence::get_data_dir(&app)?;
    persistence::load_current_preset(&data_dir)
}

#[tauri::command(rename_all = "snake_case")]
pub fn cmd_save_session(session: Value, app: tauri::AppHandle) -> Result<(), String> {
    let data_dir = persistence::get_data_dir(&app)?;
    persistence::save_session(&data_dir, &session)
}

#[tauri::command(rename_all = "snake_case")]
pub fn cmd_load_session(app: tauri::AppHandle) -> Result<Value, String> {
    let data_dir = persistence::get_data_dir(&app)?;
    persistence::load_session(&data_dir)
}

const USER_MSG_SYSTEM_PROMPT: &str =
    "Given this roleplay conversation context, generate a short, natural next user message. \
     Reply ONLY with the message text, nothing else.";

const USER_MSG_MAX_TOKENS: u32 = 256;
const USER_MSG_TEMPERATURE: f64 = 1.0;

#[tauri::command(rename_all = "snake_case")]
pub async fn generate_user_message(
    context: Vec<Value>,
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
                .await
                .map_err(|e| e.to_string())?;
            if valid_tokens.access_token != tokens.access_token {
                *state.vertex_tokens.lock().unwrap() = Some(valid_tokens.clone());
                state.persist_tokens(&app);
            }

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
                safety_settings: build_safety_settings(),
                generation_config: GenerationConfig {
                    max_output_tokens: USER_MSG_MAX_TOKENS,
                    temperature: USER_MSG_TEMPERATURE,
                    thinking_config: None,
                    top_p: None,
                    top_k: None,
                    response_mime_type: response_mime_type.clone(),
                    response_schema: response_schema.clone(),
                },
                tools: None,
            };

            vertex_api::generate_non_streaming(
                &client,
                &valid_tokens.access_token,
                &project_id,
                region,
                &model,
                &request,
            )
            .await
            .map_err(|e| e.to_string())
        }
        "gca" => {
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
                .await
                .map_err(|e| e.to_string())?;
            if valid_tokens.access_token != tokens.access_token {
                *state.gca_tokens.lock().unwrap() = Some(valid_tokens.clone());
                state.persist_tokens(&app);
            }

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
                safety_settings: build_safety_settings(),
                generation_config: GenerationConfig {
                    max_output_tokens: USER_MSG_MAX_TOKENS,
                    temperature: USER_MSG_TEMPERATURE,
                    thinking_config: None,
                    top_p: None,
                    top_k: None,
                    response_mime_type: response_mime_type.clone(),
                    response_schema: response_schema.clone(),
                },
                tools: None,
            };

            gca::generate_non_streaming(
                &client,
                &valid_tokens.access_token,
                &model,
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

#[tauri::command(rename_all = "snake_case")]
pub fn save_file_to_downloads(
    filename: String,
    data: Vec<u8>,
    app: tauri::AppHandle,
) -> Result<String, String> {
    use std::path::PathBuf;
    let mut candidates: Vec<PathBuf> = Vec::new();

    #[cfg(target_os = "android")]
    {
        candidates.push(PathBuf::from("/sdcard/Download"));
        candidates.push(PathBuf::from("/storage/emulated/0/Download"));
    }

    #[cfg(not(target_os = "android"))]
    {
        if let Ok(dir) = app.path().download_dir() {
            candidates.push(dir);
        }
    }

    if let Ok(dir) = app.path().app_data_dir() {
        candidates.push(dir.join("exports"));
    }

    let mut last_err = String::from("no candidate directory");
    for dir in candidates {
        if let Err(e) = std::fs::create_dir_all(&dir) {
            last_err = format!("mkdir {}: {}", dir.display(), e);
            continue;
        }
        let path = dir.join(&filename);
        match std::fs::write(&path, &data) {
            Ok(()) => return Ok(path.to_string_lossy().into_owned()),
            Err(e) => last_err = format!("write {}: {}", path.display(), e),
        }
    }
    Err(last_err)
}

#[tauri::command(rename_all = "snake_case")]
pub fn get_asset_data(
    asset_name: String,
    state: State<'_, CharacterAssetsState>,
) -> Result<String, String> {
    use base64::Engine;
    let guard = state.assets.lock().unwrap();
    let data = guard.get(&asset_name).ok_or("Asset not found")?;
    Ok(base64::engine::general_purpose::STANDARD.encode(data))
}

#[tauri::command(rename_all = "snake_case")]
pub fn cmd_save_current_character(character: Value, app: tauri::AppHandle) -> Result<(), String> {
    let data_dir = persistence::get_data_dir(&app)?;
    persistence::save_current_character(&data_dir, &character)
}

#[tauri::command(rename_all = "snake_case")]
pub fn cmd_load_current_character(app: tauri::AppHandle) -> Result<Value, String> {
    let data_dir = persistence::get_data_dir(&app)?;
    persistence::load_current_character(&data_dir)
}

// ──────────────────────────────────────────────────────────────────────────
// Library commands. Three kinds × four ops = twelve commands. Each kind
// gets the same shape (save/list/load/delete) so callers can write a
// generic helper on the JS side. The inner `library_*` functions in
// persistence enforce id safety and atomic writes.
// ──────────────────────────────────────────────────────────────────────────

#[tauri::command(rename_all = "snake_case")]
pub fn library_save_preset(name: String, data: Value, app: tauri::AppHandle) -> Result<String, String> {
    let data_dir = persistence::get_data_dir(&app)?;
    persistence::library_save(&data_dir, persistence::LIB_KIND_PRESET, &name, &data)
}

#[tauri::command(rename_all = "snake_case")]
pub fn library_list_presets(app: tauri::AppHandle) -> Result<Value, String> {
    let data_dir = persistence::get_data_dir(&app)?;
    let items = persistence::library_list(&data_dir, persistence::LIB_KIND_PRESET)?;
    Ok(Value::Array(items))
}

#[tauri::command(rename_all = "snake_case")]
pub fn library_load_preset(id: String, app: tauri::AppHandle) -> Result<Value, String> {
    let data_dir = persistence::get_data_dir(&app)?;
    persistence::library_load(&data_dir, persistence::LIB_KIND_PRESET, &id)
}

#[tauri::command(rename_all = "snake_case")]
pub fn library_delete_preset(id: String, app: tauri::AppHandle) -> Result<(), String> {
    let data_dir = persistence::get_data_dir(&app)?;
    persistence::library_delete(&data_dir, persistence::LIB_KIND_PRESET, &id)
}

#[tauri::command(rename_all = "snake_case")]
pub fn library_save_character(name: String, data: Value, app: tauri::AppHandle) -> Result<String, String> {
    let data_dir = persistence::get_data_dir(&app)?;
    persistence::library_save(&data_dir, persistence::LIB_KIND_CHARACTER, &name, &data)
}

#[tauri::command(rename_all = "snake_case")]
pub fn library_list_characters(app: tauri::AppHandle) -> Result<Value, String> {
    let data_dir = persistence::get_data_dir(&app)?;
    let items = persistence::library_list(&data_dir, persistence::LIB_KIND_CHARACTER)?;
    Ok(Value::Array(items))
}

#[tauri::command(rename_all = "snake_case")]
pub fn library_load_character(id: String, app: tauri::AppHandle) -> Result<Value, String> {
    let data_dir = persistence::get_data_dir(&app)?;
    persistence::library_load(&data_dir, persistence::LIB_KIND_CHARACTER, &id)
}

#[tauri::command(rename_all = "snake_case")]
pub fn library_delete_character(id: String, app: tauri::AppHandle) -> Result<(), String> {
    let data_dir = persistence::get_data_dir(&app)?;
    persistence::library_delete(&data_dir, persistence::LIB_KIND_CHARACTER, &id)
}

#[tauri::command(rename_all = "snake_case")]
pub fn library_save_module(name: String, data: Value, app: tauri::AppHandle) -> Result<String, String> {
    let data_dir = persistence::get_data_dir(&app)?;
    persistence::library_save(&data_dir, persistence::LIB_KIND_MODULE, &name, &data)
}

#[tauri::command(rename_all = "snake_case")]
pub fn library_list_modules(app: tauri::AppHandle) -> Result<Value, String> {
    let data_dir = persistence::get_data_dir(&app)?;
    let items = persistence::library_list(&data_dir, persistence::LIB_KIND_MODULE)?;
    Ok(Value::Array(items))
}

#[tauri::command(rename_all = "snake_case")]
pub fn library_load_module(id: String, app: tauri::AppHandle) -> Result<Value, String> {
    let data_dir = persistence::get_data_dir(&app)?;
    persistence::library_load(&data_dir, persistence::LIB_KIND_MODULE, &id)
}

#[tauri::command(rename_all = "snake_case")]
pub fn library_delete_module(id: String, app: tauri::AppHandle) -> Result<(), String> {
    let data_dir = persistence::get_data_dir(&app)?;
    persistence::library_delete(&data_dir, persistence::LIB_KIND_MODULE, &id)
}
