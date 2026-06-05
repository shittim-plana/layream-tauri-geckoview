use serde_json::Value;
use tauri::Manager;

use crate::commands_library::is_safe_filename;

#[tauri::command(rename_all = "snake_case")]
pub async fn open_url(app: tauri::AppHandle, url: String) -> Result<(), String> {
    #[cfg(target_os = "android")]
    {
        let handle = app.state::<crate::browser::BrowserHandle<tauri::Wry>>();
        handle
            .0
            .run_mobile_plugin::<()>("openBrowser", url)
            .map_err(|e| e.to_string())
    }
    #[cfg(not(target_os = "android"))]
    {
        use tauri_plugin_opener::OpenerExt;
        app.opener().open_url(&url, None::<&str>).map_err(|e| e.to_string())
    }
}

#[tauri::command(rename_all = "snake_case")]
pub async fn open_custom_tab(app: tauri::AppHandle, url: String) -> Result<(), String> {
    #[cfg(target_os = "android")]
    {
        let handle = app.state::<crate::browser::BrowserHandle<tauri::Wry>>();
        handle
            .0
            .run_mobile_plugin::<()>("openCustomTab", url)
            .map_err(|e| e.to_string())
    }
    #[cfg(not(target_os = "android"))]
    {
        use tauri_plugin_opener::OpenerExt;
        app.opener().open_url(&url, None::<&str>).map_err(|e| e.to_string())
    }
}

#[tauri::command(rename_all = "snake_case")]
pub async fn request_storage_permission(app: tauri::AppHandle) -> Result<Value, String> {
    #[cfg(target_os = "android")]
    {
        let handle = app.state::<crate::browser::BrowserHandle<tauri::Wry>>();
        handle.0.run_mobile_plugin::<Value>("requestStoragePermission", ()).map_err(|e| e.to_string())
    }
    #[cfg(not(target_os = "android"))]
    {
        let _ = app;
        Ok(serde_json::json!({"granted": true}))
    }
}

#[tauri::command(rename_all = "snake_case")]
pub async fn request_notification_permission(app: tauri::AppHandle) -> Result<Value, String> {
    #[cfg(target_os = "android")]
    {
        let handle = app.state::<crate::browser::BrowserHandle<tauri::Wry>>();
        handle.0.run_mobile_plugin::<Value>("requestNotificationPermission", ()).map_err(|e| e.to_string())
    }
    #[cfg(not(target_os = "android"))]
    {
        let _ = app;
        Ok(serde_json::json!({"granted": true}))
    }
}

#[tauri::command(rename_all = "snake_case")]
pub async fn list_browsers(app: tauri::AppHandle) -> Result<Value, String> {
    #[cfg(target_os = "android")]
    {
        let handle = app.state::<crate::browser::BrowserHandle<tauri::Wry>>();
        handle
            .0
            .run_mobile_plugin::<Value>("listBrowsers", ())
            .map_err(|e| e.to_string())
    }
    #[cfg(not(target_os = "android"))]
    {
        let _ = app;
        Ok(serde_json::json!({"browsers": []}))
    }
}

#[tauri::command(rename_all = "snake_case")]
pub async fn open_in_browser(app: tauri::AppHandle, url: String, package: String) -> Result<(), String> {
    #[cfg(target_os = "android")]
    {
        let handle = app.state::<crate::browser::BrowserHandle<tauri::Wry>>();
        handle
            .0
            .run_mobile_plugin::<()>("openInBrowser", format!("{}|{}", package, url))
            .map_err(|e| e.to_string())
    }
    #[cfg(not(target_os = "android"))]
    {
        use tauri_plugin_opener::OpenerExt;
        app.opener().open_url(&url, None::<&str>).map_err(|e| e.to_string())
    }
}

#[tauri::command(rename_all = "snake_case")]
pub async fn start_streaming(app: tauri::AppHandle, text: Option<String>) -> Result<(), String> {
    #[cfg(target_os = "android")]
    {
        let handle = app.state::<crate::streaming_service::StreamingServiceHandle<tauri::Wry>>();
        let payload = text.unwrap_or_else(|| "AI 응답 수신 중...".to_string());
        handle
            .0
            .run_mobile_plugin::<()>("startStreaming", payload)
            .map_err(|e| e.to_string())
    }
    #[cfg(not(target_os = "android"))]
    {
        let _ = app;
        let _ = text;
        Ok(())
    }
}

#[tauri::command(rename_all = "snake_case")]
pub async fn stop_streaming(app: tauri::AppHandle) -> Result<(), String> {
    #[cfg(target_os = "android")]
    {
        let handle = app.state::<crate::streaming_service::StreamingServiceHandle<tauri::Wry>>();
        handle
            .0
            .run_mobile_plugin::<()>("stopStreaming", ())
            .map_err(|e| e.to_string())
    }
    #[cfg(not(target_os = "android"))]
    {
        let _ = app;
        Ok(())
    }
}

#[tauri::command(rename_all = "snake_case")]
pub async fn update_notification(app: tauri::AppHandle, text: String) -> Result<(), String> {
    #[cfg(target_os = "android")]
    {
        let handle = app.state::<crate::streaming_service::StreamingServiceHandle<tauri::Wry>>();
        handle
            .0
            .run_mobile_plugin::<()>("updateNotification", text)
            .map_err(|e| e.to_string())
    }
    #[cfg(not(target_os = "android"))]
    {
        let _ = app;
        let _ = text;
        Ok(())
    }
}

#[tauri::command(rename_all = "snake_case")]
pub fn save_file_to_downloads(
    filename: String,
    data: Vec<u8>,
    app: tauri::AppHandle,
) -> Result<String, String> {
    if !is_safe_filename(&filename) { return Err(format!("Invalid filename: {}", filename)); }
    use std::path::PathBuf;
    let mut candidates: Vec<PathBuf> = Vec::new();

    #[cfg(target_os = "android")]
    {
        candidates.push(PathBuf::from("/sdcard/Download"));
        candidates.push(PathBuf::from("/storage/emulated/0/Download"));
    }

    #[cfg(not(target_os = "android"))]
    {
        if let Ok(dir) = app.path().download_dir() {
            candidates.push(dir);
        }
    }

    if let Ok(dir) = app.path().app_data_dir() {
        candidates.push(dir.join("exports"));
    }

    let mut last_err = String::from("no candidate directory");
    for dir in candidates {
        if let Err(e) = std::fs::create_dir_all(&dir) {
            last_err = format!("mkdir {}: {}", dir.display(), e);
            continue;
        }
        let path = dir.join(&filename);
        match std::fs::write(&path, &data) {
            Ok(()) => return Ok(path.to_string_lossy().into_owned()),
            Err(e) => last_err = format!("write {}: {}", path.display(), e),
        }
    }
    Err(last_err)
}
