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
use layream_core::retry;

const GCA_REDIRECT_URI: &str = "com.googleusercontent.apps.681255809395-oo8ft2oprdrnp9e3aqf6av3hmdib135j:/oauth2callback";
use layream_core::voyage;
use serde_json::Value;
use std::sync::Mutex;
use tauri::{Emitter, Manager, State};

use std::collections::HashMap;

use crate::persistence;

pub struct CharacterAssetsState {
    pub assets: Mutex<HashMap<String, Vec<u8>>>,
    pub charx_path: Mutex<Option<std::path::PathBuf>>,
}

impl Default for CharacterAssetsState {
    fn default() -> Self {
        Self {
            assets: Mutex::new(HashMap::new()),
            charx_path: Mutex::new(None),
        }
    }
}

pub struct AuthState {
    pub vertex_tokens: Mutex<Option<Tokens>>,
    pub gca_tokens: Mutex<Option<Tokens>>,
    pub vertex_pkce: Mutex<Option<PkceChallenge>>,
}

impl Default for AuthState {
    fn default() -> Self {
        Self {
            vertex_tokens: Mutex::new(None),
            gca_tokens: Mutex::new(None),
            vertex_pkce: Mutex::new(None),
        }
    }
}

impl AuthState {
    pub fn persist_tokens(&self, app: &tauri::AppHandle) {
        match persistence::get_data_dir(app) {
            Ok(data_dir) => {
                match (self.vertex_tokens.lock(), self.gca_tokens.lock()) {
                    (Ok(vertex), Ok(gca)) => {
                        if let Err(e) = persistence::save_tokens(&data_dir, &vertex.clone(), &gca.clone()) {
                            log::warn!("Failed to save tokens: {e}");
                        }
                    }
                    _ => log::warn!("Failed to lock token state for persistence"),
                }
            }
            Err(e) => log::warn!("Failed to get data dir for token persistence: {e}"),
        }
    }

    pub fn load_persisted_tokens(&self, app: &tauri::AppHandle) {
        if let Ok(data_dir) = persistence::get_data_dir(app) {
            match persistence::load_tokens(&data_dir) {
                Ok((vertex, gca)) => {
                    if let Ok(mut guard) = self.vertex_tokens.lock() {
                        *guard = vertex;
                    } else {
                        log::warn!("Failed to lock vertex_tokens for loading");
                    }
                    if let Ok(mut guard) = self.gca_tokens.lock() {
                        *guard = gca;
                    } else {
                        log::warn!("Failed to lock gca_tokens for loading");
                    }
                }
                Err(e) => log::debug!("No saved tokens found: {e}"),
            }
        }
    }
}

pub struct StreamCancelState {
    pub token: Mutex<Option<retry::CancelToken>>,
}

impl Default for StreamCancelState {
    fn default() -> Self {
        Self { token: Mutex::new(None) }
    }
}

