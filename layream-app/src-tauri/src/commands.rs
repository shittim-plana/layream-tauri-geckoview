use layream_core::cbs::parser::{CbsContext, evaluate};
use layream_core::charx;
use layream_core::preset;
use layream_core::types::BotPreset;
use serde_json::Value;

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
    Err("Not configured. Set up Vertex AI in Settings first.".into())
}

#[tauri::command]
pub fn oauth_start(_project_id: String, _region: String) -> Result<String, String> {
    Err("OAuth configuration needed. Provide client_id and client_secret.".into())
}

#[tauri::command]
pub fn oauth_status() -> String {
    "Not connected".into()
}
