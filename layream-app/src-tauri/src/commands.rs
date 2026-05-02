use layream_core::cbs::parser::{CbsContext, evaluate};
use layream_core::charx;
use layream_core::preset;
use layream_core::types::BotPreset;
use layream_core::vertex_auth::{
    self, OAuthCredentials, PkceChallenge, Tokens,
    VERTEX_CLIENT_ID, LAYREAM_REDIRECT_URI,
};
use layream_core::gca::GCA_OAUTH_CLIENT_ID;
use serde_json::Value;
use std::sync::Mutex;
use tauri::State;

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

#[tauri::command]
pub async fn chat_send(_message: String) -> Result<String, String> {
    Err("Not configured. Set up API provider in Settings first.".into())
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
pub async fn vertex_oauth_callback(code: String, state: State<'_, AuthState>) -> Result<String, String> {
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
    Ok("Vertex AI connected".into())
}

#[tauri::command]
pub async fn gca_oauth_callback(code: String, state: State<'_, AuthState>) -> Result<String, String> {
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
pub async fn vertex_list_projects(state: State<'_, AuthState>) -> Result<Value, String> {
    let token = {
        let tokens = state.vertex_tokens.lock().unwrap();
        tokens.as_ref().ok_or("Not connected")?.access_token.clone()
    };
    let client = reqwest::Client::new();
    let projects = layream_core::vertex_auth::list_gcp_projects(&client, &token)
        .await
        .map_err(|e| e.to_string())?;
    serde_json::to_value(&projects).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn vertex_oauth_disconnect(state: State<'_, AuthState>) -> String {
    *state.vertex_tokens.lock().unwrap() = None;
    *state.vertex_pkce.lock().unwrap() = None;
    "Disconnected".into()
}

#[tauri::command]
pub fn gca_oauth_disconnect(state: State<'_, AuthState>) -> String {
    *state.gca_tokens.lock().unwrap() = None;
    *state.gca_pkce.lock().unwrap() = None;
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
pub async fn vertex_list_models(access_token: String, region: String) -> Result<Value, String> {
    let client = reqwest::Client::new();
    let models = layream_core::vertex_api::list_models(&client, &access_token, &region)
        .await
        .map_err(|e| e.to_string())?;
    serde_json::to_value(&models).map_err(|e| e.to_string())
}
