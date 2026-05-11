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
const HYPA_TMP_FILE: &str = "hypa.json.tmp";

/// Atomic write: serialize to a sibling .tmp file then rename onto the target.
/// POSIX rename is atomic within a single filesystem, so a concurrent reader
/// observes either the previous full file or the new full file — never a
/// partial write. Combined with the HypaState mutex guarding load-mutate-save,
/// this prevents lost updates and torn reads.
pub fn save_hypa(data_dir: &Path, summaries: &Value) -> Result<(), String> {
    fs::create_dir_all(data_dir).map_err(|e| e.to_string())?;
    let json = serde_json::to_string_pretty(summaries).map_err(|e| e.to_string())?;
    let tmp_path = data_dir.join(HYPA_TMP_FILE);
    let final_path = data_dir.join(HYPA_FILE);
    fs::write(&tmp_path, json.as_bytes()).map_err(|e| e.to_string())?;
    fs::rename(&tmp_path, &final_path).map_err(|e| e.to_string())?;
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

const CHARACTER_FILE: &str = "current_character.json";

pub fn save_current_character(data_dir: &Path, character: &Value) -> Result<(), String> {
    fs::create_dir_all(data_dir).map_err(|e| e.to_string())?;
    let json = serde_json::to_string_pretty(character).map_err(|e| e.to_string())?;
    fs::write(data_dir.join(CHARACTER_FILE), json.as_bytes()).map_err(|e| e.to_string())?;
    Ok(())
}

pub fn load_current_character(data_dir: &Path) -> Result<Value, String> {
    let path = data_dir.join(CHARACTER_FILE);
    if !path.exists() {
        return Ok(Value::Null);
    }
    let json = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    serde_json::from_str(&json).map_err(|e| e.to_string())
}

pub fn get_data_dir(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_data_dir()
        .map_err(|e| e.to_string())
}

// ──────────────────────────────────────────────────────────────────────────
// Library: multi-slot storage for presets / characters / modules.
// Each item is one JSON file `{ id, name, created_at, data }` under
// `$APP_DATA/library/<kind>/<id>.json`. The wrapper isolates metadata from
// payload so list operations only need the small header — payload remains
// the lossless original from the load_preset / load_character / parse_risum
// pipeline (§3-A: no demote on storage).
// ──────────────────────────────────────────────────────────────────────────

const LIBRARY_DIR: &str = "library";
pub const LIB_KIND_PRESET: &str = "presets";
pub const LIB_KIND_CHARACTER: &str = "characters";
pub const LIB_KIND_MODULE: &str = "modules";

#[derive(Debug, Serialize, Deserialize)]
struct LibraryItem {
    id: String,
    name: String,
    created_at: u64,
    data: Value,
}

fn library_dir(data_dir: &Path, kind: &str) -> PathBuf {
    data_dir.join(LIBRARY_DIR).join(kind)
}

/// Generates a sortable, collision-resistant id without pulling in `uuid`
/// or `rand`. Format: `{millis_hex}-{nanos_lo_hex}` — millisecond timestamp
/// gives chronological ordering, the low 32 bits of `nanos` provide enough
/// jitter that two saves in the same millisecond on a single device do not
/// collide. Not cryptographically random, but ids are local-only.
fn generate_id() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default();
    let millis = now.as_millis() as u64;
    let nanos_lo = (now.subsec_nanos()) as u64;
    format!("{:x}-{:08x}", millis, nanos_lo)
}

fn now_secs() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Writes a new library item and returns its id. Uses an atomic
/// tmp+rename so a crash mid-save never produces a torn file.
pub fn library_save(
    data_dir: &Path,
    kind: &str,
    name: &str,
    data: &Value,
) -> Result<String, String> {
    let dir = library_dir(data_dir, kind);
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;

    let id = generate_id();
    let item = LibraryItem {
        id: id.clone(),
        name: name.to_string(),
        created_at: now_secs(),
        data: data.clone(),
    };
    let json = serde_json::to_string_pretty(&item).map_err(|e| e.to_string())?;
    let final_path = dir.join(format!("{}.json", id));
    let tmp_path = dir.join(format!("{}.json.tmp", id));
    fs::write(&tmp_path, json.as_bytes()).map_err(|e| e.to_string())?;
    fs::rename(&tmp_path, &final_path).map_err(|e| e.to_string())?;
    Ok(id)
}

/// Lists library items as `{id, name, created_at}` headers. Skips files
/// that fail to parse rather than aborting the whole listing — a single
/// corrupt file should not make the rest of the library inaccessible.
pub fn library_list(data_dir: &Path, kind: &str) -> Result<Vec<Value>, String> {
    let dir = library_dir(data_dir, kind);
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut items: Vec<Value> = Vec::new();
    for entry in fs::read_dir(&dir).map_err(|e| e.to_string())? {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("json") {
            continue;
        }
        let json = match fs::read_to_string(&path) {
            Ok(s) => s,
            Err(_) => continue,
        };
        let item: LibraryItem = match serde_json::from_str(&json) {
            Ok(i) => i,
            Err(_) => continue,
        };
        items.push(serde_json::json!({
            "id": item.id,
            "name": item.name,
            "created_at": item.created_at,
        }));
    }
    items.sort_by(|a, b| {
        let ta = a.get("created_at").and_then(|v| v.as_u64()).unwrap_or(0);
        let tb = b.get("created_at").and_then(|v| v.as_u64()).unwrap_or(0);
        tb.cmp(&ta)
    });
    Ok(items)
}

/// Loads the full payload (the `data` field). Returns the inner data only —
/// callers want what they originally saved, not the wrapper.
pub fn library_load(data_dir: &Path, kind: &str, id: &str) -> Result<Value, String> {
    if !is_safe_id(id) {
        return Err(format!("invalid id: {}", id));
    }
    let path = library_dir(data_dir, kind).join(format!("{}.json", id));
    if !path.exists() {
        return Err(format!("not found: {}/{}", kind, id));
    }
    let json = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let item: LibraryItem = serde_json::from_str(&json).map_err(|e| e.to_string())?;
    Ok(item.data)
}

pub fn library_delete(data_dir: &Path, kind: &str, id: &str) -> Result<(), String> {
    if !is_safe_id(id) {
        return Err(format!("invalid id: {}", id));
    }
    let path = library_dir(data_dir, kind).join(format!("{}.json", id));
    if path.exists() {
        fs::remove_file(&path).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Defends against path traversal: ids are constructed by `generate_id`
/// and only contain hex digits and a single `-`. Anything else is a sign
/// the id came from an untrusted source.
fn is_safe_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .chars()
            .all(|c| c.is_ascii_hexdigit() || c == '-')
}
