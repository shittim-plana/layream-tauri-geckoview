use futures::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::LayreamError;

const API_BASE: &str = "https://api.mistral.ai/v1";

#[derive(Debug, Clone, Serialize)]
pub struct ChatRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frequency_penalty: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub presence_penalty: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub random_seed: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_format: Option<ResponseFormat>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_effort: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<Tool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponseFormat {
    #[serde(rename = "type")]
    pub format_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tool {
    #[serde(rename = "type")]
    pub tool_type: String,
    pub function: ToolFunction,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolFunction {
    pub name: String,
    pub description: String,
    pub parameters: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    #[serde(rename = "type")]
    pub call_type: String,
    pub function: ToolCallFunction,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallFunction {
    pub name: String,
    pub arguments: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ChatResponse {
    pub id: Option<String>,
    pub choices: Vec<Choice>,
    pub model: Option<String>,
    pub usage: Option<Usage>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Choice {
    pub index: u32,
    pub message: Option<ChatMessage>,
    pub delta: Option<DeltaMessage>,
    pub finish_reason: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DeltaMessage {
    pub role: Option<String>,
    pub content: Option<String>,
    pub tool_calls: Option<Vec<ToolCall>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Usage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

pub async fn chat(
    client: &reqwest::Client,
    api_key: &str,
    request: &ChatRequest,
) -> Result<ChatResponse, LayreamError> {
    let url = format!("{}/chat/completions", API_BASE);

    let resp = client
        .post(&url)
        .header("Authorization", format!("Bearer {}", api_key))
        .json(request)
        .send()
        .await
        .map_err(|e| LayreamError::Http(e.to_string()))?;

    if !resp.status().is_success() {
        let status = resp.status().as_u16();
        let body = resp.text().await.unwrap_or_default();
        return Err(LayreamError::ApiError { status, body });
    }

    resp.json()
        .await
        .map_err(|e| LayreamError::Http(e.to_string()))
}

pub async fn chat_stream(
    client: &reqwest::Client,
    api_key: &str,
    request: &ChatRequest,
    on_chunk: impl Fn(&str),
) -> Result<String, LayreamError> {
    let url = format!("{}/chat/completions", API_BASE);

    let mut stream_req = request.clone();
    stream_req.stream = Some(true);

    let resp = client
        .post(&url)
        .header("Authorization", format!("Bearer {}", api_key))
        .json(&stream_req)
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

            if line == "data: [DONE]" {
                break;
            }

            if let Some(json_str) = line.strip_prefix("data: ") {
                if let Ok(chunk) = serde_json::from_str::<ChatResponse>(json_str) {
                    for choice in &chunk.choices {
                        if let Some(delta) = &choice.delta {
                            if let Some(content) = &delta.content {
                                full_text.push_str(content);
                                on_chunk(content);
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(full_text)
}

pub async fn list_models(
    client: &reqwest::Client,
    api_key: &str,
) -> Result<Vec<ModelInfo>, LayreamError> {
    let url = format!("{}/models", API_BASE);

    let resp = client
        .get(&url)
        .header("Authorization", format!("Bearer {}", api_key))
        .send()
        .await
        .map_err(|e| LayreamError::Http(e.to_string()))?;

    if !resp.status().is_success() {
        let status = resp.status().as_u16();
        let body = resp.text().await.unwrap_or_default();
        return Err(LayreamError::ApiError { status, body });
    }

    let list: ModelListResponse = resp
        .json()
        .await
        .map_err(|e| LayreamError::Http(e.to_string()))?;

    Ok(list.data)
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ModelInfo {
    pub id: String,
    pub object: Option<String>,
    pub created: Option<u64>,
    pub owned_by: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ModelListResponse {
    data: Vec<ModelInfo>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chat_request_full_serialization() {
        let req = ChatRequest {
            model: "mistral-medium-2508".into(),
            messages: vec![ChatMessage {
                role: "user".into(),
                content: "Hello".into(),
                tool_calls: None,
                tool_call_id: None,
            }],
            temperature: Some(0.7),
            top_p: None,
            max_tokens: Some(1024),
            stream: None,
            frequency_penalty: Some(0.5),
            presence_penalty: None,
            stop: Some(vec!["</s>".into()]),
            random_seed: None,
            response_format: None,
            reasoning_effort: None,
            tools: None,
            tool_choice: None,
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("mistral-medium-2508"));
        assert!(json.contains("\"frequency_penalty\":0.5"));
        assert!(json.contains("</s>"));
        assert!(!json.contains("top_p"));
        assert!(!json.contains("presence_penalty"));
        assert!(!json.contains("reasoning_effort"));
    }

    #[test]
    fn reasoning_effort_serialization() {
        let req = ChatRequest {
            model: "magistral-medium-2509".into(),
            messages: vec![ChatMessage {
                role: "user".into(),
                content: "Think step by step".into(),
                tool_calls: None,
                tool_call_id: None,
            }],
            temperature: None,
            top_p: None,
            max_tokens: None,
            stream: None,
            frequency_penalty: None,
            presence_penalty: None,
            stop: None,
            random_seed: None,
            response_format: None,
            reasoning_effort: Some("high".into()),
            tools: None,
            tool_choice: None,
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("\"reasoning_effort\":\"high\""));
    }

    #[test]
    fn stream_response_parsing() {
        let json = r#"{"id":"abc","choices":[{"index":0,"delta":{"content":"Hi"},"finish_reason":null}]}"#;
        let resp: ChatResponse = serde_json::from_str(json).unwrap();
        assert_eq!(
            resp.choices[0].delta.as_ref().unwrap().content.as_deref(),
            Some("Hi")
        );
    }

    #[test]
    fn tool_call_response_parsing() {
        let json = r#"{"id":"x","choices":[{"index":0,"message":{"role":"assistant","content":"","tool_calls":[{"id":"t1","type":"function","function":{"name":"get_weather","arguments":"{\"city\":\"Seoul\"}"}}]},"finish_reason":"tool_calls"}]}"#;
        let resp: ChatResponse = serde_json::from_str(json).unwrap();
        let msg = resp.choices[0].message.as_ref().unwrap();
        let tc = msg.tool_calls.as_ref().unwrap();
        assert_eq!(tc[0].function.name, "get_weather");
        assert!(tc[0].function.arguments.contains("Seoul"));
    }

    #[test]
    fn response_format_json() {
        let rf = ResponseFormat {
            format_type: "json_object".into(),
        };
        let json = serde_json::to_string(&rf).unwrap();
        assert!(json.contains("\"type\":\"json_object\""));
    }
}
