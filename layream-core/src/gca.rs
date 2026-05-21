use futures::StreamExt;
use serde_json::Value;

use crate::error::LayreamError;
use crate::retry::{self, CancelToken};
use crate::vertex_api::{GenerateRequest, StreamChunk};

const GCA_BASE: &str = "https://cloudcode-pa.googleapis.com/v1internal";
pub const GCA_OAUTH_CLIENT_ID: &str = "681255809395-oo8ft2oprdrnp9e3aqf6av3hmdib135j.apps.googleusercontent.com";
pub const GCA_OAUTH_CLIENT_SECRET: &str = "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";
pub const GCA_OAUTH_SCOPE: &str = "https://www.googleapis.com/auth/cloud-platform https://www.googleapis.com/auth/userinfo.email https://www.googleapis.com/auth/userinfo.profile";

// Source: risu-gca.js mA array (GCA plugin v0.2.2)
pub const GCA_MODELS: &[&str] = &[
    "gemini-3.1-pro",
    "gemini-3.1-pro-preview",
    "gemini-3.1-flash-lite-preview",
    "gemini-3-pro-preview",
    "gemini-3-flash-preview",
    "gemini-2.5-pro",
    "gemini-2.5-flash",
    "gemini-2.5-flash-lite",
];

pub fn build_stream_endpoint() -> String {
    format!("{}:streamGenerateContent?alt=sse", GCA_BASE)
}

