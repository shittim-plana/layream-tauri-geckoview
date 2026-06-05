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
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, &encrypted).map_err(|e| e.to_string())?;
    fs::rename(&tmp, &path).map_err(|e| e.to_string())?;
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
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, json.as_bytes()).map_err(|e| e.to_string())?;
    fs::rename(&tmp, &path).map_err(|e| e.to_string())?;
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
    let path = data_dir.join(PRESET_FILE);
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, json.as_bytes()).map_err(|e| e.to_string())?;
    fs::rename(&tmp, &path).map_err(|e| e.to_string())?;
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
    let path = data_dir.join(SESSION_FILE);
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, json.as_bytes()).map_err(|e| e.to_string())?;
    fs::rename(&tmp, &path).map_err(|e| e.to_string())?;
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
    let path = data_dir.join(CHARACTER_FILE);
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, json.as_bytes()).map_err(|e| e.to_string())?;
    fs::rename(&tmp, &path).map_err(|e| e.to_string())?;
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

const PERSONAS_FILE: &str = "personas.json";

pub fn save_personas(data_dir: &Path, personas: &Value) -> Result<(), String> {
    fs::create_dir_all(data_dir).map_err(|e| e.to_string())?;
    let json = serde_json::to_string_pretty(personas).map_err(|e| e.to_string())?;
    let path = data_dir.join(PERSONAS_FILE);
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, json.as_bytes()).map_err(|e| e.to_string())?;
    fs::rename(&tmp, &path).map_err(|e| e.to_string())?;
    Ok(())
}

