use anyhow::{anyhow, Result};
use bytes::Bytes;
use futures_util::stream::{self, BoxStream, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::llm::{
    LlmBackend, LlmProvider, LlmRequest, LlmResponse, LlmStream, LlmStreamEvent, LlmUsage,
};

const DEFAULT_OPENAI_BASE_URL: &str = "https://api.openai.com/v1";

#[derive(Debug, Clone)]
pub struct OpenAiBackend {
    pub base_url: String,
    pub max_tokens: Option<u32>,
}

#[derive(Debug, Serialize)]
struct ChatCompletionRequest {
    model: String,
    messages: Vec<ChatMessage>,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stream_options: Option<StreamOptions>,
}

#[derive(Debug, Serialize)]
struct StreamOptions {
    include_usage: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct ChatMessage {
    role: String,
    content: Value,
}

#[derive(Debug, Deserialize)]
struct ChatCompletionResponse {
    choices: Vec<ChatChoice>,
    #[serde(default)]
    usage: Option<OpenAiUsage>,
}

#[derive(Debug, Deserialize, Clone)]
struct OpenAiUsage {
    #[serde(default)]
    prompt_tokens: Option<u64>,
    #[serde(default)]
    completion_tokens: Option<u64>,
    #[serde(default)]
    total_tokens: Option<u64>,
    #[serde(default)]
    prompt_tokens_details: Option<OpenAiPromptTokensDetails>,
    #[serde(default)]
    completion_tokens_details: Option<OpenAiCompletionTokensDetails>,
}

#[derive(Debug, Deserialize, Clone)]
struct OpenAiPromptTokensDetails {
    #[serde(default)]
    cached_tokens: Option<u64>,
}

#[derive(Debug, Deserialize, Clone)]
struct OpenAiCompletionTokensDetails {
    #[serde(default)]
    reasoning_tokens: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct ChatChoice {
    message: Option<ChatMessageResponse>,
}

#[derive(Debug, Deserialize)]
struct ChatMessageResponse {
    content: Value,
}

impl OpenAiBackend {
    pub fn new(base_url: Option<String>, max_tokens: Option<u32>) -> Self {
        Self {
            base_url: base_url.unwrap_or_else(|| DEFAULT_OPENAI_BASE_URL.to_string()),
            max_tokens,
        }
    }

    fn chat_completions_url(&self) -> String {
        let base = self.base_url.trim_end_matches('/');
        if base.ends_with("/v1") {
            format!("{}/chat/completions", base)
        } else {
            format!("{}/v1/chat/completions", base)
        }
    }

    fn api_key(&self) -> Result<String> {
        std::env::var("OPENAI_API_KEY").map_err(|_| anyhow!("OPENAI_API_KEY is not set"))
    }

    fn request_body(
        &self,
        model: &str,
        request: &LlmRequest,
        stream: bool,
    ) -> ChatCompletionRequest {
        let content = if request.images.is_empty() {
            Value::String(request.prompt.clone())
        } else {
            let mut items = vec![serde_json::json!({
                "type": "text",
                "text": request.prompt,
            })];
            for image in &request.images {
                items.push(serde_json::json!({
                    "type": "image_url",
                    "image_url": {
                        "url": format!("data:{};base64,{}", image.media_type, image.data_base64)
                    }
                }));
            }
            Value::Array(items)
        };
        ChatCompletionRequest {
            model: model.to_string(),
            messages: vec![ChatMessage {
                role: "user".to_string(),
                content,
            }],
            stream,
            max_tokens: self.max_tokens,
            stream_options: stream.then_some(StreamOptions {
                include_usage: true,
            }),
        }
    }

    fn normalize_usage(usage: OpenAiUsage, raw: Option<Value>) -> LlmUsage {
        LlmUsage {
            provider: "openai".to_string(),
            input_tokens: usage.prompt_tokens,
            output_tokens: usage.completion_tokens,
            total_tokens: usage.total_tokens.or_else(|| {
                match (usage.prompt_tokens, usage.completion_tokens) {
                    (Some(input), Some(output)) => Some(input + output),
                    _ => None,
                }
            }),
            cache_creation_input_tokens: None,
            cache_read_input_tokens: usage
                .prompt_tokens_details
                .and_then(|details| details.cached_tokens),
            reasoning_tokens: usage
                .completion_tokens_details
                .and_then(|details| details.reasoning_tokens),
            raw,
        }
    }

    fn parse_stream_data(data: &str) -> Result<Option<LlmStreamEvent>> {
        let payload = data.trim();
        if payload.is_empty() || payload == "[DONE]" {
            return Ok(None);
        }

        let value: Value = serde_json::from_str(payload)?;
        if let Some(error) = value
            .get("error")
            .and_then(|err| err.get("message"))
            .and_then(Value::as_str)
        {
            return Err(anyhow!("openai stream error: {}", error));
        }

        if let Some(usage_value) = value.get("usage").cloned() {
            if let Ok(usage) = serde_json::from_value::<OpenAiUsage>(usage_value.clone()) {
                return Ok(Some(LlmStreamEvent::Usage(Self::normalize_usage(
                    usage,
                    Some(usage_value),
                ))));
            }
        }

        let Some(choice) = value
            .get("choices")
            .and_then(Value::as_array)
            .and_then(|choices| choices.first())
        else {
            return Ok(None);
        };

        let Some(delta) = choice.get("delta") else {
            return Ok(None);
        };

        if let Some(text) = delta.get("content").and_then(Value::as_str) {
            if !text.is_empty() {
                return Ok(Some(LlmStreamEvent::Text(text.to_string())));
            }
        }

        Ok(None)
    }
}

#[async_trait::async_trait]
impl LlmBackend for OpenAiBackend {
    fn provider(&self) -> LlmProvider {
        LlmProvider::OpenAI
    }

    async fn generate(&self, model: &str, request: &LlmRequest) -> Result<LlmResponse> {
        let api_key = self.api_key()?;
        let client = reqwest::Client::new();
        let response = client
            .post(self.chat_completions_url())
            .bearer_auth(api_key)
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .json(&self.request_body(model, request, false))
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(anyhow!("openai error: {} {}", status, body.trim()));
        }

        let body: ChatCompletionResponse = response.json().await?;
        let content = body
            .choices
            .into_iter()
            .find_map(|choice| choice.message.map(|message| message.content))
            .map(|content| match content {
                Value::String(text) => text,
                Value::Array(items) => items
                    .into_iter()
                    .filter_map(|item| item.get("text").and_then(Value::as_str).map(str::to_string))
                    .collect::<Vec<_>>()
                    .join(""),
                _ => String::new(),
            })
            .unwrap_or_default();
        Ok(LlmResponse {
            content,
            usage: body.usage.map(|usage| Self::normalize_usage(usage, None)),
        })
    }

    async fn generate_stream(&self, model: &str, request: &LlmRequest) -> Result<LlmStream> {
        let api_key = self.api_key()?;
        let client = reqwest::Client::new();
        let response = client
            .post(self.chat_completions_url())
            .bearer_auth(api_key)
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .json(&self.request_body(model, request, true))
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(anyhow!("openai error: {} {}", status, body.trim()));
        }

        struct StreamState {
            stream: BoxStream<'static, Result<Bytes, reqwest::Error>>,
            buffer: String,
            pending_data: Vec<String>,
            finished: bool,
        }

        fn take_message(state: &mut StreamState) -> Result<Option<LlmStreamEvent>> {
            while let Some(idx) = state.buffer.find('\n') {
                let mut line = state.buffer[..idx].to_string();
                state.buffer = state.buffer[idx + 1..].to_string();
                if line.ends_with('\r') {
                    line.pop();
                }

                if line.is_empty() {
                    if !state.pending_data.is_empty() {
                        let data = state.pending_data.join("\n");
                        state.pending_data.clear();
                        return OpenAiBackend::parse_stream_data(&data);
                    }
                    continue;
                }

                if let Some(data) = line.strip_prefix("data:") {
                    state.pending_data.push(data.trim_start().to_string());
                }
            }

            Ok(None)
        }

        let state = StreamState {
            stream: Box::pin(response.bytes_stream()),
            buffer: String::new(),
            pending_data: Vec::new(),
            finished: false,
        };

        let output = stream::unfold(state, |mut state| async move {
            if state.finished {
                return None;
            }

            loop {
                match take_message(&mut state) {
                    Ok(Some(text)) => return Some((Ok(text), state)),
                    Ok(None) => {}
                    Err(err) => {
                        state.finished = true;
                        return Some((Err(err), state));
                    }
                }

                match state.stream.next().await {
                    Some(Ok(chunk)) => {
                        state.buffer.push_str(&String::from_utf8_lossy(&chunk));
                    }
                    Some(Err(err)) => {
                        state.finished = true;
                        return Some((Err(anyhow::Error::new(err)), state));
                    }
                    None => {
                        state.finished = true;
                        if !state.pending_data.is_empty() {
                            let data = state.pending_data.join("\n");
                            match OpenAiBackend::parse_stream_data(&data) {
                                Ok(Some(text)) => return Some((Ok(text), state)),
                                Ok(None) => return None,
                                Err(err) => return Some((Err(err), state)),
                            }
                        }
                        return None;
                    }
                }
            }
        });

        Ok(Box::pin(output) as BoxStream<'static, Result<LlmStreamEvent>>)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_chat_completions_url_from_default_base() {
        let backend = OpenAiBackend::new(None, None);
        assert_eq!(
            backend.chat_completions_url(),
            "https://api.openai.com/v1/chat/completions"
        );
    }

    #[test]
    fn parses_content_delta_from_stream_payload() {
        let payload = r#"{"choices":[{"delta":{"content":"hello"}}]}"#;
        let parsed = OpenAiBackend::parse_stream_data(payload).unwrap();
        assert!(matches!(parsed, Some(LlmStreamEvent::Text(text)) if text == "hello"));
    }
}
