use tauri::Manager;

mod commands;
mod commands_hypa;
mod persistence;

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
        .plugin(tauri_plugin_dialog::init())
        .manage(commands::AuthState::default())
        .manage(commands::RequestLogState::default())
        .manage(commands_hypa::HypaState::default())
        .setup(|app| {
            let auth_state = app.state::<commands::AuthState>();
            auth_state.load_persisted_tokens(app.handle());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::load_preset,
            commands::export_preset,
            commands::load_character,
            commands::evaluate_cbs,
            commands::chat_vertex,
            commands::chat_gca,
            commands::chat_mistral,
            commands::vertex_oauth_start,
            commands::vertex_oauth_callback,
            commands::vertex_oauth_status,
            commands::gca_oauth_start,
            commands::gca_oauth_callback,
            commands::gca_oauth_status,
            commands::gca_oauth_disconnect,
            commands::vertex_list_projects,
            commands::vertex_oauth_disconnect,
            commands::mistral_list_models,
            commands::vertex_list_models,
            commands::highlight_cbs,
            commands::get_request_logs,
            commands::clear_request_logs,
            commands::cmd_save_settings,
            commands::cmd_load_settings,
            commands::embed_vertex,
            commands::embed_voyage,
            commands::gca_load_code_assist,
            commands::gca_check_opt_out,
            commands::cmd_save_hypa,
            commands::cmd_load_hypa,
            commands::open_url,
            commands::cmd_save_current_preset,
            commands::cmd_load_current_preset,
            commands::cmd_save_session,
            commands::cmd_load_session,
            commands::parse_risum,
            commands::generate_user_message,
            commands_hypa::hypa_summarize,
            commands_hypa::hypa_search,
            commands_hypa::hypa_pin_message,
            commands_hypa::hypa_invalidate_summary,
            commands_hypa::hypa_cleanup,
            commands_hypa::hypa_load_all,
            commands_hypa::hypa_save_all,
        ])
        .run(tauri::generate_context!());

    if let Err(e) = result {
        log::error!("Tauri app error: {}", e);
        eprintln!("Tauri app error: {}", e);
    }
}
