mod commands;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let result = tauri::Builder::default()
        .plugin(
            tauri_plugin_log::Builder::default()
                .level(log::LevelFilter::Debug)
                .build(),
        )
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_shell::init())
        .invoke_handler(tauri::generate_handler![
            commands::load_preset,
            commands::export_preset,
            commands::load_character,
            commands::evaluate_cbs,
            commands::chat_send,
            commands::oauth_start,
            commands::oauth_status,
            commands::mistral_list_models,
            commands::vertex_list_models,
        ])
        .run(tauri::generate_context!());

    if let Err(e) = result {
        log::error!("Tauri app error: {}", e);
        eprintln!("Tauri app error: {}", e);
    }
}
