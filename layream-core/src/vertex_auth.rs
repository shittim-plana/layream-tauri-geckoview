use serde::{Deserialize, Serialize};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::error::LayreamError;

const TOKEN_ENDPOINT: &str = "https://oauth2.googleapis.com/token";
const AUTH_ENDPOINT: &str = "https://accounts.google.com/o/oauth2/v2/auth";
const REVOKE_ENDPOINT: &str = "https://oauth2.googleapis.com/revoke";
const SCOPE: &str = "https://www.googleapis.com/auth/cloud-platform https://www.googleapis.com/auth/cloudplatformprojects.readonly";

const REFRESH_MARGIN: Duration = Duration::from_secs(300);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthCredentials {
    pub client_id: String,
    pub client_secret: String,
    pub redirect_uri: String,
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

pub fn build_auth_url(creds: &OAuthCredentials) -> String {
    let params = [
        ("client_id", creds.client_id.as_str()),
        ("redirect_uri", creds.redirect_uri.as_str()),
        ("response_type", "code"),
        ("scope", SCOPE),
        ("access_type", "offline"),
        ("prompt", "consent"),
    ];
    let query: String = params
        .iter()
        .map(|(k, v)| format!("{}={}", k, urlencoded(v)))
        .collect::<Vec<_>>()
        .join("&");
    format!("{}?{}", AUTH_ENDPOINT, query)
}

pub async fn exchange_code(
    client: &reqwest::Client,
    creds: &OAuthCredentials,
    code: &str,
) -> Result<Tokens, LayreamError> {
    let params = [
        ("code", code),
        ("client_id", &creds.client_id),
        ("client_secret", &creds.client_secret),
        ("redirect_uri", &creds.redirect_uri),
        ("grant_type", "authorization_code"),
    ];

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
    let params = [
        ("refresh_token", refresh),
        ("client_id", &creds.client_id),
        ("client_secret", &creds.client_secret),
        ("grant_type", "refresh_token"),
    ];

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

fn urlencoded(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                String::from(b as char)
            }
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
            client_secret: "secret".into(),
            redirect_uri: "http://localhost:8080/callback".into(),
        };
        let url = build_auth_url(&creds);
        assert!(url.starts_with(AUTH_ENDPOINT));
        assert!(url.contains("client_id=test-client"));
        assert!(url.contains("access_type=offline"));
        assert!(url.contains("prompt=consent"));
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
}
