use layream_core::crypto;
use layream_core::vertex_auth::Tokens;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use tauri::Manager;

const TOKEN_FILE: &str = "tokens.json";
const SETTINGS_FILE: &str = "settings.json";
const ENCRYPTION_KEY: &str = "layream-token-store-v1";

#[derive(Debug, Serialize, Deserialize)]
struct StoredTokens {
    vertex: Option<Tokens>,
    gca: Option<Tokens>,
}

pub fn save_tokens(data_dir: &Path, vertex: &Option<Tokens>, gca: &Option<Tokens>) -> Result<(), String> {
    fs::create_dir_all(data_dir).map_err(|e| e.to_string())?;

    let stored = StoredTokens {
        vertex: vertex.clone(),
        gca: gca.clone(),
    };
    let json = serde_json::to_string(&stored).map_err(|e| e.to_string())?;
    let encrypted = crypto::encrypt(json.as_bytes(), ENCRYPTION_KEY).map_err(|e| format!("{:?}", e))?;

    let path = data_dir.join(TOKEN_FILE);
    fs::write(&path, &encrypted).map_err(|e| e.to_string())?;
    Ok(())
}

pub fn load_tokens(data_dir: &Path) -> Result<(Option<Tokens>, Option<Tokens>), String> {
    let path = data_dir.join(TOKEN_FILE);
    if !path.exists() {
        return Ok((None, None));
    }

    let encrypted = fs::read(&path).map_err(|e| e.to_string())?;
    let decrypted = crypto::decrypt(&encrypted, ENCRYPTION_KEY).map_err(|e| format!("{:?}", e))?;
    let json = String::from_utf8(decrypted).map_err(|e| e.to_string())?;
    let stored: StoredTokens = serde_json::from_str(&json).map_err(|e| e.to_string())?;

    Ok((stored.vertex, stored.gca))
}

pub fn save_settings(data_dir: &Path, settings: &Value) -> Result<(), String> {
    fs::create_dir_all(data_dir).map_err(|e| e.to_string())?;

    let json = serde_json::to_string_pretty(settings).map_err(|e| e.to_string())?;
    let path = data_dir.join(SETTINGS_FILE);
    fs::write(&path, json.as_bytes()).map_err(|e| e.to_string())?;
    Ok(())
}

pub fn load_settings(data_dir: &Path) -> Result<Value, String> {
    let path = data_dir.join(SETTINGS_FILE);
    if !path.exists() {
        return Ok(Value::Object(Default::default()));
    }

    let json = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    serde_json::from_str(&json).map_err(|e| e.to_string())
}

const HYPA_FILE: &str = "hypa.json";

pub fn save_hypa(data_dir: &Path, summaries: &Value) -> Result<(), String> {
    fs::create_dir_all(data_dir).map_err(|e| e.to_string())?;
    let json = serde_json::to_string_pretty(summaries).map_err(|e| e.to_string())?;
    let path = data_dir.join(HYPA_FILE);
    fs::write(&path, json.as_bytes()).map_err(|e| e.to_string())?;
    Ok(())
}

pub fn load_hypa(data_dir: &Path) -> Result<Value, String> {
    let path = data_dir.join(HYPA_FILE);
    if !path.exists() {
        return Ok(serde_json::json!({ "summaries": [] }));
    }
    let json = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    serde_json::from_str(&json).map_err(|e| e.to_string())
}

const PRESET_FILE: &str = "current_preset.json";

pub fn save_current_preset(data_dir: &Path, preset: &Value) -> Result<(), String> {
    fs::create_dir_all(data_dir).map_err(|e| e.to_string())?;
    let json = serde_json::to_string_pretty(preset).map_err(|e| e.to_string())?;
    fs::write(data_dir.join(PRESET_FILE), json.as_bytes()).map_err(|e| e.to_string())?;
    Ok(())
}

pub fn load_current_preset(data_dir: &Path) -> Result<Value, String> {
    let path = data_dir.join(PRESET_FILE);
    if !path.exists() {
        return Ok(Value::Null);
    }
    let json = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    serde_json::from_str(&json).map_err(|e| e.to_string())
}

const SESSION_FILE: &str = "session.json";

pub fn save_session(data_dir: &Path, session: &Value) -> Result<(), String> {
    fs::create_dir_all(data_dir).map_err(|e| e.to_string())?;
    let json = serde_json::to_string_pretty(session).map_err(|e| e.to_string())?;
    fs::write(data_dir.join(SESSION_FILE), json.as_bytes()).map_err(|e| e.to_string())?;
    Ok(())
}

pub fn load_session(data_dir: &Path) -> Result<Value, String> {
    let path = data_dir.join(SESSION_FILE);
    if !path.exists() {
        return Ok(serde_json::json!({ "messages": [] }));
    }
    let json = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    serde_json::from_str(&json).map_err(|e| e.to_string())
}

pub fn get_data_dir(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_data_dir()
        .map_err(|e| e.to_string())
}
