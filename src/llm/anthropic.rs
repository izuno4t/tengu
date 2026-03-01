use anyhow::{anyhow, Result};
use bytes::Bytes;
use futures_util::stream::{self, BoxStream, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::llm::{
    LlmBackend, LlmProvider, LlmRequest, LlmResponse, LlmStream, LlmStreamEvent, LlmUsage,
};

const DEFAULT_ANTHROPIC_BASE_URL: &str = "https://api.anthropic.com";
const DEFAULT_ANTHROPIC_VERSION: &str = "2023-06-01";
const DEFAULT_MAX_TOKENS: u32 = 8192;

#[derive(Debug, Clone)]
pub struct AnthropicBackend {
    pub base_url: String,
    pub max_tokens: u32,
}

#[derive(Debug, Serialize)]
struct MessageRequest {
    model: String,
    max_tokens: u32,
    messages: Vec<MessageInput>,
    stream: bool,
}

#[derive(Debug, Serialize)]
struct MessageInput {
    role: String,
    content: Vec<MessageContentBlock>,
}

#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum MessageContentBlock {
    Text { text: String },
    Image { source: ImageSource },
}

#[derive(Debug, Serialize)]
struct ImageSource {
    #[serde(rename = "type")]
    kind: &'static str,
    media_type: String,
    data: String,
}

#[derive(Debug, Deserialize)]
struct MessageResponse {
    content: Vec<ContentBlock>,
    #[serde(default)]
    usage: Option<MessageUsage>,
}

#[derive(Debug, Deserialize, Clone)]
struct MessageUsage {
    #[serde(default)]
    input_tokens: Option<u64>,
    #[serde(default)]
    output_tokens: Option<u64>,
    #[serde(default)]
    cache_creation_input_tokens: Option<u64>,
    #[serde(default)]
    cache_read_input_tokens: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct ContentBlock {
    #[serde(rename = "type")]
    kind: String,
    text: Option<String>,
}

impl AnthropicBackend {
    pub fn new(base_url: Option<String>, max_tokens: Option<u32>) -> Self {
        Self {
            base_url: base_url.unwrap_or_else(|| DEFAULT_ANTHROPIC_BASE_URL.to_string()),
            max_tokens: max_tokens.unwrap_or(DEFAULT_MAX_TOKENS),
        }
    }

    fn messages_url(&self) -> String {
        let base = self.base_url.trim_end_matches('/');
        if base.ends_with("/v1") {
            format!("{}/messages", base)
        } else {
            format!("{}/v1/messages", base)
        }
    }

    fn api_key(&self) -> Result<String> {
        std::env::var("ANTHROPIC_API_KEY").map_err(|_| anyhow!("ANTHROPIC_API_KEY is not set"))
    }

    fn request_body(&self, model: &str, request: &LlmRequest, stream: bool) -> MessageRequest {
        let mut content = vec![MessageContentBlock::Text {
            text: request.prompt.clone(),
        }];
        for image in &request.images {
            content.push(MessageContentBlock::Image {
                source: ImageSource {
                    kind: "base64",
                    media_type: image.media_type.clone(),
                    data: image.data_base64.clone(),
                },
            });
        }
        MessageRequest {
            model: model.to_string(),
            max_tokens: self.max_tokens,
            messages: vec![MessageInput {
                role: "user".to_string(),
                content,
            }],
            stream,
        }
    }

    fn collect_text(blocks: Vec<ContentBlock>) -> String {
        blocks
            .into_iter()
            .filter(|block| block.kind == "text")
            .filter_map(|block| block.text)
            .collect::<Vec<_>>()
            .join("")
    }

    fn normalize_usage(usage: MessageUsage, raw: Option<Value>) -> LlmUsage {
        let total_tokens = match (usage.input_tokens, usage.output_tokens) {
            (Some(input), Some(output)) => Some(input + output),
            _ => None,
        };
        LlmUsage {
            provider: "anthropic".to_string(),
            input_tokens: usage.input_tokens,
            output_tokens: usage.output_tokens,
            total_tokens,
            cache_creation_input_tokens: usage.cache_creation_input_tokens,
            cache_read_input_tokens: usage.cache_read_input_tokens,
            reasoning_tokens: None,
            raw,
        }
    }

