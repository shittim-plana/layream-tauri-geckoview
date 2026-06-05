use tauri::Manager;

mod browser;
mod commands_auth;
mod commands_cbs;
mod commands_chat;
mod commands_hypa;
mod commands_library;
mod commands_platform;
mod commands_settings;
mod commands_workspace;
mod persistence;
mod streaming_service;

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
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_deep_link::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(browser::init())
        .plugin(streaming_service::init())
        .manage(commands_auth::AuthState::default())
        .manage(commands_chat::RequestLogState::default())
        .manage(commands_library::CharacterAssetsState::default())
        .manage(commands_chat::StreamCancelState::default())
        .manage(commands_chat::StreamBufferState::default())
        .manage(commands_hypa::HypaState::default())
        .setup(|app| {
            let auth_state = app.state::<commands_auth::AuthState>();
            auth_state.load_persisted_tokens(app.handle());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands_chat::poll_stream_chunks,
            commands_library::load_preset,
            commands_library::export_preset,
            commands_library::load_character,
            commands_cbs::evaluate_cbs,
            commands_chat::chat_vertex,
            commands_chat::chat_gca,
            commands_chat::chat_mistral,
            commands_auth::vertex_oauth_start,
            commands_auth::vertex_oauth_callback,
            commands_auth::vertex_oauth_status,
            commands_auth::gca_oauth_start,
            commands_auth::gca_oauth_url,
            commands_auth::gca_oauth_callback,
            commands_auth::gca_oauth_status,
            commands_auth::gca_oauth_disconnect,
            commands_auth::vertex_list_projects,
            commands_auth::vertex_oauth_disconnect,
            commands_chat::mistral_list_models,
            commands_chat::vertex_list_models,
            commands_cbs::highlight_cbs,
            commands_chat::get_request_logs,
            commands_chat::clear_request_logs,
            commands_settings::cmd_save_settings,
            commands_settings::cmd_load_settings,
            commands_chat::embed_vertex,
            commands_chat::embed_voyage,
            commands_auth::gca_load_code_assist,
            commands_auth::gca_check_opt_out,
            commands_auth::cmd_gca_load_project,
            commands_platform::open_url,
            commands_platform::open_custom_tab,
            commands_platform::request_storage_permission,
            commands_platform::request_notification_permission,
            commands_auth::get_pending_oauth,
            commands_platform::list_browsers,
            commands_platform::open_in_browser,
            commands_auth::open_geckoview_oauth,
            commands_chat::cancel_chat,
            commands_platform::start_streaming,
            commands_platform::stop_streaming,
            commands_platform::update_notification,
            commands_settings::cmd_save_personas,
            commands_settings::cmd_load_personas,
            commands_settings::cmd_save_current_preset,
            commands_settings::cmd_load_current_preset,
            commands_settings::cmd_save_session,
            commands_settings::cmd_load_session,
            commands_library::parse_risum,
            commands_library::load_preset_from_path,
            commands_library::load_character_from_path,
            commands_library::parse_risum_from_path,
            commands_chat::generate_user_message,
            commands_platform::save_file_to_downloads,
            commands_library::get_asset_data,
            commands_settings::cmd_save_current_character,
            commands_settings::cmd_load_current_character,
            commands_library::library_save_preset,
            commands_library::library_list_presets,
            commands_library::library_load_preset,
            commands_library::library_delete_preset,
            commands_library::library_save_character,
            commands_library::library_list_characters,
            commands_library::library_load_character,
            commands_library::library_delete_character,
            commands_library::library_save_module,
            commands_library::library_list_modules,
            commands_library::library_load_module,
            commands_library::library_delete_module,
            commands_library::cmd_load_modules,
            commands_library::cmd_save_module,
            commands_workspace::cmd_workspace_create,
            commands_workspace::cmd_workspace_list,
            commands_workspace::cmd_workspace_load,
            commands_workspace::cmd_workspace_update,
            commands_workspace::cmd_workspace_delete,
            commands_workspace::cmd_workspace_save_session_ws,
            commands_workspace::cmd_workspace_load_session_ws,
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
    }
}
