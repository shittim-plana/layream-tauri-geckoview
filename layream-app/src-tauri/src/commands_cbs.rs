use layream_core::cbs::highlighter;
use layream_core::cbs::parser::{CbsContext, evaluate};
use serde_json::Value;
use std::collections::HashMap;

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
                "alt": t.alt,
            })
        }).collect::<Vec<_>>(),
        "diagnostics": diagnostics.iter().map(|d| {
            serde_json::json!({ "line": d.line, "message": d.message })
        }).collect::<Vec<_>>(),
    })
}
