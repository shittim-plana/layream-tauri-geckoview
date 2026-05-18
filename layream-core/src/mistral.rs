use std::collections::HashMap;

use futures::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::LayreamError;
use crate::retry::{self, CancelToken};

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
pub struct JsonSchemaSpec {
    pub name: String,
    pub schema: Value,
    #[serde(default)]
    pub strict: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponseFormat {
    #[serde(rename = "type")]
    pub format_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub json_schema: Option<JsonSchemaSpec>,
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
        let body = resp.text().await.unwrap_or_else(|e| format!("(body read failed: {e})"));
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
    cancel: Option<CancelToken>,
) -> Result<String, LayreamError> {
    let url = format!("{}/chat/completions", API_BASE);
    let auth = format!("Bearer {}", api_key);

    let mut stream_req = request.clone();
    stream_req.stream = Some(true);

    let resp = retry::retry_request(&cancel, || {
        let req = client
            .post(&url)
            .header("Authorization", &auth)
            .json(&stream_req);
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

/// Returns `true` if the model's capabilities indicate it supports chat completions.
/// Models without a `capabilities` object are excluded (soundness over completeness:
/// we only include models that explicitly declare chat support).
fn is_chat_model(model: &ModelInfo) -> bool {
    model
        .capabilities
        .as_ref()
        .and_then(|caps| caps.get("completion_chat"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
}

/// Extracts the base model name by stripping date/version suffixes and `-latest`.
///
/// Examples:
///   `"mistral-large-latest"` → `"mistral-large"`
///   `"mistral-medium-2505"` → `"mistral-medium"`
///   `"mistral-medium-2501"` → `"mistral-medium"`
///   `"codestral-2501"` → `"codestral"`
fn model_base_name(id: &str) -> &str {
    let s = id.strip_suffix("-latest").unwrap_or(id);
    // Strip trailing `-YYMM` or `-YYMMDD` date suffixes (4–6 digit patterns)
    if let Some(pos) = s.rfind('-') {
        let suffix = &s[pos + 1..];
        let all_digits = !suffix.is_empty() && suffix.chars().all(|c| c.is_ascii_digit());
        let is_date_suffix = all_digits && (suffix.len() == 4 || suffix.len() == 6);
        if is_date_suffix {
            return &s[..pos];
        }
    }
    s
}

pub async fn list_models(
    client: &reqwest::Client,
    api_key: &str,
) -> Result<Vec<ModelInfo>, LayreamError> {
    let url = format!("{}/models", API_BASE);
    let auth = format!("Bearer {}", api_key);

    let resp = retry::retry_request(&None, || {
        let req = client.get(&url).header("Authorization", &auth);
        async { req.send().await.map_err(|e| LayreamError::Http(e.to_string())) }
    }).await?;

    let list: ModelListResponse = resp
        .json()
        .await
        .map_err(|e| LayreamError::Http(e.to_string()))?;

    // 1. Filter: only models with capabilities.completion_chat == true
    let chat_models: Vec<ModelInfo> = list.data.into_iter().filter(is_chat_model).collect();

    // 2. Deduplicate: for each base name, keep the "-latest" variant if present,
    //    otherwise the model with the highest `created` timestamp.
    let mut best: HashMap<String, ModelInfo> = HashMap::new();
    for model in chat_models {
        let base = model_base_name(&model.id).to_owned();
        let dominated = best.get(&base).map_or(false, |existing| {
            let existing_is_latest = existing.id.ends_with("-latest");
            let new_is_latest = model.id.ends_with("-latest");
            if existing_is_latest && !new_is_latest {
                // Existing is -latest, new is not: existing wins
                true
            } else if !existing_is_latest && new_is_latest {
                // New is -latest: new wins (not dominated)
                false
            } else {
                // Both same suffix class: higher created wins
                existing.created.unwrap_or(0) >= model.created.unwrap_or(0)
            }
        });
        if !dominated {
            best.insert(base, model);
        }
    }

    let mut models: Vec<ModelInfo> = best.into_values().collect();
    models.sort_by(|a, b| {
        let a_latest = a.id.ends_with("-latest");
        let b_latest = b.id.ends_with("-latest");
        match (a_latest, b_latest) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => {
                // Prefer newer models, then alphabetical
                b.created
                    .unwrap_or(0)
                    .cmp(&a.created.unwrap_or(0))
                    .then_with(|| a.id.cmp(&b.id))
            }
        }
    });

    Ok(models)
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ModelInfo {
    pub id: String,
    pub object: Option<String>,
    pub created: Option<u64>,
    pub owned_by: Option<String>,
    /// Model capabilities (e.g. {"completion_chat": true, "completion_fim": false, ...}).
    /// Present in Mistral API responses; None if the field is absent.
    #[serde(default)]
    pub capabilities: Option<Value>,
    /// Model type string (e.g. "base", "fine-tuned").
    #[serde(default, rename = "type")]
    pub model_type: Option<String>,
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
            json_schema: None,
        };
        let json = serde_json::to_string(&rf).unwrap();
        assert!(json.contains("\"type\":\"json_object\""));
        assert!(!json.contains("json_schema"));
    }

    #[test]
    fn response_format_json_schema() {
        let rf = ResponseFormat {
            format_type: "json_schema".into(),
            json_schema: Some(JsonSchemaSpec {
                name: "user_message".into(),
                schema: serde_json::json!({
                    "type": "object",
                    "properties": { "name": { "type": "string" } }
                }),
                strict: true,
            }),
        };
        let json = serde_json::to_string(&rf).unwrap();
        assert!(json.contains("\"type\":\"json_schema\""));
        assert!(json.contains("\"json_schema\""));
        assert!(json.contains("\"name\":\"user_message\""));
        assert!(json.contains("\"strict\":true"));
        assert!(json.contains("\"properties\""));
    }

    // -- model filtering tests --

    fn make_model(id: &str, chat: Option<bool>, created: Option<u64>) -> ModelInfo {
        let capabilities = chat.map(|v| serde_json::json!({"completion_chat": v}));
        ModelInfo {
            id: id.to_string(),
            object: None,
            created,
            owned_by: None,
            capabilities,
            model_type: None,
        }
    }

    #[test]
    fn is_chat_model_filters_correctly() {
        // completion_chat == true → included
        assert!(is_chat_model(&make_model("mistral-large-latest", Some(true), None)));

        // completion_chat == false → excluded
        assert!(!is_chat_model(&make_model("mistral-embed", Some(false), None)));

        // No capabilities → excluded (soundness: unknown capability = not chat)
        assert!(!is_chat_model(&make_model("unknown-model", None, None)));

        // capabilities present but completion_chat absent → excluded
        let model = ModelInfo {
            id: "partial-caps".into(),
            object: None,
            created: None,
            owned_by: None,
            capabilities: Some(serde_json::json!({"completion_fim": true})),
            model_type: None,
        };
        assert!(!is_chat_model(&model));
    }

    #[test]
    fn model_base_name_extraction() {
        assert_eq!(model_base_name("mistral-large-latest"), "mistral-large");
        assert_eq!(model_base_name("mistral-medium-2505"), "mistral-medium");
        assert_eq!(model_base_name("mistral-medium-250115"), "mistral-medium");
        assert_eq!(model_base_name("codestral-2501"), "codestral");
        assert_eq!(model_base_name("mistral-large-2411"), "mistral-large");
        // No suffix to strip
        assert_eq!(model_base_name("pixtral-large"), "pixtral-large");
        // 3-digit suffix is NOT a date → kept as-is
        assert_eq!(model_base_name("model-123"), "model-123");
        // 5-digit suffix is NOT a date → kept as-is
        assert_eq!(model_base_name("model-12345"), "model-12345");
    }

    #[test]
    fn deduplication_prefers_latest() {
        let models = vec![
            make_model("mistral-large-latest", Some(true), Some(100)),
            make_model("mistral-large-2411", Some(true), Some(200)),
            make_model("mistral-large-2505", Some(true), Some(300)),
        ];

        // Simulate the dedup logic
        let mut best: std::collections::HashMap<String, ModelInfo> = std::collections::HashMap::new();
        for model in models {
            let base = model_base_name(&model.id).to_owned();
            let dominated = best.get(&base).map_or(false, |existing| {
                let existing_is_latest = existing.id.ends_with("-latest");
                let new_is_latest = model.id.ends_with("-latest");
                if existing_is_latest && !new_is_latest {
                    true
                } else if !existing_is_latest && new_is_latest {
                    false
                } else {
                    existing.created.unwrap_or(0) >= model.created.unwrap_or(0)
                }
            });
            if !dominated {
                best.insert(base, model);
            }
        }

        assert_eq!(best.len(), 1);
        assert_eq!(best["mistral-large"].id, "mistral-large-latest");
    }
}