    fn extract_usage_value(value: &Value) -> Option<LlmUsage> {
        let usage_value = value.get("usage").cloned().or_else(|| {
            value
                .get("message")
                .and_then(|msg| msg.get("usage"))
                .cloned()
        })?;
        let usage = serde_json::from_value::<MessageUsage>(usage_value.clone()).ok()?;
        Some(Self::normalize_usage(usage, Some(usage_value)))
    }

    fn parse_stream_event(data: &str) -> Result<Option<LlmStreamEvent>> {
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
            return Err(anyhow!("anthropic stream error: {}", error));
        }

        if let Some(usage) = Self::extract_usage_value(&value) {
            return Ok(Some(LlmStreamEvent::Usage(usage)));
        }

        if let Some(text) = value
            .get("delta")
            .and_then(|delta| delta.get("text"))
            .and_then(Value::as_str)
        {
            return Ok(Some(LlmStreamEvent::Text(text.to_string())));
        }

        if let Some(text) = value
            .get("content_block")
            .and_then(|block| block.get("text"))
            .and_then(Value::as_str)
        {
            if !text.is_empty() {
                return Ok(Some(LlmStreamEvent::Text(text.to_string())));
            }
        }

        Ok(None)
    }
}

#[async_trait::async_trait]
impl LlmBackend for AnthropicBackend {
    fn provider(&self) -> LlmProvider {
        LlmProvider::Anthropic
    }

    async fn generate(&self, model: &str, request: &LlmRequest) -> Result<LlmResponse> {
        let api_key = self.api_key()?;
        let client = reqwest::Client::new();
        let response = client
            .post(self.messages_url())
            .header("x-api-key", api_key)
            .header("anthropic-version", DEFAULT_ANTHROPIC_VERSION)
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .json(&self.request_body(model, request, false))
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(anyhow!("anthropic error: {} {}", status, body.trim()));
        }

        let body: MessageResponse = response.json().await?;
        Ok(LlmResponse {
            content: Self::collect_text(body.content),
            usage: body.usage.map(|usage| Self::normalize_usage(usage, None)),
        })
    }

    async fn generate_stream(&self, model: &str, request: &LlmRequest) -> Result<LlmStream> {
        let api_key = self.api_key()?;
        let client = reqwest::Client::new();
        let response = client
            .post(self.messages_url())
            .header("x-api-key", api_key)
            .header("anthropic-version", DEFAULT_ANTHROPIC_VERSION)
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .json(&self.request_body(model, request, true))
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(anyhow!("anthropic error: {} {}", status, body.trim()));
        }

        struct StreamState {
            stream: BoxStream<'static, Result<Bytes, reqwest::Error>>,
            buffer: String,
            pending_event: Option<String>,
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
                    if state.pending_event.is_some() || !state.pending_data.is_empty() {
                        let data = state.pending_data.join("\n");
                        state.pending_event = None;
                        state.pending_data.clear();
                        return AnthropicBackend::parse_stream_event(&data);
                    }
                    continue;
                }

                if let Some(event) = line.strip_prefix("event:") {
                    state.pending_event = Some(event.trim().to_string());
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
            pending_event: None,
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
                            match AnthropicBackend::parse_stream_event(&data) {
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
    fn builds_messages_url_from_default_base() {
        let backend = AnthropicBackend::new(None, None);
        assert_eq!(
            backend.messages_url(),
            "https://api.anthropic.com/v1/messages"
        );
    }

    #[test]
    fn parses_text_delta_from_stream_payload() {
        let payload =
            r#"{"type":"content_block_delta","delta":{"type":"text_delta","text":"hello"}}"#;
        let parsed = AnthropicBackend::parse_stream_event(payload).unwrap();
        assert!(matches!(parsed, Some(LlmStreamEvent::Text(text)) if text == "hello"));
    }

    #[test]
    fn parses_usage_from_stream_payload() {
        let payload = r#"{"type":"message_delta","usage":{"input_tokens":10,"output_tokens":4,"cache_creation_input_tokens":2,"cache_read_input_tokens":1}}"#;
        let parsed = AnthropicBackend::parse_stream_event(payload).unwrap();
        assert!(matches!(
            parsed,
            Some(LlmStreamEvent::Usage(usage))
                if usage.provider == "anthropic"
                    && usage.input_tokens == Some(10)
                    && usage.output_tokens == Some(4)
                    && usage.total_tokens == Some(14)
                    && usage.cache_creation_input_tokens == Some(2)
                    && usage.cache_read_input_tokens == Some(1)
        ));
    }
}
