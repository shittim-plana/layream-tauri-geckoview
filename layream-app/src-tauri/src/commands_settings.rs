use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

use crate::persistence;

/// Session data persisted between app restarts.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionData {
    #[serde(default)]
    pub messages: Vec<Value>,
    #[serde(default)]
    pub active_toggles: Option<HashMap<String, bool>>,
    #[serde(default)]
    pub branches: Option<Vec<Value>>,
    #[serde(default)]
    pub active_branch_id: Option<String>,
    #[serde(flatten)]
    pub extra: HashMap<String, Value>,
}

/// Application settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    #[serde(default)]
    pub user_name: Option<String>,
    #[serde(default)]
    pub chat_provider: Option<String>,
    #[serde(default)]
    pub summary_provider: Option<String>,
    #[serde(default)]
    pub embedding_provider: Option<String>,
    #[serde(default)]
    pub vertex_project_id: Option<String>,
    #[serde(default)]
    pub vertex_region: Option<String>,
    #[serde(default)]
    pub vertex_model: Option<String>,
    #[serde(default)]
    pub vertex_embedding_model: Option<String>,
    #[serde(default)]
    pub vertex_config: Option<Value>,
    #[serde(default)]
    pub gca_model: Option<String>,
    #[serde(default)]
    pub gca_config: Option<Value>,
    #[serde(default)]
    pub gca_project: Option<String>,
    #[serde(default)]
    pub mistral_key: Option<String>,
    #[serde(default)]
    pub mistral_model: Option<String>,
    #[serde(default)]
    pub mistral_config: Option<Value>,
    #[serde(default)]
    pub voyage_key: Option<String>,
    #[serde(default)]
    pub voyage_model: Option<String>,
    #[serde(default)]
    pub hypa: Option<Value>,
    #[serde(flatten)]
    pub extra: HashMap<String, Value>,
}

#[tauri::command(rename_all = "snake_case")]
pub fn cmd_save_settings(settings: AppSettings, app: tauri::AppHandle) -> Result<(), String> {
    let data_dir = persistence::get_data_dir(&app)?;
    let value = serde_json::to_value(&settings).map_err(|e| e.to_string())?;
    persistence::save_settings(&data_dir, &value)
}

#[tauri::command(rename_all = "snake_case")]
pub fn cmd_load_settings(app: tauri::AppHandle) -> Result<AppSettings, String> {
    let data_dir = persistence::get_data_dir(&app)?;
    let value = persistence::load_settings(&data_dir)?;
    serde_json::from_value(value).map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub fn cmd_save_personas(personas: Value, app: tauri::AppHandle) -> Result<(), String> {
    let data_dir = persistence::get_data_dir(&app)?;
    persistence::save_personas(&data_dir, &personas)
}

#[tauri::command(rename_all = "snake_case")]
pub fn cmd_load_personas(app: tauri::AppHandle) -> Result<Value, String> {
    let data_dir = persistence::get_data_dir(&app)?;
    persistence::load_personas(&data_dir)
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
pub fn cmd_save_session(session: SessionData, app: tauri::AppHandle) -> Result<(), String> {
    let data_dir = persistence::get_data_dir(&app)?;
    let value = serde_json::to_value(&session).map_err(|e| e.to_string())?;
    persistence::save_session(&data_dir, &value)
}

#[tauri::command(rename_all = "snake_case")]
pub fn cmd_load_session(app: tauri::AppHandle) -> Result<SessionData, String> {
    let data_dir = persistence::get_data_dir(&app)?;
    let value = persistence::load_session(&data_dir)?;
    serde_json::from_value(value).map_err(|e| e.to_string())
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
