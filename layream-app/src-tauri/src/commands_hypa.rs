//! HyPA (Hierarchical Prompt Augmentation) Tauri commands.
//!
//! This module houses all HyPA-related Tauri commands. It is intentionally a
//! skeleton at this point: every command body is `todo!()` and the commands
//! are NOT yet registered in `lib.rs::invoke_handler`. Phase 1 worktrees will
//! fill in the bodies and any supporting types in parallel.
//!
//! ## Data schema (RisuAI hypa.json compatible)
//!
//! Phase 1 will introduce the concrete `Summary` struct. For reference, the
//! intended shape is:
//!
//! ```ignore
//! struct Summary {
//!     text: String,
//!     chatMemos: Vec<String>,        // RisuAI compatible (Set of chatIds)
//!     embedding: Option<Vec<f32>>,
//!     isImportant: bool,             // RisuAI compatible
//!     pinBoost: f32,                 // new-arona-bot inspired (default 0)
//!     invalidated: bool,             // new-arona-bot inspired (default false)
//! }
//! ```
//!
//! ## Commands (stubs)
//!
//! - `hypa_summarize`         — generate a summary from N messages
//! - `hypa_search`            — cosine-similarity search over summary embeddings
//! - `hypa_pin_message`       — toggle pin (updates `pinBoost` on covering summaries)
//! - `hypa_invalidate_summary` — mark summaries containing this message as invalidated
//! - `hypa_cleanup`           — delete summaries with empty `chatMemos`, return count
//! - `hypa_load_all`          — load all summaries from `hypa.json`
//! - `hypa_save_all`          — save all summaries to `hypa.json`

use serde_json::Value;

#[tauri::command]
pub async fn hypa_summarize(
    messages: Vec<Value>,
    settings: Value,
    app: tauri::AppHandle,
) -> Result<Value, String> {
    let _ = (messages, settings, app);
    todo!("hypa_summarize not yet implemented")
}

#[tauri::command]
pub async fn hypa_search(
    query_embedding: Vec<f32>,
    top_k: usize,
    app: tauri::AppHandle,
) -> Result<Vec<Value>, String> {
    let _ = (query_embedding, top_k, app);
    todo!("hypa_search not yet implemented")
}

#[tauri::command]
pub async fn hypa_pin_message(
    chat_id: String,
    is_pinned: bool,
    app: tauri::AppHandle,
) -> Result<(), String> {
    let _ = (chat_id, is_pinned, app);
    todo!("hypa_pin_message not yet implemented")
}

#[tauri::command]
pub async fn hypa_invalidate_summary(
    chat_id: String,
    app: tauri::AppHandle,
) -> Result<(), String> {
    let _ = (chat_id, app);
    todo!("hypa_invalidate_summary not yet implemented")
}

#[tauri::command]
pub async fn hypa_cleanup(app: tauri::AppHandle) -> Result<usize, String> {
    let _ = app;
    todo!("hypa_cleanup not yet implemented")
}

#[tauri::command]
pub async fn hypa_load_all(app: tauri::AppHandle) -> Result<Value, String> {
    let _ = app;
    todo!("hypa_load_all not yet implemented")
}

#[tauri::command]
pub async fn hypa_save_all(summaries: Value, app: tauri::AppHandle) -> Result<(), String> {
    let _ = (summaries, app);
    todo!("hypa_save_all not yet implemented")
}
