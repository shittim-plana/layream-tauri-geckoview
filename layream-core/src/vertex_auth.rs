use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Sha256, Digest};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::error::LayreamError;

const TOKEN_ENDPOINT: &str = "https://oauth2.googleapis.com/token";
const AUTH_ENDPOINT: &str = "https://accounts.google.com/o/oauth2/v2/auth";
const REVOKE_ENDPOINT: &str = "https://oauth2.googleapis.com/revoke";
const SCOPE: &str = "https://www.googleapis.com/auth/cloud-platform https://www.googleapis.com/auth/cloudplatformprojects.readonly";

const REFRESH_MARGIN: Duration = Duration::from_secs(300);

pub const VERTEX_CLIENT_ID: &str = "317210024447-v4g6e0e1q5933vogajp0651vhkrgal06.apps.googleusercontent.com";
pub const LAYREAM_REDIRECT_URI: &str = "com.shittimplana.layream://oauth/callback";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthCredentials {
    pub client_id: String,
    pub client_secret: Option<String>,
    pub redirect_uri: String,
}

#[derive(Debug, Clone)]
pub struct PkceChallenge {
    pub verifier: String,
    pub challenge: String,
}

pub fn generate_pkce() -> PkceChallenge {
    use rand::Rng;
    let verifier: String = rand::rng()
        .sample_iter(&rand::distr::Alphanumeric)
        .take(64)
        .map(char::from)
        .collect();
    let hash = Sha256::digest(verifier.as_bytes());
    let challenge = base64url_encode(&hash);
    PkceChallenge { verifier, challenge }
}

fn base64url_encode(data: &[u8]) -> String {
    use base64::Engine;
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(data)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tokens {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_at: u64,
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: String,
    refresh_token: Option<String>,
    expires_in: u64,
}

impl Tokens {
    pub fn is_expired(&self) -> bool {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        now >= self.expires_at
    }

    pub fn needs_refresh(&self) -> bool {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        now + REFRESH_MARGIN.as_secs() >= self.expires_at
    }
}

pub fn build_auth_url(creds: &OAuthCredentials, pkce: Option<&PkceChallenge>) -> String {
    let mut params = vec![
        ("client_id", creds.client_id.as_str()),
        ("redirect_uri", creds.redirect_uri.as_str()),
        ("response_type", "code"),
        ("scope", SCOPE),
        ("access_type", "offline"),
        ("prompt", "select_account consent"),
    ];
    let challenge_str;
    if let Some(p) = pkce {
        challenge_str = p.challenge.clone();
        params.push(("code_challenge", &challenge_str));
        params.push(("code_challenge_method", "S256"));
    }
    let query: String = params
        .iter()
        .map(|(k, v)| format!("{}={}", k, uri_encode(v)))
        .collect::<Vec<_>>()
        .join("&");
    format!("{}?{}", AUTH_ENDPOINT, query)
}

pub async fn exchange_code(
    client: &reqwest::Client,
    creds: &OAuthCredentials,
    code: &str,
    code_verifier: Option<&str>,
) -> Result<Tokens, LayreamError> {
    let mut params = vec![
        ("code".to_string(), code.to_string()),
        ("client_id".to_string(), creds.client_id.clone()),
        ("redirect_uri".to_string(), creds.redirect_uri.clone()),
        ("grant_type".to_string(), "authorization_code".to_string()),
    ];
    if let Some(secret) = &creds.client_secret {
        params.push(("client_secret".to_string(), secret.clone()));
    }
    if let Some(verifier) = code_verifier {
        params.push(("code_verifier".to_string(), verifier.to_string()));
    }

    let resp = client
        .post(TOKEN_ENDPOINT)
        .form(&params)
        .send()
        .await
        .map_err(|e| LayreamError::Http(e.to_string()))?;

    if !resp.status().is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(LayreamError::OAuthError(body));
    }

    let token_resp: TokenResponse = resp
        .json()
        .await
        .map_err(|e| LayreamError::Http(e.to_string()))?;

    Ok(to_tokens(token_resp))
}

pub async fn refresh_token(
    client: &reqwest::Client,
    creds: &OAuthCredentials,
    refresh: &str,
) -> Result<Tokens, LayreamError> {
    let mut params = vec![
        ("refresh_token".to_string(), refresh.to_string()),
        ("client_id".to_string(), creds.client_id.clone()),
        ("grant_type".to_string(), "refresh_token".to_string()),
    ];
    if let Some(secret) = &creds.client_secret {
        params.push(("client_secret".to_string(), secret.clone()));
    }

    let resp = client
        .post(TOKEN_ENDPOINT)
        .form(&params)
        .send()
        .await
        .map_err(|e| LayreamError::Http(e.to_string()))?;

    if !resp.status().is_success() {
        let body = resp.text().await.unwrap_or_default();
        if body.contains("invalid_grant") {
            return Err(LayreamError::InvalidGrant);
        }
        return Err(LayreamError::OAuthError(body));
    }

    let mut token_resp: TokenResponse = resp
        .json()
        .await
        .map_err(|e| LayreamError::Http(e.to_string()))?;

    if token_resp.refresh_token.is_none() {
        token_resp.refresh_token = Some(refresh.to_string());
    }

    Ok(to_tokens(token_resp))
}

