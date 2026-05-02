use futures::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::LayreamError;

#[derive(Debug, Clone, Serialize)]
pub struct GenerateRequest {
    pub contents: Vec<Content>,
    #[serde(rename = "systemInstruction", skip_serializing_if = "Option::is_none")]
    pub system_instruction: Option<Content>,
    #[serde(rename = "safetySettings")]
    pub safety_settings: Vec<SafetySetting>,
    #[serde(rename = "generationConfig")]
    pub generation_config: GenerationConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Content {
    pub role: String,
    pub parts: Vec<Part>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Part {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thought: Option<bool>,
    #[serde(rename = "inlineData", skip_serializing_if = "Option::is_none")]
    pub inline_data: Option<InlineData>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InlineData {
    #[serde(rename = "mimeType")]
    pub mime_type: String,
    pub data: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct SafetySetting {
    pub category: String,
    pub threshold: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct GenerationConfig {
    #[serde(rename = "maxOutputTokens")]
    pub max_output_tokens: u32,
    pub temperature: f64,
    #[serde(rename = "thinkingConfig", skip_serializing_if = "Option::is_none")]
    pub thinking_config: Option<ThinkingConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_k: Option<u32>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
pub enum ThinkingConfig {
    Level {
        #[serde(rename = "thinkingLevel")]
        thinking_level: String,
    },
    Budget {
        #[serde(rename = "thinkingBudget")]
        thinking_budget: i32,
    },
}

#[derive(Debug, Clone, Deserialize)]
pub struct StreamChunk {
    pub candidates: Option<Vec<Candidate>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Candidate {
    pub content: Option<Content>,
    #[serde(rename = "finishReason")]
    pub finish_reason: Option<String>,
}

pub fn default_safety_settings() -> Vec<SafetySetting> {
    [
        "HARM_CATEGORY_SEXUALLY_EXPLICIT",
        "HARM_CATEGORY_HATE_SPEECH",
        "HARM_CATEGORY_HARASSMENT",
        "HARM_CATEGORY_DANGEROUS_CONTENT",
        "HARM_CATEGORY_CIVIC_INTEGRITY",
    ]
    .iter()
    .map(|cat| SafetySetting {
        category: cat.to_string(),
        threshold: "BLOCK_NONE".into(),
    })
    .collect()
}

pub fn build_endpoint(project_id: &str, region: &str, model: &str) -> String {
    let host = if region == "global" {
        "aiplatform.googleapis.com".to_string()
    } else {
        format!("{}-aiplatform.googleapis.com", region)
    };
    format!(
        "https://{}/v1/projects/{}/locations/{}/publishers/google/models/{}:streamGenerateContent?alt=sse",
        host, project_id, region, model
    )
}

pub async fn stream_generate(
    client: &reqwest::Client,
    access_token: &str,
    project_id: &str,
    region: &str,
    model: &str,
    request: &GenerateRequest,
    on_chunk: impl Fn(&str),
) -> Result<String, LayreamError> {
    let url = build_endpoint(project_id, region, model);

    let resp = client
        .post(&url)
        .header("Authorization", format!("Bearer {}", access_token))
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
    project_id: &str,
    region: &str,
    model: &str,
    request: &GenerateRequest,
) -> Result<String, LayreamError> {
    let host = if region == "global" {
        "aiplatform.googleapis.com".to_string()
    } else {
        format!("{}-aiplatform.googleapis.com", region)
    };
    let url = format!(
        "https://{}/v1/projects/{}/locations/{}/publishers/google/models/{}:generateContent",
        host, project_id, region, model
    );

    let resp = client
        .post(&url)
        .header("Authorization", format!("Bearer {}", access_token))
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

#[derive(Debug, Clone, Deserialize)]
pub struct VertexModelInfo {
    pub name: String,
    #[serde(rename = "displayName")]
    pub display_name: Option<String>,
    pub description: Option<String>,
}

pub async fn list_models(
    client: &reqwest::Client,
    access_token: &str,
    region: &str,
) -> Result<Vec<String>, LayreamError> {
    let host = if region == "global" {
        "aiplatform.googleapis.com".to_string()
    } else {
        format!("{}-aiplatform.googleapis.com", region)
    };
    let url = format!("https://{}/v1/publishers/google/models", host);

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

    let mut models = Vec::new();
    if let Some(arr) = body.get("models").and_then(|m| m.as_array()) {
        for entry in arr {
            if let Some(name) = entry.get("name").and_then(|n| n.as_str()) {
                let model_id = name.strip_prefix("publishers/google/models/").unwrap_or(name);
                models.push(model_id.to_string());
            }
        }
    }
    models.sort();
    Ok(models)
}

pub async fn embed_content(
    client: &reqwest::Client,
    access_token: &str,
    project_id: &str,
    region: &str,
    model: &str,
    text: &str,
) -> Result<Vec<f64>, LayreamError> {
    let host = if region == "global" {
        "aiplatform.googleapis.com".to_string()
    } else {
        format!("{}-aiplatform.googleapis.com", region)
    };
    let url = format!(
        "https://{}/v1/projects/{}/locations/{}/publishers/google/models/{}:embedContent",
        host, project_id, region, model
    );

    let body = serde_json::json!({
        "content": {
            "parts": [{ "text": text }]
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
        let body = resp.text().await.unwrap_or_default();
        return Err(LayreamError::ApiError { status, body });
    }

    let resp_body: Value = resp
        .json()
        .await
        .map_err(|e| LayreamError::Http(e.to_string()))?;

    let values = resp_body
        .get("embedding")
        .and_then(|e| e.get("values"))
        .and_then(|v| v.as_array())
        .ok_or_else(|| LayreamError::Http("missing embedding.values".into()))?;

    Ok(values.iter().filter_map(|v| v.as_f64()).collect())
}

pub async fn batch_embed_contents(
    client: &reqwest::Client,
    access_token: &str,
    project_id: &str,
    region: &str,
    model: &str,
    texts: &[&str],
) -> Result<Vec<Vec<f64>>, LayreamError> {
    let host = if region == "global" {
        "aiplatform.googleapis.com".to_string()
    } else {
        format!("{}-aiplatform.googleapis.com", region)
    };
    let url = format!(
        "https://{}/v1/projects/{}/locations/{}/publishers/google/models/{}:batchEmbedContents",
        host, project_id, region, model
    );

    let requests: Vec<Value> = texts
        .iter()
        .map(|t| serde_json::json!({
            "model": format!("models/{}", model),
            "content": { "parts": [{ "text": t }] }
        }))
        .collect();

    let body = serde_json::json!({ "requests": requests });

    let resp = client
        .post(&url)
        .header("Authorization", format!("Bearer {}", access_token))
        .json(&body)
        .send()
        .await
        .map_err(|e| LayreamError::Http(e.to_string()))?;

    if !resp.status().is_success() {
        let status = resp.status().as_u16();
        let body = resp.text().await.unwrap_or_default();
        return Err(LayreamError::ApiError { status, body });
    }

    let resp_body: Value = resp
        .json()
        .await
        .map_err(|e| LayreamError::Http(e.to_string()))?;

    let mut embeddings = Vec::new();
    if let Some(arr) = resp_body.get("embeddings").and_then(|e| e.as_array()) {
        for entry in arr {
            if let Some(values) = entry.get("values").and_then(|v| v.as_array()) {
                embeddings.push(values.iter().filter_map(|v| v.as_f64()).collect());
            }
        }
    }

    Ok(embeddings)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn endpoint_regional() {
        let url = build_endpoint("my-project", "us-central1", "gemini-2.5-flash");
        assert_eq!(
            url,
            "https://us-central1-aiplatform.googleapis.com/v1/projects/my-project/locations/us-central1/publishers/google/models/gemini-2.5-flash:streamGenerateContent?alt=sse"
        );
    }

    #[test]
    fn endpoint_global() {
        let url = build_endpoint("proj", "global", "gemini-3.0-flash-preview");
        assert!(url.starts_with("https://aiplatform.googleapis.com/"));
        assert!(url.contains("locations/global"));
    }

}
