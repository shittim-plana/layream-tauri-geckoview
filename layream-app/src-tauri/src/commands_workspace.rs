use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

use crate::commands_settings::SessionData;
use crate::persistence;

/// Workspace metadata for create/update operations.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct WorkspaceData {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub character_id: Option<String>,
    #[serde(default)]
    pub preset_id: Option<String>,
    #[serde(default)]
    pub module_ids: Option<Vec<String>>,
    #[serde(default)]
    pub provider: Option<String>,
    #[serde(flatten)]
    pub extra: HashMap<String, Value>,
}

// ──────────────────────────────────────────────────────────────────────────
// Workspace commands. CRUD for workspaces + per-workspace session/hypa.
// ──────────────────────────────────────────────────────────────────────────

#[tauri::command(rename_all = "snake_case")]
pub fn cmd_workspace_create(name: String, app: tauri::AppHandle) -> Result<String, String> {
    let data_dir = persistence::get_data_dir(&app)?;
    persistence::workspace_create(&data_dir, &name)
}

#[tauri::command(rename_all = "snake_case")]
pub fn cmd_workspace_list(app: tauri::AppHandle) -> Result<Value, String> {
    let data_dir = persistence::get_data_dir(&app)?;
    let items = persistence::workspace_list(&data_dir)?;
    Ok(Value::Array(items))
}

#[tauri::command(rename_all = "snake_case")]
pub fn cmd_workspace_load(id: String, app: tauri::AppHandle) -> Result<Value, String> {
    let data_dir = persistence::get_data_dir(&app)?;
    persistence::workspace_load(&data_dir, &id)
}

#[tauri::command(rename_all = "snake_case")]
pub fn cmd_workspace_update(id: String, data: WorkspaceData, app: tauri::AppHandle) -> Result<(), String> {
    let data_dir = persistence::get_data_dir(&app)?;
    let value = serde_json::to_value(&data).map_err(|e| e.to_string())?;
    persistence::workspace_update(&data_dir, &id, &value)
}

#[tauri::command(rename_all = "snake_case")]
pub fn cmd_workspace_delete(id: String, app: tauri::AppHandle) -> Result<(), String> {
    let data_dir = persistence::get_data_dir(&app)?;
    persistence::workspace_delete(&data_dir, &id)
}

#[tauri::command(rename_all = "snake_case")]
pub fn cmd_workspace_save_session_ws(id: String, session: SessionData, app: tauri::AppHandle) -> Result<(), String> {
    let data_dir = persistence::get_data_dir(&app)?;
    let value = serde_json::to_value(&session).map_err(|e| e.to_string())?;
    persistence::workspace_save_session(&data_dir, &id, &value)
}

#[tauri::command(rename_all = "snake_case")]
pub fn cmd_workspace_load_session_ws(id: String, app: tauri::AppHandle) -> Result<SessionData, String> {
    let data_dir = persistence::get_data_dir(&app)?;
    let value = persistence::workspace_load_session(&data_dir, &id)?;
    serde_json::from_value(value).map_err(|e| e.to_string())
}
