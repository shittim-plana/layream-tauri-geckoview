use layream_core::gca::{self, GCA_OAUTH_CLIENT_ID, GCA_OAUTH_CLIENT_SECRET, GCA_OAUTH_SCOPE};
use layream_core::vertex_auth::{
    self, OAuthCredentials, PkceChallenge, Tokens, VERTEX_CLIENT_ID, LAYREAM_REDIRECT_URI,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Mutex;
use tauri::{Emitter, Manager, State};

use crate::persistence;

const GCA_REDIRECT_URI: &str = "com.googleusercontent.apps.681255809395-oo8ft2oprdrnp9e3aqf6av3hmdib135j:/oauth2callback";

/// OAuth status returned by vertex_oauth_status / gca_oauth_status.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthStatus {
    pub connected: bool,
    #[serde(default)]
    pub expired: bool,
}

pub struct AuthState {
    pub vertex_tokens: Mutex<Option<Tokens>>,
    pub gca_tokens: Mutex<Option<Tokens>>,
    pub vertex_pkce: Mutex<Option<PkceChallenge>>,
}

impl Default for AuthState {
    fn default() -> Self {
        Self {
            vertex_tokens: Mutex::new(None),
            gca_tokens: Mutex::new(None),
            vertex_pkce: Mutex::new(None),
        }
    }
}

impl AuthState {
    pub fn persist_tokens(&self, app: &tauri::AppHandle) {
        match persistence::get_data_dir(app) {
            Ok(data_dir) => {
                match (self.vertex_tokens.lock(), self.gca_tokens.lock()) {
                    (Ok(vertex), Ok(gca)) => {
                        if let Err(e) = persistence::save_tokens(&data_dir, &vertex.clone(), &gca.clone()) {
                            log::warn!("Failed to save tokens: {e}");
                        }
                    }
                    _ => log::warn!("Failed to lock token state for persistence"),
                }
            }
            Err(e) => log::warn!("Failed to get data dir for token persistence: {e}"),
        }
    }

    pub fn load_persisted_tokens(&self, app: &tauri::AppHandle) {
        if let Ok(data_dir) = persistence::get_data_dir(app) {
            match persistence::load_tokens(&data_dir) {
                Ok((vertex, gca)) => {
                    if let Ok(mut guard) = self.vertex_tokens.lock() {
                        *guard = vertex;
                    } else {
                        log::warn!("Failed to lock vertex_tokens for loading");
                    }
                    if let Ok(mut guard) = self.gca_tokens.lock() {
                        *guard = gca;
                    } else {
                        log::warn!("Failed to lock gca_tokens for loading");
                    }
                }
                Err(e) => log::debug!("No saved tokens found: {e}"),
            }
        }
    }
}

fn vertex_creds() -> OAuthCredentials {
    OAuthCredentials {
        client_id: VERTEX_CLIENT_ID.to_string(),
        client_secret: None,
        redirect_uri: LAYREAM_REDIRECT_URI.to_string(),
    }
}

fn gca_creds() -> OAuthCredentials {
    OAuthCredentials {
        client_id: GCA_OAUTH_CLIENT_ID.to_string(),
        client_secret: Some(GCA_OAUTH_CLIENT_SECRET.to_string()),
        redirect_uri: GCA_REDIRECT_URI.to_string(),
    }
}

pub(crate) async fn ensure_vertex_token(
    state: &AuthState,
    app: &tauri::AppHandle,
) -> Result<(reqwest::Client, String), String> {
    let tokens = {
        let guard = state.vertex_tokens.lock().map_err(|e| format!("lock poisoned: {e}"))?;
        guard.clone().ok_or("Vertex AI not connected")?
    };
    let client = reqwest::Client::new();
    let valid_tokens = vertex_auth::get_valid_token(&client, &vertex_creds(), &tokens)
        .await.map_err(|e| e.to_string())?;
    if valid_tokens.access_token != tokens.access_token {
        *state.vertex_tokens.lock().map_err(|e| format!("lock poisoned: {e}"))? = Some(valid_tokens.clone());
        state.persist_tokens(app);
    }
    Ok((client, valid_tokens.access_token))
}