pub async fn stream_generate(
    client: &reqwest::Client,
    access_token: &str,
    model: &str,
    request: &GenerateRequest,
    on_chunk: impl Fn(&str),
    cancel: Option<CancelToken>,
) -> Result<String, LayreamError> {
    let url = build_stream_endpoint();
    let auth = format!("Bearer {}", access_token);
    let wrapped = serde_json::json!({ "model": model, "request": request });

    let resp = retry::retry_request(&cancel, || {
        let req = client
            .post(&url)
            .header("Authorization", &auth)
            .header("x-goog-api-client", "google-cloud-intellij")
            .json(&wrapped);
        async { req.send().await.map_err(|e| LayreamError::Http(e.to_string())) }
    }).await?;

    if !resp.status().is_success() {
        let status = resp.status().as_u16();
        let body = resp.text().await.unwrap_or_else(|e| format!("(body read failed: {e})"));
        return Err(LayreamError::ApiError { status, body });
    }

    let mut full_text = String::new();
    let mut stream = resp.bytes_stream();
    let mut buffer = String::new();

    while let Some(chunk) = stream.next().await {
        if retry::is_cancelled(&cancel) {
            return Err(LayreamError::Http("cancelled".to_string()));
        }
        let bytes = chunk.map_err(|e| LayreamError::Http(e.to_string()))?;
        buffer.push_str(&String::from_utf8_lossy(&bytes));

        while let Some(line_end) = buffer.find('\n') {
            let line = buffer[..line_end].trim().to_string();
            buffer = buffer[line_end + 1..].to_string();

            if let Some(json_str) = line.strip_prefix("data: ") {
                if let Ok(chunk) = serde_json::from_str::<StreamChunk>(json_str) {
                    if let Some(candidates) = &chunk.candidates {
                        for candidate in candidates {
                            if let Some(content) = &candidate.content {
                                for part in &content.parts {
                                    if part.thought != Some(true) {
                                        if let Some(text) = &part.text {
                                            full_text.push_str(text);
                                            on_chunk(text);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(full_text)
}

pub async fn generate_non_streaming(
    client: &reqwest::Client,
    access_token: &str,
    model: &str,
    request: &GenerateRequest,
) -> Result<String, LayreamError> {
    let url = format!("{}:generateContent", GCA_BASE);
    let wrapped = serde_json::json!({ "model": model, "request": request });

    let resp = client
        .post(&url)
        .header("Authorization", format!("Bearer {}", access_token))
        .header("x-goog-api-client", "google-cloud-intellij")
        .json(&wrapped)
        .send()
        .await
        .map_err(|e| LayreamError::Http(e.to_string()))?;

    if !resp.status().is_success() {
        let status = resp.status().as_u16();
        let body = resp.text().await.unwrap_or_else(|e| format!("(body read failed: {e})"));
        return Err(LayreamError::ApiError { status, body });
    }

    let body: Value = resp
        .json()
        .await
        .map_err(|e| LayreamError::Http(e.to_string()))?;

    let mut text = String::new();
    if let Some(candidates) = body.get("candidates").and_then(|c| c.as_array()) {
        for candidate in candidates {
            if let Some(parts) = candidate
                .get("content")
                .and_then(|c| c.get("parts"))
                .and_then(|p| p.as_array())
            {
                for part in parts {
                    if let Some(t) = part.get("text").and_then(|t| t.as_str()) {
                        text.push_str(t);
                    }
                }
            }
        }
    }

    Ok(text)
}

pub async fn check_and_opt_out(
    client: &reqwest::Client,
    access_token: &str,
) -> Result<bool, LayreamError> {
    let url = format!("{}/getCodeAssistGlobalUserSetting", GCA_BASE);
    let resp = client
        .get(&url)
        .header("Authorization", format!("Bearer {}", access_token))
        .send()
        .await
        .map_err(|e| LayreamError::Http(e.to_string()))?;

    if !resp.status().is_success() {
        let status = resp.status().as_u16();
        let body = resp.text().await.unwrap_or_else(|e| format!("(body read failed: {e})"));
        return Err(LayreamError::ApiError { status, body });
    }

    let body: Value = resp.json().await.map_err(|e| LayreamError::Http(e.to_string()))?;

    if body.get("freeTierDataCollectionOptin") == Some(&Value::Bool(true)) {
        let set_url = format!("{}/setCodeAssistGlobalUserSetting", GCA_BASE);
        let opt_resp = client
            .post(&set_url)
            .header("Authorization", format!("Bearer {}", access_token))
            .json(&serde_json::json!({ "freeTierDataCollectionOptin": false }))
            .send()
            .await
            .map_err(|e| LayreamError::Http(e.to_string()))?;
        if !opt_resp.status().is_success() {
            log::warn!("GCA opt-out failed: status {}", opt_resp.status().as_u16());
        }
        return Ok(true);
    }

    Ok(false)
}

pub async fn load_code_assist(
    client: &reqwest::Client,
    access_token: &str,
) -> Result<String, LayreamError> {
    let url = format!("{}/loadCodeAssist", GCA_BASE);
    let body = serde_json::json!({
        "metadata": {
            "ideType": "IDE_UNSPECIFIED",
            "platform": "PLATFORM_UNSPECIFIED",
            "pluginType": "GEMINI"
        }
    });

    let resp = client
        .post(&url)
        .header("Authorization", format!("Bearer {}", access_token))
        .json(&body)
        .send()
        .await
        .map_err(|e| LayreamError::Http(e.to_string()))?;

    if !resp.status().is_success() {
        let status = resp.status().as_u16();
        let body = resp.text().await.unwrap_or_else(|e| format!("(body read failed: {e})"));
        return Err(LayreamError::ApiError { status, body });
    }

    let resp_body: Value = resp.json().await.map_err(|e| LayreamError::Http(e.to_string()))?;
    let project_id = resp_body
        .get("projectId")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    Ok(project_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gca_endpoint_format() {
        let url = build_endpoint("gemini-2.5-flash");
        assert_eq!(
            url,
            "https://cloudcode-pa.googleapis.com/v1internal/models/gemini-2.5-flash:streamGenerateContent?alt=sse"
        );
    }

    #[test]
    fn model_list_not_empty() {
        assert!(!GCA_MODELS.is_empty());
        assert!(GCA_MODELS.contains(&"gemini-2.5-flash"));
        assert!(GCA_MODELS.contains(&"gemini-3-pro-preview"));
        assert!(GCA_MODELS.contains(&"gemini-3.1-pro"));
    }
}
