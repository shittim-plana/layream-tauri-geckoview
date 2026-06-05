use layream_core::charx;
use layream_core::preset;
use layream_core::types::BotPreset;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Mutex;
use tauri::State;

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

pub(crate) fn is_safe_filename(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 255
        && !name.contains('/')
        && !name.contains('\\')
        && !name.contains("..")
        && name != "."
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
    if !is_safe_filename(&temp_name) { return Err(format!("Invalid temp filename: {}", temp_name)); }
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
    if !is_safe_filename(&temp_name) { return Err(format!("Invalid temp filename: {}", temp_name)); }
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
    if !is_safe_filename(&temp_name) { return Err(format!("Invalid temp filename: {}", temp_name)); }
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

#[tauri::command(rename_all = "snake_case")]
pub fn cmd_save_module(id: String, name: String, data: Value, app: tauri::AppHandle) -> Result<(), String> {
    let data_dir = persistence::get_data_dir(&app)?;
    persistence::library_update(&data_dir, persistence::LIB_KIND_MODULE, &id, &name, &data)
}

#[tauri::command(rename_all = "snake_case")]
pub fn cmd_load_modules(ids: Vec<String>, app: tauri::AppHandle) -> Result<Vec<Value>, String> {
    let data_dir = persistence::get_data_dir(&app)?;
    let mut results = Vec::new();
    for id in &ids {
        match persistence::library_load(&data_dir, persistence::LIB_KIND_MODULE, id) {
            Ok(v) => results.push(v),
            Err(e) => log::warn!("Failed to load module {}: {}", id, e),
        }
    }
    Ok(results)
}