#[tauri::command(rename_all = "snake_case")]
pub fn cancel_chat(state: State<'_, StreamCancelState>) -> Result<(), String> {
    let guard = state.token.lock().map_err(|e| format!("lock poisoned: {e}"))?;
    if let Some(token) = guard.as_ref() {
        token.store(true, std::sync::atomic::Ordering::Relaxed);
    }
    Ok(())
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
    app: tauri::AppHandle,
) -> Result<Value, String> {
    let is_charx = {
        let lower = name.to_lowercase();
        lower.ends_with(".charx") || lower.ends_with(".jpeg")
    };

    if is_charx {
        let data_dir = persistence::get_data_dir(&app)?;
        let charx_store = data_dir.join("charx_cache.bin");
        let data_clone = data.clone();
        let store_path = charx_store.clone();

        let result = tokio::task::spawn_blocking(move || -> Result<_, String> {
            std::fs::write(&store_path, &data_clone)
                .map_err(|e| format!("Save charx cache: {}", e))?;
            let meta = charx::read_charx_metadata(&data_clone).map_err(|e| e.to_string())?;
            let card_json = match &meta.card {
                Some(charx::CardData::V2(card)) => serde_json::to_value(card).ok(),
                Some(charx::CardData::OldTavern(card)) => serde_json::to_value(card).ok(),
                None => None,
            };
            let json = serde_json::json!({
                "card": card_json,
                "assetCount": meta.asset_list.len(),
                "assetList": meta.asset_list,
                "hasModule": meta.module_data.is_some(),
            });
            Ok(json)
        })
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())?;

        *asset_state.assets.lock().map_err(|e| format!("lock poisoned: {e}"))? = HashMap::new();
        *asset_state.charx_path.lock().map_err(|e| format!("lock poisoned: {e}"))? = Some(charx_store);
        Ok(result)
    } else {
        let result = tokio::task::spawn_blocking(move || -> Result<_, String> {
            let ch = charx::read_character(&name, &data).map_err(|e| e.to_string())?;
            let card_json = match &ch.card {
                Some(charx::CardData::V2(card)) => serde_json::to_value(card).ok(),
                Some(charx::CardData::OldTavern(card)) => serde_json::to_value(card).ok(),
                None => None,
            };
            let asset_list: Vec<Value> = ch.assets.iter().map(|(name, data)| {
                serde_json::json!({"name": name, "size": data.len()})
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

        *asset_state.assets.lock().map_err(|e| format!("lock poisoned: {e}"))? = result.1;
        *asset_state.charx_path.lock().map_err(|e| format!("lock poisoned: {e}"))? = None;
        Ok(result.0)
    }
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
    if let Err(e) = std::fs::remove_file(&path) { log::warn!("cleanup temp file {}: {e}", path.display()); }
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

    let is_charx = {
        let lower = name.to_lowercase();
        lower.ends_with(".charx") || lower.ends_with(".jpeg")
    };

    if is_charx {
        let charx_store = data_dir.join("charx_cache.bin");
        std::fs::rename(&path, &charx_store)
            .or_else(|_| std::fs::copy(&path, &charx_store).map(|_| ()))
            .map_err(|e| format!("Move charx to cache: {}", e))?;
        if let Err(e) = std::fs::remove_file(&path) { log::warn!("cleanup temp file {}: {e}", path.display()); }

        let store_clone = charx_store.clone();
        let result = tokio::task::spawn_blocking(move || -> Result<_, String> {
            let data = std::fs::read(&store_clone).map_err(|e| e.to_string())?;
            let meta = charx::read_charx_metadata(&data).map_err(|e| e.to_string())?;
            let card_json = match &meta.card {
                Some(charx::CardData::V2(card)) => serde_json::to_value(card).ok(),
                Some(charx::CardData::OldTavern(card)) => serde_json::to_value(card).ok(),
                None => None,
            };
            let json = serde_json::json!({
                "card": card_json,
                "assetCount": meta.asset_list.len(),
                "assetList": meta.asset_list,
                "hasModule": meta.module_data.is_some(),
            });
            Ok(json)
        }).await.map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())?;

        *asset_state.assets.lock().map_err(|e| format!("lock poisoned: {e}"))? = HashMap::new();
        *asset_state.charx_path.lock().map_err(|e| format!("lock poisoned: {e}"))? = Some(charx_store);
        Ok(result)
    } else {
        let data = std::fs::read(&path).map_err(|e| format!("read {}: {}", path.display(), e))?;
        if let Err(e) = std::fs::remove_file(&path) { log::warn!("cleanup temp file {}: {e}", path.display()); }
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

        *asset_state.assets.lock().map_err(|e| format!("lock poisoned: {e}"))? = result.1;
        *asset_state.charx_path.lock().map_err(|e| format!("lock poisoned: {e}"))? = None;
        Ok(result.0)
    }
}

#[tauri::command(rename_all = "snake_case")]
pub async fn parse_risum_from_path(temp_name: String, app: tauri::AppHandle) -> Result<Value, String> {
    let data_dir = persistence::get_data_dir(&app)?;
    let path = data_dir.join(&temp_name);
    let data = std::fs::read(&path).map_err(|e| format!("read {}: {}", path.display(), e))?;
    if let Err(e) = std::fs::remove_file(&path) { log::warn!("cleanup temp file {}: {e}", path.display()); }
    tokio::task::spawn_blocking(move || -> Result<Value, String> {
        preset::parse_risum_data(&data).map_err(|e| e.to_string())
    }).await.map_err(|e| e.to_string())?
    .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub fn evaluate_cbs(
    input: String,
    char_name: String,
    user_name: String,
    toggles: Option<HashMap<String, String>>,
) -> String {
    let mut ctx = CbsContext {
        char_name,
        user_name,
        toggles: toggles.unwrap_or_default(),
        ..Default::default()
    };
    evaluate(&input, &mut ctx)
}

fn build_system_instruction(prompt: &Option<String>) -> Option<Content> {
    prompt.as_ref()
        .filter(|s| !s.trim().is_empty())
        .map(|s| Content {
            role: "user".to_string(),
            parts: vec![Part { text: Some(s.clone()), thought: None, inline_data: None }],
        })
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
    messages.iter().enumerate().filter_map(|(i, m)| {
        let role = m.get("role").and_then(|v| v.as_str());
        let text = m.get("text").and_then(|v| v.as_str());
        if role.is_none() || text.is_none() {
            log::warn!("messages_to_contents: skipping message[{i}] — missing role or text");
            return None;
        }
        let api_role = match role.expect("guarded by is_none check") { "char" => "model", r => r };
        Some(Content {
            role: api_role.to_string(),
            parts: vec![Part { text: Some(text.expect("guarded by is_none check").to_string()), thought: None, inline_data: None }],
        })
    }).collect()
}

fn messages_to_chat_messages(messages: &[Value]) -> Vec<mistral::ChatMessage> {
    messages.iter().enumerate().filter_map(|(i, m)| {
        let role = m.get("role").and_then(|v| v.as_str());
        let text = m.get("text").and_then(|v| v.as_str());
        if role.is_none() || text.is_none() {
            log::warn!("messages_to_chat_messages: skipping message[{i}] — missing role or text");
            return None;
        }
        let mistral_role = match role.expect("guarded by is_none check") { "model" | "char" => "assistant", r => r };
        Some(mistral::ChatMessage {
            role: mistral_role.to_string(),
            content: text.expect("guarded by is_none check").to_string(),
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
    if let Ok(mut logs) = log_state.logs.lock() {
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
}

#[tauri::command(rename_all = "snake_case")]
pub async fn chat_vertex(
    messages: Vec<Value>,
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

    let tokens = {
        let guard = state.vertex_tokens.lock().map_err(|e| format!("lock poisoned: {e}"))?;
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
        *state.vertex_tokens.lock().map_err(|e| format!("lock poisoned: {e}"))? = Some(valid_tokens.clone());
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
        system_instruction: build_system_instruction(&system_prompt),
        safety_settings: build_safety_settings(),
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

    let app_clone = app.clone();
    let on_chunk = move |text: &str| {
        let _ = app_clone.emit("chat-chunk", text);
    };

    let start = std::time::Instant::now();
    let result = vertex_api::stream_generate(
        &client, &valid_tokens.access_token, &project_id, &region, &model, &request, on_chunk,
        Some(cancel),
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

    let tokens = {
        let guard = state.gca_tokens.lock().map_err(|e| format!("lock poisoned: {e}"))?;
        guard.clone().ok_or("GCA not connected")?
    };
    let client = reqwest::Client::new();
    let creds = OAuthCredentials {
        client_id: GCA_OAUTH_CLIENT_ID.to_string(),
        client_secret: Some(GCA_OAUTH_CLIENT_SECRET.to_string()),
        redirect_uri: GCA_REDIRECT_URI.to_string(),
    };
    let valid_tokens = vertex_auth::get_valid_token(&client, &creds, &tokens)
        .await.map_err(|e| e.to_string())?;
    if valid_tokens.access_token != tokens.access_token {
        *state.gca_tokens.lock().map_err(|e| format!("lock poisoned: {e}"))? = Some(valid_tokens.clone());
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
        system_instruction: build_system_instruction(&system_prompt),
        safety_settings: build_safety_settings(),
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

    let app_clone = app.clone();
    let on_chunk = move |text: &str| {
        let _ = app_clone.emit("chat-chunk", text);
    };

    let start = std::time::Instant::now();
    let result = gca::stream_generate(
        &client, &valid_tokens.access_token, &model, &request, on_chunk,
        Some(cancel),
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
    let app_clone = app.clone();
    let on_chunk = move |text: &str| {
        let _ = app_clone.emit("chat-chunk", text);
    };

    let start = std::time::Instant::now();
    let result = mistral::chat_stream(&client, &api_key, &request, on_chunk, Some(cancel)).await;
    let elapsed = start.elapsed().as_millis();

    match &result {
        Ok(_) => log_api_call(&log_state, "mistral", &model, "ok", elapsed),
        Err(e) => log_api_call(&log_state, "mistral", &model, &format!("error: {}", e), elapsed),
    }

    result.map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub async fn vertex_oauth_start(
    state: State<'_, AuthState>,
    app: tauri::AppHandle,
) -> Result<String, String> {
    let pkce = vertex_auth::generate_pkce();
    let creds = OAuthCredentials {
        client_id: VERTEX_CLIENT_ID.to_string(),
        client_secret: None,
        redirect_uri: LAYREAM_REDIRECT_URI.to_string(),
    };
    let auth_url = vertex_auth::build_auth_url(&creds, Some(&pkce));
    *state.vertex_pkce.lock().map_err(|e| format!("lock poisoned: {e}"))? = Some(pkce.clone());
    if let Ok(data_dir) = persistence::get_data_dir(&app) {
        if let Err(e) = std::fs::write(data_dir.join("pkce_verifier.txt"), &pkce.verifier) {
            log::warn!("Failed to persist PKCE verifier: {e}");
        }
    }
    Ok(auth_url)
}

#[tauri::command(rename_all = "snake_case")]
pub async fn vertex_oauth_callback(
    code: String,
    state: State<'_, AuthState>,
    app: tauri::AppHandle,
) -> Result<String, String> {
    let verifier = state.vertex_pkce.lock().map_err(|e| format!("lock poisoned: {e}"))?.take()
        .map(|p| p.verifier)
        .or_else(|| {
            persistence::get_data_dir(&app).ok()
                .and_then(|d| std::fs::read_to_string(d.join("pkce_verifier.txt")).ok())
        })
        .ok_or("No PKCE verifier found")?;
    if let Ok(data_dir) = persistence::get_data_dir(&app) {
        if let Err(e) = std::fs::remove_file(data_dir.join("pkce_verifier.txt")) {
            log::warn!("Failed to clean PKCE verifier: {e}");
        }
    }
    let creds = OAuthCredentials {
        client_id: VERTEX_CLIENT_ID.to_string(),
        client_secret: None,
        redirect_uri: LAYREAM_REDIRECT_URI.to_string(),
    };
    let client = reqwest::Client::new();
    let tokens = vertex_auth::exchange_code(&client, &creds, &code, Some(&verifier))
        .await
        .map_err(|e| e.to_string())?;
    *state.vertex_tokens.lock().map_err(|e| format!("lock poisoned: {e}"))? = Some(tokens);
    state.persist_tokens(&app);
    Ok("Vertex AI connected".into())
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
        vertex_auth::uri_encode(GCA_OAUTH_CLIENT_ID),
        vertex_auth::uri_encode(&redirect_uri),
        vertex_auth::uri_encode(GCA_OAUTH_SCOPE),
    );

    let app_clone = app.clone();
    let redirect_for_exchange = redirect_uri.clone();
    let client = reqwest::Client::new();
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
        let creds = OAuthCredentials {
            client_id: GCA_OAUTH_CLIENT_ID.to_string(),
            client_secret: Some(GCA_OAUTH_CLIENT_SECRET.to_string()),
            redirect_uri: redirect_for_exchange,
        };
        match vertex_auth::exchange_code(&client, &creds, &code, None).await {
            Ok(tokens) => {
                let auth: tauri::State<'_, AuthState> = app_clone.state();
                if let Ok(mut guard) = auth.gca_tokens.lock() {
                    *guard = Some(tokens);
                }
                auth.persist_tokens(&app_clone);
                let _ = app_clone.emit("gca-auth-complete", "ok");
                let _ = stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: text/html\r\n\r\n<html><body><h2>GCA Connected!</h2><p>You can close this tab.</p></body></html>").await;
            }
            Err(e) => {
                log::error!("GCA token exchange failed: {:?}", e);
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
pub async fn gca_oauth_callback(
    code: String,
    state: State<'_, AuthState>,
    app: tauri::AppHandle,
) -> Result<String, String> {
    let creds = OAuthCredentials {
        client_id: GCA_OAUTH_CLIENT_ID.to_string(),
        client_secret: Some(GCA_OAUTH_CLIENT_SECRET.to_string()),
        redirect_uri: GCA_REDIRECT_URI.to_string(),
    };
    let client = reqwest::Client::new();
    let tokens = vertex_auth::exchange_code(&client, &creds, &code, None)
        .await
        .map_err(|e| e.to_string())?;
    *state.gca_tokens.lock().map_err(|e| format!("lock poisoned: {e}"))? = Some(tokens);
    state.persist_tokens(&app);
    Ok("GCA connected".into())
}

#[tauri::command(rename_all = "snake_case")]
pub fn vertex_oauth_status(state: State<'_, AuthState>) -> Result<Value, String> {
    let tokens = state.vertex_tokens.lock().map_err(|e| format!("lock poisoned: {e}"))?;
    Ok(match tokens.as_ref() {
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
    })
}

#[tauri::command(rename_all = "snake_case")]
pub fn gca_oauth_status(state: State<'_, AuthState>) -> Result<Value, String> {
    let tokens = state.gca_tokens.lock().map_err(|e| format!("lock poisoned: {e}"))?;
    Ok(match tokens.as_ref() {
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
    })
}

#[tauri::command(rename_all = "snake_case")]
pub async fn vertex_list_projects(
    state: State<'_, AuthState>,
    app: tauri::AppHandle,
) -> Result<Value, String> {
    let tokens = {
        let guard = state.vertex_tokens.lock().map_err(|e| format!("lock poisoned: {e}"))?;
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
        *state.vertex_tokens.lock().map_err(|e| format!("lock poisoned: {e}"))? = Some(valid_tokens.clone());
        state.persist_tokens(&app);
    }
    let projects = layream_core::vertex_auth::list_gcp_projects(&client, &valid_tokens.access_token)
        .await
        .map_err(|e| e.to_string())?;
    serde_json::to_value(&projects).map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub fn vertex_oauth_disconnect(state: State<'_, AuthState>, app: tauri::AppHandle) -> Result<String, String> {
    *state.vertex_tokens.lock().map_err(|e| format!("lock poisoned: {e}"))? = None;
    state.persist_tokens(&app);
    Ok("Disconnected".into())
}

#[tauri::command(rename_all = "snake_case")]
pub fn gca_oauth_disconnect(state: State<'_, AuthState>, app: tauri::AppHandle) -> Result<String, String> {
    *state.gca_tokens.lock().map_err(|e| format!("lock poisoned: {e}"))? = None;
    state.persist_tokens(&app);
    Ok("Disconnected".into())
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
        let guard = state.vertex_tokens.lock().map_err(|e| format!("lock poisoned: {e}"))?;
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
        *state.vertex_tokens.lock().map_err(|e| format!("lock poisoned: {e}"))? = Some(valid_tokens.clone());
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
pub fn get_request_logs(state: State<'_, RequestLogState>) -> Result<Vec<Value>, String> {
    Ok(state.logs.lock().map_err(|e| format!("lock poisoned: {e}"))?.clone())
}

#[tauri::command(rename_all = "snake_case")]
pub fn clear_request_logs(state: State<'_, RequestLogState>) -> Result<(), String> {
    state.logs.lock().map_err(|e| format!("lock poisoned: {e}"))?.clear();
    Ok(())
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
        let guard = state.vertex_tokens.lock().map_err(|e| format!("lock poisoned: {e}"))?;
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
        *state.vertex_tokens.lock().map_err(|e| format!("lock poisoned: {e}"))? = Some(valid_tokens.clone());
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
        let guard = state.gca_tokens.lock().map_err(|e| format!("lock poisoned: {e}"))?;
        guard.clone().ok_or("GCA not connected")?
    };
    let client = reqwest::Client::new();
    let creds = OAuthCredentials {
        client_id: GCA_OAUTH_CLIENT_ID.to_string(),
        client_secret: Some(GCA_OAUTH_CLIENT_SECRET.to_string()),
        redirect_uri: GCA_REDIRECT_URI.to_string(),
    };
    let valid_tokens = vertex_auth::get_valid_token(&client, &creds, &tokens)
        .await.map_err(|e| e.to_string())?;
    if valid_tokens.access_token != tokens.access_token {
        *state.gca_tokens.lock().map_err(|e| format!("lock poisoned: {e}"))? = Some(valid_tokens.clone());
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
        let guard = state.gca_tokens.lock().map_err(|e| format!("lock poisoned: {e}"))?;
        guard.clone().ok_or("GCA not connected")?
    };
    let client = reqwest::Client::new();
    let creds = OAuthCredentials {
        client_id: GCA_OAUTH_CLIENT_ID.to_string(),
        client_secret: Some(GCA_OAUTH_CLIENT_SECRET.to_string()),
        redirect_uri: GCA_REDIRECT_URI.to_string(),
    };
    let valid_tokens = vertex_auth::get_valid_token(&client, &creds, &tokens)
        .await.map_err(|e| e.to_string())?;
    if valid_tokens.access_token != tokens.access_token {
        *state.gca_tokens.lock().map_err(|e| format!("lock poisoned: {e}"))? = Some(valid_tokens.clone());
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
        use tauri_plugin_opener::OpenerExt;
        app.opener().open_url(&url, None::<&str>).map_err(|e| e.to_string())
    }
}

#[tauri::command(rename_all = "snake_case")]
pub async fn open_custom_tab(app: tauri::AppHandle, url: String) -> Result<(), String> {
    #[cfg(target_os = "android")]
    {
        let handle = app.state::<crate::browser::BrowserHandle<tauri::Wry>>();
        handle
            .0
            .run_mobile_plugin::<()>("openCustomTab", url)
            .map_err(|e| e.to_string())
    }
    #[cfg(not(target_os = "android"))]
    {
        use tauri_plugin_opener::OpenerExt;
        app.opener().open_url(&url, None::<&str>).map_err(|e| e.to_string())
    }
}

#[tauri::command(rename_all = "snake_case")]
pub async fn request_storage_permission(app: tauri::AppHandle) -> Result<Value, String> {
    #[cfg(target_os = "android")]
    {
        let handle = app.state::<crate::browser::BrowserHandle<tauri::Wry>>();
        handle.0.run_mobile_plugin::<Value>("requestStoragePermission", ()).map_err(|e| e.to_string())
    }
    #[cfg(not(target_os = "android"))]
    {
        let _ = app;
        Ok(serde_json::json!({"granted": true}))
    }
}

#[tauri::command(rename_all = "snake_case")]
pub async fn request_notification_permission(app: tauri::AppHandle) -> Result<Value, String> {
    #[cfg(target_os = "android")]
    {
        let handle = app.state::<crate::browser::BrowserHandle<tauri::Wry>>();
        handle.0.run_mobile_plugin::<Value>("requestNotificationPermission", ()).map_err(|e| e.to_string())
    }
    #[cfg(not(target_os = "android"))]
    {
        let _ = app;
        Ok(serde_json::json!({"granted": true}))
    }
}

#[tauri::command(rename_all = "snake_case")]
pub async fn get_pending_oauth(app: tauri::AppHandle) -> Result<Value, String> {
    #[cfg(target_os = "android")]
    {
        let handle = app.state::<crate::browser::BrowserHandle<tauri::Wry>>();
        handle.0.run_mobile_plugin::<Value>("getPendingOAuth", ()).map_err(|e| e.to_string())
    }
    #[cfg(not(target_os = "android"))]
    {
        let _ = app;
        Ok(serde_json::json!({}))
    }
}

#[tauri::command(rename_all = "snake_case")]
pub async fn list_browsers(app: tauri::AppHandle) -> Result<Value, String> {
    #[cfg(target_os = "android")]
    {
        let handle = app.state::<crate::browser::BrowserHandle<tauri::Wry>>();
        handle
            .0
            .run_mobile_plugin::<Value>("listBrowsers", ())
            .map_err(|e| e.to_string())
    }
    #[cfg(not(target_os = "android"))]
    {
        let _ = app;
        Ok(serde_json::json!({"browsers": []}))
    }
}

#[tauri::command(rename_all = "snake_case")]
pub async fn open_in_browser(app: tauri::AppHandle, url: String, package: String) -> Result<(), String> {
    #[cfg(target_os = "android")]
    {
        let handle = app.state::<crate::browser::BrowserHandle<tauri::Wry>>();
        handle
            .0
            .run_mobile_plugin::<()>("openInBrowser", format!("{}|{}", package, url))
            .map_err(|e| e.to_string())
    }
    #[cfg(not(target_os = "android"))]
    {
        use tauri_plugin_opener::OpenerExt;
        app.opener().open_url(&url, None::<&str>).map_err(|e| e.to_string())
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
                let guard = state.vertex_tokens.lock().map_err(|e| format!("lock poisoned: {e}"))?;
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
                *state.vertex_tokens.lock().map_err(|e| format!("lock poisoned: {e}"))? = Some(valid_tokens.clone());
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
                    frequency_penalty: None,
                    presence_penalty: None,
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
                let guard = state.gca_tokens.lock().map_err(|e| format!("lock poisoned: {e}"))?;
                guard.clone().ok_or("GCA not connected")?
            };
            let client = reqwest::Client::new();
            let creds = OAuthCredentials {
                client_id: GCA_OAUTH_CLIENT_ID.to_string(),
                client_secret: Some(GCA_OAUTH_CLIENT_SECRET.to_string()),
                redirect_uri: GCA_REDIRECT_URI.to_string(),
            };
            let valid_tokens = vertex_auth::get_valid_token(&client, &creds, &tokens)
                .await
                .map_err(|e| e.to_string())?;
            if valid_tokens.access_token != tokens.access_token {
                *state.gca_tokens.lock().map_err(|e| format!("lock poisoned: {e}"))? = Some(valid_tokens.clone());
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
                    frequency_penalty: None,
                    presence_penalty: None,
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

    {
        let guard = state.assets.lock().map_err(|e| format!("lock poisoned: {e}"))?;
        if let Some(data) = guard.get(&asset_name) {
            return Ok(base64::engine::general_purpose::STANDARD.encode(data));
        }
    }

    let charx_guard = state.charx_path.lock().map_err(|e| format!("lock poisoned: {e}"))?;
    if let Some(path) = charx_guard.as_ref() {
        let data = charx::read_charx_asset_from_file(path, &asset_name)
            .map_err(|e| e.to_string())?;
        return Ok(base64::engine::general_purpose::STANDARD.encode(&data));
    }

    Err("Asset not found".into())
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
