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
        .plugin(tauri_plugin_deep_link::init())
        .manage(commands::AuthState::default())
        .invoke_handler(tauri::generate_handler![
            commands::load_preset,
            commands::export_preset,
            commands::load_character,
            commands::evaluate_cbs,
            commands::chat_send,
            commands::vertex_oauth_start,
            commands::vertex_oauth_callback,
            commands::vertex_oauth_status,
            commands::gca_oauth_start,
            commands::gca_oauth_callback,
            commands::gca_oauth_status,
            commands::mistral_list_models,
            commands::vertex_list_models,
        ])
        .run(tauri::generate_context!());

    if let Err(e) = result {
        log::error!("Tauri app error: {}", e);
        eprintln!("Tauri app error: {}", e);
    }
}