pub fn load_personas(data_dir: &Path) -> Result<Value, String> {
    let path = data_dir.join(PERSONAS_FILE);
    if !path.exists() {
        return Ok(serde_json::json!({ "personas": [], "selectedPersona": -1 }));
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

/// Updates an existing library item's name and/or data in place, preserving
/// its id and created_at. Uses atomic tmp+rename like library_save.
pub fn library_update(
    data_dir: &Path,
    kind: &str,
    id: &str,
    name: &str,
    data: &Value,
) -> Result<(), String> {
    if !is_safe_id(id) {
        return Err(format!("invalid id: {}", id));
    }
    let dir = library_dir(data_dir, kind);
    let final_path = dir.join(format!("{}.json", id));
    if !final_path.exists() {
        return Err(format!("not found: {}/{}", kind, id));
    }

    // Preserve the original created_at timestamp.
    let existing_json = fs::read_to_string(&final_path).map_err(|e| e.to_string())?;
    let existing: LibraryItem = serde_json::from_str(&existing_json).map_err(|e| e.to_string())?;

    let item = LibraryItem {
        id: id.to_string(),
        name: name.to_string(),
        created_at: existing.created_at,
        data: data.clone(),
    };
    let json = serde_json::to_string_pretty(&item).map_err(|e| e.to_string())?;
    let tmp_path = dir.join(format!("{}.json.tmp", id));
    fs::write(&tmp_path, json.as_bytes()).map_err(|e| e.to_string())?;
    fs::rename(&tmp_path, &final_path).map_err(|e| e.to_string())?;
    Ok(())
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

// ──────────────────────────────────────────────────────────────────────────
// Workspaces: each workspace is a directory under `$APP_DATA/workspaces/`
// containing session.json and hypa.json, plus a metadata file
// `$APP_DATA/workspaces/{id}.json` with the workspace header (name,
// character_id, preset_id, module_ids, provider, timestamps).
// ──────────────────────────────────────────────────────────────────────────

pub const WORKSPACE_DIR: &str = "workspaces";

fn workspace_dir(data_dir: &Path) -> PathBuf {
    data_dir.join(WORKSPACE_DIR)
}

/// Creates a new workspace with the given name. Returns the generated id.
/// The metadata file and subdirectory are created atomically (metadata via
/// tmp+rename, subdirectory via create_dir_all).
pub fn workspace_create(data_dir: &Path, name: &str) -> Result<String, String> {
    let dir = workspace_dir(data_dir);
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;

    let id = generate_id();
    let ts = now_secs();
    let meta = serde_json::json!({
        "id": id,
        "name": name,
        "character_id": null,
        "preset_id": null,
        "module_ids": [],
        "provider": null,
        "created_at": ts,
        "updated_at": ts,
    });

    let json = serde_json::to_string_pretty(&meta).map_err(|e| e.to_string())?;
    let final_path = dir.join(format!("{}.json", id));
    let tmp_path = dir.join(format!("{}.json.tmp", id));
    fs::write(&tmp_path, json.as_bytes()).map_err(|e| e.to_string())?;
    fs::rename(&tmp_path, &final_path).map_err(|e| e.to_string())?;

    // Create the workspace subdirectory for session/hypa data
    fs::create_dir_all(dir.join(&id)).map_err(|e| e.to_string())?;

    Ok(id)
}

/// Lists all workspaces as metadata headers, sorted by updated_at descending.
/// Skips files that fail to parse (same resilience as library_list).
pub fn workspace_list(data_dir: &Path) -> Result<Vec<Value>, String> {
    let dir = workspace_dir(data_dir);
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
        // Only read .json files (skip directories and .tmp files)
        if !path.is_file() || path.extension().and_then(|s| s.to_str()) != Some("json") {
            continue;
        }
        let json = match fs::read_to_string(&path) {
            Ok(s) => s,
            Err(_) => continue,
        };
        let meta: Value = match serde_json::from_str(&json) {
            Ok(v) => v,
            Err(_) => continue,
        };
        // Only include items that have an id field (workspace metadata)
        if meta.get("id").is_some() {
            items.push(meta);
        }
    }
    items.sort_by(|a, b| {
        let ta = a.get("updated_at").and_then(|v| v.as_u64()).unwrap_or(0);
        let tb = b.get("updated_at").and_then(|v| v.as_u64()).unwrap_or(0);
        tb.cmp(&ta)
    });
    Ok(items)
}

/// Loads workspace metadata by id.
pub fn workspace_load(data_dir: &Path, id: &str) -> Result<Value, String> {
    if !is_safe_id(id) {
        return Err(format!("invalid workspace id: {}", id));
    }
    let path = workspace_dir(data_dir).join(format!("{}.json", id));
    if !path.exists() {
        return Err(format!("workspace not found: {}", id));
    }
    let json = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    serde_json::from_str(&json).map_err(|e| e.to_string())
}

/// Updates workspace metadata. Merges the provided data into the existing
/// metadata, updates the `updated_at` timestamp, and writes atomically.
pub fn workspace_update(data_dir: &Path, id: &str, data: &Value) -> Result<(), String> {
    if !is_safe_id(id) {
        return Err(format!("invalid workspace id: {}", id));
    }
    let dir = workspace_dir(data_dir);
    let final_path = dir.join(format!("{}.json", id));
    if !final_path.exists() {
        return Err(format!("workspace not found: {}", id));
    }

    // Load existing metadata and merge
    let existing_json = fs::read_to_string(&final_path).map_err(|e| e.to_string())?;
    let mut meta: serde_json::Map<String, Value> =
        serde_json::from_str(&existing_json).map_err(|e| e.to_string())?;

    if let Some(obj) = data.as_object() {
        for (k, v) in obj {
            // Protect immutable fields
            if k == "id" || k == "created_at" {
                continue;
            }
            meta.insert(k.clone(), v.clone());
        }
    }
    meta.insert("updated_at".to_string(), Value::from(now_secs()));

    let json = serde_json::to_string_pretty(&meta).map_err(|e| e.to_string())?;
    let tmp_path = dir.join(format!("{}.json.tmp", id));
    fs::write(&tmp_path, json.as_bytes()).map_err(|e| e.to_string())?;
    fs::rename(&tmp_path, &final_path).map_err(|e| e.to_string())?;
    Ok(())
}

/// Deletes a workspace: removes both the metadata file and the subdirectory.
pub fn workspace_delete(data_dir: &Path, id: &str) -> Result<(), String> {
    if !is_safe_id(id) {
        return Err(format!("invalid workspace id: {}", id));
    }
    let dir = workspace_dir(data_dir);

    // Remove metadata file
    let meta_path = dir.join(format!("{}.json", id));
    if meta_path.exists() {
        fs::remove_file(&meta_path).map_err(|e| e.to_string())?;
    }

    // Remove subdirectory (session.json, hypa.json, etc.)
    let sub_dir = dir.join(id);
    if sub_dir.exists() {
        fs::remove_dir_all(&sub_dir).map_err(|e| e.to_string())?;
    }

    Ok(())
}

/// Saves session data for a workspace. Uses atomic tmp+rename.
pub fn workspace_save_session(data_dir: &Path, id: &str, session: &Value) -> Result<(), String> {
    if !is_safe_id(id) {
        return Err(format!("invalid workspace id: {}", id));
    }
    let sub_dir = workspace_dir(data_dir).join(id);
    fs::create_dir_all(&sub_dir).map_err(|e| e.to_string())?;

    let json = serde_json::to_string_pretty(session).map_err(|e| e.to_string())?;
    let tmp_path = sub_dir.join("session.json.tmp");
    let final_path = sub_dir.join("session.json");
    fs::write(&tmp_path, json.as_bytes()).map_err(|e| e.to_string())?;
    fs::rename(&tmp_path, &final_path).map_err(|e| e.to_string())?;
    Ok(())
}

/// Loads session data for a workspace. Returns `{ "messages": [] }` if none.
pub fn workspace_load_session(data_dir: &Path, id: &str) -> Result<Value, String> {
    if !is_safe_id(id) {
        return Err(format!("invalid workspace id: {}", id));
    }
    let path = workspace_dir(data_dir).join(id).join("session.json");
    if !path.exists() {
        return Ok(serde_json::json!({ "messages": [] }));
    }
    let json = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    serde_json::from_str(&json).map_err(|e| e.to_string())
}

/// Saves HyPA data for a workspace. Uses atomic tmp+rename.
pub fn workspace_save_hypa(data_dir: &Path, id: &str, data: &Value) -> Result<(), String> {
    if !is_safe_id(id) {
        return Err(format!("invalid workspace id: {}", id));
    }
    let sub_dir = workspace_dir(data_dir).join(id);
    fs::create_dir_all(&sub_dir).map_err(|e| e.to_string())?;

    let json = serde_json::to_string_pretty(data).map_err(|e| e.to_string())?;
    let tmp_path = sub_dir.join("hypa.json.tmp");
    let final_path = sub_dir.join("hypa.json");
    fs::write(&tmp_path, json.as_bytes()).map_err(|e| e.to_string())?;
    fs::rename(&tmp_path, &final_path).map_err(|e| e.to_string())?;
    Ok(())
}

/// Loads HyPA data for a workspace. Returns `{ "summaries": [] }` if none.
pub fn workspace_load_hypa(data_dir: &Path, id: &str) -> Result<Value, String> {
    if !is_safe_id(id) {
        return Err(format!("invalid workspace id: {}", id));
    }
    let path = workspace_dir(data_dir).join(id).join("hypa.json");
    if !path.exists() {
        return Ok(serde_json::json!({ "summaries": [] }));
    }
    let json = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    serde_json::from_str(&json).map_err(|e| e.to_string())
}

// ──────────────────────────────────────────────────────────────────────────
// Benchmark: session.json save cost (serialize + atomic tmp+rename).
//
// What is measured: the body of `save_session` — the same code path
// `workspace_save_session` runs — i.e. `serde_json::to_string_pretty` over
// `{ "messages": [...] }` followed by `fs::write(tmp)` + `fs::rename`.
//
// Message shape mirrors the frontend (messageStore.js / ChatView.svelte):
//   { chatId, parentId, branchId, role, text, time, [pinned], [alternatives] }
// Sizes are documented constants below, not magic values (§1.4). The produced
// byte count is reported alongside each timing so the payload is observable.
//
// Run: cargo test --release --lib bench_session -- --nocapture --test-threads=1
// Release is the production build mode; debug serde_json is several× slower
// and would not reflect what users experience.
// ──────────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod bench_session {
    use super::*;
    use std::time::Instant;

    // Representative content sizes (chars). User turns are short prompts;
    // assistant ("char") turns are longer prose. Every 4th assistant turn
    // carries two regeneration alternatives, matching the swipe feature.
    const USER_TEXT_LEN: usize = 280;
    const CHAR_TEXT_LEN: usize = 1200;
    const ALT_TEXT_LEN: usize = 1200;
    const ALTS_EVERY: usize = 4;

    const WARMUP: usize = 5;
    const ITERS: usize = 50;

    fn text_of(len: usize, seed: usize) -> String {
        // Deterministic ASCII filler with spaces so it resembles prose and
        // exercises serde_json string escaping the way real text would.
        let words = [
            "the", "model", "responds", "with", "context", "about", "the",
            "current", "scene", "and", "asks", "a", "question", "before",
            "continuing", "the", "narrative", "naturally", "again", "now",
        ];
        let mut s = String::with_capacity(len + 16);
        let mut i = seed;
        while s.len() < len {
            s.push_str(words[i % words.len()]);
            s.push(' ');
            i += 1;
        }
        s.truncate(len);
        s
    }

    fn id_of(idx: usize) -> String {
        format!("{:08x}-{:08x}-msg", idx as u64, (idx.wrapping_mul(2654435761)) as u32)
    }

    fn build_session(num_messages: usize) -> Value {
        let mut messages = Vec::with_capacity(num_messages);
        let mut prev_id: Option<String> = None;
        for idx in 0..num_messages {
            let is_user = idx % 2 == 0;
            let role = if is_user { "user" } else { "char" };
            let text = if is_user {
                text_of(USER_TEXT_LEN, idx)
            } else {
                text_of(CHAR_TEXT_LEN, idx)
            };
            let mut msg = serde_json::json!({
                "chatId": id_of(idx),
                "parentId": prev_id,
                "branchId": "main",
                "role": role,
                "text": text,
                "time": "2:45:30 PM",
            });
            if !is_user {
                msg["pinned"] = Value::Bool(idx % 7 == 0);
                if (idx / 2) % ALTS_EVERY == 0 {
                    msg["alternatives"] = serde_json::json!([
                        text_of(ALT_TEXT_LEN, idx + 1),
                        text_of(ALT_TEXT_LEN, idx + 2),
                    ]);
                }
            }
            messages.push(msg);
            prev_id = Some(id_of(idx));
        }
        serde_json::json!({
            "messages": messages,
            "activeBranchId": "main",
            "branches": [ { "id": "main", "name": "main", "headId": prev_id, "forkPoint": null } ],
        })
    }

    fn percentile(sorted_micros: &[u128], p: f64) -> u128 {
        if sorted_micros.is_empty() {
            return 0;
        }
        let rank = (p / 100.0 * (sorted_micros.len() as f64 - 1.0)).round() as usize;
        sorted_micros[rank]
    }

    #[test]
    #[ignore]
    fn bench_session_save() {
        let dir = std::env::temp_dir().join(format!("layream_bench_{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();

        println!("\n=== session.json save benchmark (release) ===");
        println!("iters/size: {} (after {} warmup)\n", ITERS, WARMUP);
        println!(
            "{:>8} | {:>10} | {:>8} | {:>8} | {:>8} | {:>8} | {:>10}",
            "messages", "bytes", "min(ms)", "med(ms)", "p90(ms)", "max(ms)", "serialize"
        );
        println!("{}", "-".repeat(78));

        for &n in &[100usize, 500, 1000, 5000] {
            let session = build_session(n);

            // Observable payload size: serialize once up front.
            let sample = serde_json::to_string_pretty(&session).unwrap();
            let bytes = sample.len();

            // Warmup (fills page cache, stabilizes fs state).
            for _ in 0..WARMUP {
                save_session(&dir, &session).unwrap();
            }

            // Measure full save_session (serialize + write tmp + rename).
            let mut totals: Vec<u128> = Vec::with_capacity(ITERS);
            // Separately measure serialize-only to attribute the split.
            let mut sers: Vec<u128> = Vec::with_capacity(ITERS);
            for _ in 0..ITERS {
                let t0 = Instant::now();
                let _ = serde_json::to_string_pretty(&session).unwrap();
                sers.push(t0.elapsed().as_micros());

                let t1 = Instant::now();
                save_session(&dir, &session).unwrap();
                totals.push(t1.elapsed().as_micros());
            }
            totals.sort_unstable();
            sers.sort_unstable();

            let med_ser = percentile(&sers, 50.0) as f64 / 1000.0;
            println!(
                "{:>8} | {:>10} | {:>8.3} | {:>8.3} | {:>8.3} | {:>8.3} | {:>8.3}ms",
                n,
                bytes,
                percentile(&totals, 0.0) as f64 / 1000.0,
                percentile(&totals, 50.0) as f64 / 1000.0,
                percentile(&totals, 90.0) as f64 / 1000.0,
                percentile(&totals, 100.0) as f64 / 1000.0,
                med_ser,
            );
        }
        println!();

        // Cleanup (§5.2: release the bench artifacts).
        let _ = fs::remove_dir_all(&dir);
    }
}