pub(crate) async fn ensure_gca_token(
    state: &AuthState,
    app: &tauri::AppHandle,
) -> Result<(reqwest::Client, String), String> {
    let tokens = {
        let guard = state.gca_tokens.lock().map_err(|e| format!("lock poisoned: {e}"))?;
        guard.clone().ok_or("GCA not connected")?
    };
    let client = reqwest::Client::new();
    let valid_tokens = vertex_auth::get_valid_token(&client, &gca_creds(), &tokens)
        .await.map_err(|e| e.to_string())?;
    if valid_tokens.access_token != tokens.access_token {
        *state.gca_tokens.lock().map_err(|e| format!("lock poisoned: {e}"))? = Some(valid_tokens.clone());
        state.persist_tokens(app);
    }
    Ok((client, valid_tokens.access_token))
}

/// Read gcaProject from persisted settings. Returns None on any failure
/// so callers always have a graceful fallback.
pub(crate) fn load_gca_project(app: &tauri::AppHandle) -> Option<String> {
    let data_dir = persistence::get_data_dir(app).ok()?;
    let settings = persistence::load_settings(&data_dir).ok()?;
    settings.get("gcaProject").and_then(|v| v.as_str()).map(|s| s.to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub async fn vertex_oauth_start(
    state: State<'_, AuthState>,
    app: tauri::AppHandle,
) -> Result<String, String> {
    let pkce = vertex_auth::generate_pkce();
    let creds = vertex_creds();
    let auth_url = vertex_auth::build_auth_url(&creds, Some(&pkce));
    *state.vertex_pkce.lock().map_err(|e| format!("lock poisoned: {e}"))? = Some(pkce.clone());
    if let Ok(data_dir) = persistence::get_data_dir(&app) {
        if let Err(e) = std::fs::write(data_dir.join("pkce_verifier.txt"), &pkce.verifier) {
            log::warn!("Failed to persist PKCE verifier: {e}");
        }
    }
    Ok(auth_url)
}

#[tauri::command(rename_all = "snake_case")]
pub async fn vertex_oauth_callback(
    code: String,
    state: State<'_, AuthState>,
    app: tauri::AppHandle,
) -> Result<String, String> {
    let verifier = state.vertex_pkce.lock().map_err(|e| format!("lock poisoned: {e}"))?.take()
        .map(|p| p.verifier)
        .or_else(|| {
            persistence::get_data_dir(&app).ok()
                .and_then(|d| std::fs::read_to_string(d.join("pkce_verifier.txt")).ok())
        })
        .ok_or("No PKCE verifier found")?;
    if let Ok(data_dir) = persistence::get_data_dir(&app) {
        if let Err(e) = std::fs::remove_file(data_dir.join("pkce_verifier.txt")) {
            log::warn!("Failed to clean PKCE verifier: {e}");
        }
    }
    let creds = vertex_creds();
    let client = reqwest::Client::new();
    let tokens = vertex_auth::exchange_code(&client, &creds, &code, Some(&verifier))
        .await
        .map_err(|e| e.to_string())?;
    *state.vertex_tokens.lock().map_err(|e| format!("lock poisoned: {e}"))? = Some(tokens);
    state.persist_tokens(&app);
    Ok("Vertex AI connected".into())
}

#[tauri::command(rename_all = "snake_case")]
pub async fn gca_oauth_start(
    state: State<'_, AuthState>,
    app: tauri::AppHandle,
) -> Result<String, String> {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .map_err(|e| format!("Failed to bind loopback server: {}", e))?;
    let port = listener.local_addr().map_err(|e| e.to_string())?.port();
    let redirect_uri = format!("http://localhost:{}/oauth2callback", port);

    let auth_url = format!(
        "https://accounts.google.com/o/oauth2/v2/auth?client_id={}&redirect_uri={}&response_type=code&scope={}&access_type=offline&prompt=select_account+consent",
        vertex_auth::uri_encode(GCA_OAUTH_CLIENT_ID),
        vertex_auth::uri_encode(&redirect_uri),
        vertex_auth::uri_encode(GCA_OAUTH_SCOPE),
    );

    let app_clone = app.clone();
    let redirect_for_exchange = redirect_uri.clone();
    let client = reqwest::Client::new();
    tokio::spawn(async move {
        let accept_result = tokio::time::timeout(
            std::time::Duration::from_secs(300),
            listener.accept(),
        ).await;
        let (stream, _) = match accept_result {
            Ok(Ok(v)) => v,
            _ => return,
        };
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let mut stream = stream;
        let mut buf = vec![0u8; 4096];
        let n = match stream.read(&mut buf).await {
            Ok(n) => n,
            _ => return,
        };
        let request = String::from_utf8_lossy(&buf[..n]);
        let code = match extract_code_from_request(&request) {
            Some(c) => c,
            None => return,
        };
        let creds = OAuthCredentials {
            client_id: GCA_OAUTH_CLIENT_ID.to_string(),
            client_secret: Some(GCA_OAUTH_CLIENT_SECRET.to_string()),
            redirect_uri: redirect_for_exchange,
        };
        match vertex_auth::exchange_code(&client, &creds, &code, None).await {
            Ok(tokens) => {
                let auth: tauri::State<'_, AuthState> = app_clone.state();
                if let Ok(mut guard) = auth.gca_tokens.lock() {
                    *guard = Some(tokens);
                }
                auth.persist_tokens(&app_clone);
                let _ = app_clone.emit("gca-auth-complete", "ok");
                let _ = stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: text/html\r\n\r\n<html><body><h2>GCA Connected!</h2><p>You can close this tab.</p></body></html>").await;
            }
            Err(e) => {
                log::error!("GCA token exchange failed: {:?}", e);
                let _ = app_clone.emit("gca-auth-complete", format!("error: {}", e));
                let body = format!("<html><body><h2>Error</h2><p>{}</p></body></html>", e);
                let _ = stream.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: text/html\r\n\r\n{}", body).as_bytes()).await;
            }
        }
    });

    Ok(auth_url)
}

/// Returns the GCA OAuth authorization URL using the deep link redirect URI
/// (for GeckoView in-app OAuth, without starting a loopback TCP server).
#[tauri::command(rename_all = "snake_case")]
pub fn gca_oauth_url() -> Result<String, String> {
    let auth_url = format!(
        "https://accounts.google.com/o/oauth2/v2/auth?client_id={}&redirect_uri={}&response_type=code&scope={}&access_type=offline&prompt=select_account+consent",
        vertex_auth::uri_encode(GCA_OAUTH_CLIENT_ID),
        vertex_auth::uri_encode(GCA_REDIRECT_URI),
        vertex_auth::uri_encode(GCA_OAUTH_SCOPE),
    );
    Ok(auth_url)
}

fn extract_code_from_request(request: &str) -> Option<String> {
    let first_line = request.lines().next()?;
    let path = first_line.split_whitespace().nth(1)?;
    let query = path.split('?').nth(1)?;
    for param in query.split('&') {
        let mut kv = param.splitn(2, '=');
        if kv.next()? == "code" {
            return Some(kv.next()?.to_string());
        }
    }
    None
}

#[tauri::command(rename_all = "snake_case")]
pub async fn gca_oauth_callback(
    code: String,
    state: State<'_, AuthState>,
    app: tauri::AppHandle,
) -> Result<String, String> {
    let creds = gca_creds();
    let client = reqwest::Client::new();
    let tokens = vertex_auth::exchange_code(&client, &creds, &code, None)
        .await
        .map_err(|e| e.to_string())?;
    *state.gca_tokens.lock().map_err(|e| format!("lock poisoned: {e}"))? = Some(tokens);
    state.persist_tokens(&app);
    Ok("GCA connected".into())
}

#[tauri::command(rename_all = "snake_case")]
pub fn vertex_oauth_status(state: State<'_, AuthState>) -> Result<OAuthStatus, String> {
    let tokens = state.vertex_tokens.lock().map_err(|e| format!("lock poisoned: {e}"))?;
    Ok(match tokens.as_ref() {
        Some(t) if !t.is_expired() => OAuthStatus { connected: true, expired: false },
        Some(_) => OAuthStatus { connected: true, expired: true },
        None => OAuthStatus { connected: false, expired: false },
    })
}

#[tauri::command(rename_all = "snake_case")]
pub fn gca_oauth_status(state: State<'_, AuthState>) -> Result<OAuthStatus, String> {
    let tokens = state.gca_tokens.lock().map_err(|e| format!("lock poisoned: {e}"))?;
    Ok(match tokens.as_ref() {
        Some(t) if !t.is_expired() => OAuthStatus { connected: true, expired: false },
        Some(_) => OAuthStatus { connected: true, expired: true },
        None => OAuthStatus { connected: false, expired: false },
    })
}

#[tauri::command(rename_all = "snake_case")]
pub async fn vertex_list_projects(
    state: State<'_, AuthState>,
    app: tauri::AppHandle,
) -> Result<Value, String> {
    let (client, access_token) = ensure_vertex_token(&state, &app).await?;
    let projects = layream_core::vertex_auth::list_gcp_projects(&client, &access_token)
        .await
        .map_err(|e| e.to_string())?;
    serde_json::to_value(&projects).map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub fn vertex_oauth_disconnect(state: State<'_, AuthState>, app: tauri::AppHandle) -> Result<String, String> {
    *state.vertex_tokens.lock().map_err(|e| format!("lock poisoned: {e}"))? = None;
    state.persist_tokens(&app);
    Ok("Disconnected".into())
}

#[tauri::command(rename_all = "snake_case")]
pub fn gca_oauth_disconnect(state: State<'_, AuthState>, app: tauri::AppHandle) -> Result<String, String> {
    *state.gca_tokens.lock().map_err(|e| format!("lock poisoned: {e}"))? = None;
    state.persist_tokens(&app);
    Ok("Disconnected".into())
}

#[tauri::command(rename_all = "snake_case")]
pub async fn gca_load_code_assist(
    state: State<'_, AuthState>,
    app: tauri::AppHandle,
) -> Result<String, String> {
    let (client, access_token) = ensure_gca_token(&state, &app).await?;
    gca::load_code_assist(&client, &access_token)
        .await.map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub async fn gca_check_opt_out(
    state: State<'_, AuthState>,
    app: tauri::AppHandle,
) -> Result<bool, String> {
    let (client, access_token) = ensure_gca_token(&state, &app).await?;
    gca::check_and_opt_out(&client, &access_token)
        .await.map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub async fn cmd_gca_load_project(
    state: State<'_, AuthState>,
    app: tauri::AppHandle,
) -> Result<String, String> {
    let (client, access_token) = ensure_gca_token(&state, &app).await?;

    // opt-out check (fire-and-forget warning on failure)
    if let Err(e) = gca::check_and_opt_out(&client, &access_token).await {
        log::warn!("GCA opt-out check failed: {e}");
    }

    // load project id
    let project_id = gca::load_code_assist(&client, &access_token)
        .await
        .map_err(|e| e.to_string())?;

    // persist to settings
    let data_dir = persistence::get_data_dir(&app)?;
    let mut settings = persistence::load_settings(&data_dir)
        .unwrap_or_else(|_| serde_json::json!({}));
    if let Some(obj) = settings.as_object_mut() {
        obj.insert("gcaProject".to_string(), Value::String(project_id.clone()));
    }
    persistence::save_settings(&data_dir, &settings)?;

    Ok(project_id)
}

#[tauri::command(rename_all = "snake_case")]
pub async fn get_pending_oauth(app: tauri::AppHandle) -> Result<Value, String> {
    #[cfg(target_os = "android")]
    {
        let handle = app.state::<crate::browser::BrowserHandle<tauri::Wry>>();
        handle.0.run_mobile_plugin::<Value>("getPendingOAuth", ()).map_err(|e| e.to_string())
    }
    #[cfg(not(target_os = "android"))]
    {
        let _ = app;
        Ok(serde_json::json!({}))
    }
}

#[tauri::command(rename_all = "snake_case")]
pub async fn open_geckoview_oauth(app: tauri::AppHandle, url: String, redirect_uri_prefix: String) -> Result<Value, String> {
    #[cfg(target_os = "android")]
    {
        let handle = app.state::<crate::browser::BrowserHandle<tauri::Wry>>();
        handle
            .0
            .run_mobile_plugin::<Value>("openGeckoViewOAuth", format!("{}|{}", url, redirect_uri_prefix))
            .map_err(|e| e.to_string())
    }
    #[cfg(not(target_os = "android"))]
    {
        let _ = (app, url, redirect_uri_prefix);
        Err("GeckoView OAuth is only available on Android".into())
    }
}