pub async fn get_valid_token(
    client: &reqwest::Client,
    creds: &OAuthCredentials,
    tokens: &Tokens,
) -> Result<Tokens, LayreamError> {
    if !tokens.needs_refresh() {
        return Ok(tokens.clone());
    }
    let refresh = tokens
        .refresh_token
        .as_deref()
        .ok_or(LayreamError::OAuthError("no refresh token".into()))?;
    refresh_token(client, creds, refresh).await
}

pub async fn revoke_token(
    client: &reqwest::Client,
    token: &str,
) -> Result<(), LayreamError> {
    let _ = client
        .post(REVOKE_ENDPOINT)
        .form(&[("token", token)])
        .send()
        .await
        .map_err(|e| LayreamError::Http(e.to_string()))?;
    Ok(())
}

fn to_tokens(resp: TokenResponse) -> Tokens {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    Tokens {
        access_token: resp.access_token,
        refresh_token: resp.refresh_token,
        expires_at: now + resp.expires_in,
    }
}

const GCP_PROJECTS_ENDPOINT: &str = "https://cloudresourcemanager.googleapis.com/v1/projects";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GcpProject {
    #[serde(rename = "projectId")]
    pub project_id: String,
    pub name: String,
}

pub async fn list_gcp_projects(
    client: &reqwest::Client,
    access_token: &str,
) -> Result<Vec<GcpProject>, LayreamError> {
    let url = format!("{}?filter=lifecycleState:ACTIVE", GCP_PROJECTS_ENDPOINT);
    let resp = client
        .get(&url)
        .header("Authorization", format!("Bearer {}", access_token))
        .send()
        .await
        .map_err(|e| LayreamError::Http(e.to_string()))?;

    if !resp.status().is_success() {
        let status = resp.status().as_u16();
        let body = resp.text().await.unwrap_or_default();
        return Err(LayreamError::ApiError { status, body });
    }

    let body: Value = resp
        .json()
        .await
        .map_err(|e| LayreamError::Http(e.to_string()))?;

    let projects = body
        .get("projects")
        .and_then(|p| p.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|p| {
                    Some(GcpProject {
                        project_id: p.get("projectId")?.as_str()?.to_string(),
                        name: p.get("name")?.as_str()?.to_string(),
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    Ok(projects)
}

fn uri_encode(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                String::from(b as char)
            }
            _ => format!("%{:02X}", b),
        })
        .collect()
}

pub fn urlencoded(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                String::from(b as char)
            }
            b' ' => String::from('+'),
            _ => format!("%{:02X}", b),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auth_url_format() {
        let creds = OAuthCredentials {
            client_id: "test-client".into(),
            client_secret: None,
            redirect_uri: "http://localhost:8080/callback".into(),
        };
        let url = build_auth_url(&creds, None);
        assert!(url.starts_with(AUTH_ENDPOINT));
        assert!(url.contains("client_id=test-client"));
        assert!(url.contains("access_type=offline"));
        assert!(url.contains("prompt=select_account"));
    }

    #[test]
    fn auth_url_with_pkce() {
        let creds = OAuthCredentials {
            client_id: "test-client".into(),
            client_secret: None,
            redirect_uri: "com.test://callback".into(),
        };
        let pkce = generate_pkce();
        let url = build_auth_url(&creds, Some(&pkce));
        assert!(url.contains("code_challenge="));
        assert!(url.contains("code_challenge_method=S256"));
        assert_eq!(pkce.verifier.len(), 64);
    }

    #[test]
    fn token_expiration() {
        let tokens = Tokens {
            access_token: "test".into(),
            refresh_token: None,
            expires_at: 0,
        };
        assert!(tokens.is_expired());
        assert!(tokens.needs_refresh());

        let far_future = Tokens {
            access_token: "test".into(),
            refresh_token: None,
            expires_at: u64::MAX,
        };
        assert!(!far_future.is_expired());
        assert!(!far_future.needs_refresh());
    }

    #[test]
    fn urlencoded_space_as_plus() {
        assert_eq!(urlencoded("select_account consent"), "select_account+consent");
        assert_eq!(urlencoded("a b c"), "a+b+c");
        assert_eq!(urlencoded("no_spaces"), "no_spaces");
    }

    #[test]
    fn auth_url_prompt_encoding() {
        let creds = OAuthCredentials {
            client_id: "test".into(),
            client_secret: None,
            redirect_uri: "http://localhost/cb".into(),
        };
        let url = build_auth_url(&creds, None);
        assert!(url.contains("prompt=select_account%20consent"));
    }

    #[test]
    fn uri_encode_uses_percent() {
        assert_eq!(uri_encode("select_account consent"), "select_account%20consent");
        assert_eq!(uri_encode("a b"), "a%20b");
    }
}
