use futures::StreamExt;
use serde_json::Value;

use crate::error::LayreamError;
use crate::vertex_api::{GenerateRequest, StreamChunk};

const GCA_BASE: &str = "https://cloudcode-pa.googleapis.com/v1internal";

pub const GCA_MODELS: &[&str] = &[
    "gemini-2.5-flash",
    "gemini-2.5-flash-lite",
    "gemini-2.5-pro",
    "gemini-3-flash-preview",
    "gemini-3-pro-preview",
    "gemini-3.1-flash-lite-preview",
    "gemini-3.1-pro",
    "gemini-3.1-pro-preview",
];

pub fn build_endpoint(model: &str) -> String {
    format!(
        "{}/models/{}:streamGenerateContent?alt=sse",
        GCA_BASE, model
    )
}

pub async fn stream_generate(
    client: &reqwest::Client,
    access_token: &str,
    model: &str,
    request: &GenerateRequest,
    on_chunk: impl Fn(&str),
) -> Result<String, LayreamError> {
    let url = build_endpoint(model);

    let resp = client
        .post(&url)
        .header("Authorization", format!("Bearer {}", access_token))
        .header("x-goog-api-client", "google-cloud-intellij")
        .json(request)
        .send()
        .await
        .map_err(|e| LayreamError::Http(e.to_string()))?;

    if !resp.status().is_success() {
        let status = resp.status().as_u16();
        let body = resp.text().await.unwrap_or_default();
        return Err(LayreamError::ApiError { status, body });
    }

    let mut full_text = String::new();
    let mut stream = resp.bytes_stream();
    let mut buffer = String::new();

    while let Some(chunk) = stream.next().await {
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
    let url = format!("{}/models/{}:generateContent", GCA_BASE, model);

    let resp = client
        .post(&url)
        .header("Authorization", format!("Bearer {}", access_token))
        .header("x-goog-api-client", "google-cloud-intellij")
        .json(request)
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
        assert!(GCA_MODELS.contains(&"gemini-3.1-pro-preview"));
    }
}
