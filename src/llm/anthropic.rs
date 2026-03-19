use anyhow::{anyhow, Result};
use bytes::Bytes;
use futures_util::stream::{self, BoxStream, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::llm::{
    ChatRequest, ChatResponse, ChatStream, ChatStreamEvent, ContentBlock, LlmBackend, LlmProvider,
    LlmRequest, LlmResponse, LlmStream, LlmStreamEvent, LlmUsage, StopReason,
};

const DEFAULT_ANTHROPIC_BASE_URL: &str = "https://api.anthropic.com";
const DEFAULT_ANTHROPIC_VERSION: &str = "2023-06-01";
const DEFAULT_MAX_TOKENS: u32 = 8192;

#[derive(Debug, Clone)]
pub struct AnthropicBackend {
    pub base_url: String,
    pub max_tokens: u32,
}

// -- Legacy request/response types (for generate/generate_stream) --

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
    content: Vec<ResponseContentBlock>,
    #[serde(default)]
    usage: Option<MessageUsage>,
    #[serde(default)]
    stop_reason: Option<String>,
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
struct ResponseContentBlock {
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    input: Option<Value>,
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

    fn collect_text(blocks: &[ResponseContentBlock]) -> String {
        blocks
            .iter()
            .filter(|block| block.kind == "text")
            .filter_map(|block| block.text.as_deref())
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

    // -- Chat API (tool_use) helpers --

    fn build_chat_body(&self, model: &str, request: &ChatRequest) -> Value {
        let mut messages = Vec::new();
        for msg in &request.messages {
            let role = match msg.role {
                crate::llm::MessageRole::User => "user",
                crate::llm::MessageRole::Assistant => "assistant",
            };
            let content = msg
                .content
                .iter()
                .map(|block| match block {
                    ContentBlock::Text { text } => {
                        serde_json::json!({"type": "text", "text": text})
                    }
                    ContentBlock::ToolUse { id, name, input } => {
                        serde_json::json!({
                            "type": "tool_use",
                            "id": id,
                            "name": name,
                            "input": input,
                        })
                    }
                    ContentBlock::ToolResult {
                        tool_use_id,
                        content,
                        is_error,
                    } => {
                        serde_json::json!({
                            "type": "tool_result",
                            "tool_use_id": tool_use_id,
                            "content": content,
                            "is_error": is_error,
                        })
                    }
                    ContentBlock::Thinking { thinking } => {
                        serde_json::json!({
                            "type": "thinking",
                            "thinking": thinking,
                        })
                    }
                })
                .collect::<Vec<_>>();
            messages.push(serde_json::json!({"role": role, "content": content}));
        }

        let tools: Vec<Value> = request
            .tools
            .iter()
            .map(|tool| {
                serde_json::json!({
                    "name": tool.name,
                    "description": tool.description,
                    "input_schema": tool.input_schema,
                })
            })
            .collect();

        let mut body = serde_json::json!({
            "model": model,
            "max_tokens": request.max_tokens,
            "messages": messages,
        });

        if !tools.is_empty() {
            body["tools"] = Value::Array(tools);
        }

        if let Some(system) = &request.system {
            // Use structured system with cache_control for prompt caching
            body["system"] = serde_json::json!([{
                "type": "text",
                "text": system,
                "cache_control": {"type": "ephemeral"}
            }]);
        }

        body
    }

    fn parse_chat_response(body: MessageResponse) -> ChatResponse {
        let content = body
            .content
            .into_iter()
            .filter_map(|block| match block.kind.as_str() {
                "text" => block.text.map(|text| ContentBlock::Text { text }),
                "tool_use" => {
                    let id = block.id.unwrap_or_default();
                    let name = block.name.unwrap_or_default();
                    let input = block.input.unwrap_or(Value::Object(Default::default()));
                    Some(ContentBlock::ToolUse { id, name, input })
                }
                _ => None,
            })
            .collect();

        let stop_reason = match body.stop_reason.as_deref() {
            Some("tool_use") => StopReason::ToolUse,
            Some("max_tokens") => StopReason::MaxTokens,
            _ => StopReason::EndTurn,
        };

        ChatResponse {
            content,
            stop_reason,
            usage: body.usage.map(|u| Self::normalize_usage(u, None)),
        }
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
            content: Self::collect_text(&body.content),
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

    async fn chat(&self, model: &str, request: &ChatRequest) -> Result<ChatResponse> {
        let api_key = self.api_key()?;
        let client = reqwest::Client::new();
        let body = self.build_chat_body(model, request);

        let response = client
            .post(self.messages_url())
            .header("x-api-key", &api_key)
            .header("anthropic-version", DEFAULT_ANTHROPIC_VERSION)
            .header("anthropic-beta", "prompt-caching-2024-07-31")
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .json(&body)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let resp_body = response.text().await.unwrap_or_default();
            return Err(anyhow!("anthropic error: {} {}", status, resp_body.trim()));
        }

        let msg_response: MessageResponse = response.json().await?;
        Ok(Self::parse_chat_response(msg_response))
    }

    async fn chat_stream(&self, model: &str, request: &ChatRequest) -> Result<ChatStream> {
        let api_key = self.api_key()?;
        let client = reqwest::Client::new();
        let mut body = self.build_chat_body(model, request);
        body["stream"] = Value::Bool(true);

        let response = client
            .post(self.messages_url())
            .header("x-api-key", &api_key)
            .header("anthropic-version", DEFAULT_ANTHROPIC_VERSION)
            .header("anthropic-beta", "prompt-caching-2024-07-31")
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .json(&body)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let resp_body = response.text().await.unwrap_or_default();
            return Err(anyhow!("anthropic error: {} {}", status, resp_body.trim()));
        }

        /// State for accumulating tool_use blocks from streaming events.
        struct ChatStreamState {
            stream: BoxStream<'static, Result<Bytes, reqwest::Error>>,
            buffer: String,
            pending_data: Vec<String>,
            finished: bool,
            // Track current content block being streamed
            current_block_type: Option<String>,
            current_tool_id: Option<String>,
            current_tool_name: Option<String>,
            current_tool_input_json: String,
        }

        fn parse_chat_stream_event(
            state: &mut ChatStreamState,
            data: &str,
        ) -> Result<Option<ChatStreamEvent>> {
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

            let event_type = value
                .get("type")
                .and_then(Value::as_str)
                .unwrap_or("");

            match event_type {
                "content_block_start" => {
                    if let Some(block) = value.get("content_block") {
                        let block_type = block
                            .get("type")
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .to_string();
                        state.current_block_type = Some(block_type.clone());
                        if block_type == "tool_use" {
                            state.current_tool_id = block
                                .get("id")
                                .and_then(Value::as_str)
                                .map(String::from);
                            state.current_tool_name = block
                                .get("name")
                                .and_then(Value::as_str)
                                .map(String::from);
                            state.current_tool_input_json.clear();
                        }
                    }
                    Ok(None)
                }
                "content_block_delta" => {
                    if let Some(delta) = value.get("delta") {
                        let delta_type = delta
                            .get("type")
                            .and_then(Value::as_str)
                            .unwrap_or("");
                        match delta_type {
                            "text_delta" => {
                                if let Some(text) = delta.get("text").and_then(Value::as_str) {
                                    return Ok(Some(ChatStreamEvent::TextDelta(
                                        text.to_string(),
                                    )));
                                }
                            }
                            "thinking_delta" => {
                                if let Some(thinking) = delta.get("thinking").and_then(Value::as_str) {
                                    return Ok(Some(ChatStreamEvent::ThinkingDelta(
                                        thinking.to_string(),
                                    )));
                                }
                            }
                            "input_json_delta" => {
                                if let Some(json_part) =
                                    delta.get("partial_json").and_then(Value::as_str)
                                {
                                    state.current_tool_input_json.push_str(json_part);
                                }
                            }
                            _ => {}
                        }
                    }
                    Ok(None)
                }
                "content_block_stop" => {
                    if state.current_block_type.as_deref() == Some("tool_use") {
                        let id = state.current_tool_id.take().unwrap_or_default();
                        let name = state.current_tool_name.take().unwrap_or_default();
                        let input: Value =
                            serde_json::from_str(&state.current_tool_input_json)
                                .unwrap_or(Value::Object(Default::default()));
                        state.current_tool_input_json.clear();
                        state.current_block_type = None;
                        return Ok(Some(ChatStreamEvent::ToolUse { id, name, input }));
                    }
                    state.current_block_type = None;
                    Ok(None)
                }
                "message_delta" => {
                    // Extract stop_reason and usage
                    if let Some(usage) =
                        AnthropicBackend::extract_usage_value(&value)
                    {
                        return Ok(Some(ChatStreamEvent::Usage(usage)));
                    }
                    let stop_reason = value
                        .get("delta")
                        .and_then(|d| d.get("stop_reason"))
                        .and_then(Value::as_str);
                    match stop_reason {
                        Some("tool_use") => {
                            Ok(Some(ChatStreamEvent::Done(StopReason::ToolUse)))
                        }
                        Some("max_tokens") => {
                            Ok(Some(ChatStreamEvent::Done(StopReason::MaxTokens)))
                        }
                        Some("end_turn") => {
                            Ok(Some(ChatStreamEvent::Done(StopReason::EndTurn)))
                        }
                        _ => Ok(None),
                    }
                }
                "message_start" => {
                    // Extract initial usage from message_start
                    if let Some(usage) = value
                        .get("message")
                        .and_then(|m| m.get("usage"))
                    {
                        if let Ok(u) = serde_json::from_value::<MessageUsage>(usage.clone()) {
                            return Ok(Some(ChatStreamEvent::Usage(
                                AnthropicBackend::normalize_usage(u, Some(usage.clone())),
                            )));
                        }
                    }
                    Ok(None)
                }
                "message_stop" => {
                    state.finished = true;
                    Ok(None)
                }
                _ => Ok(None),
            }
        }

        fn take_chat_event(
            state: &mut ChatStreamState,
        ) -> Result<Option<ChatStreamEvent>> {
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
                        return parse_chat_stream_event(state, &data);
                    }
                    continue;
                }

                if line.starts_with("event:") {
                    continue;
                }

                if let Some(data) = line.strip_prefix("data:") {
                    state.pending_data.push(data.trim_start().to_string());
                }
            }
            Ok(None)
        }

        let state = ChatStreamState {
            stream: Box::pin(response.bytes_stream()),
            buffer: String::new(),
            pending_data: Vec::new(),
            finished: false,
            current_block_type: None,
            current_tool_id: None,
            current_tool_name: None,
            current_tool_input_json: String::new(),
        };

        let output = stream::unfold(state, |mut state| async move {
            if state.finished {
                return None;
            }

            loop {
                match take_chat_event(&mut state) {
                    Ok(Some(event)) => return Some((Ok(event), state)),
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
                            match parse_chat_stream_event(&mut state, &data) {
                                Ok(Some(event)) => return Some((Ok(event), state)),
                                Ok(None) => return None,
                                Err(err) => return Some((Err(err), state)),
                            }
                        }
                        return None;
                    }
                }
            }
        });

        Ok(Box::pin(output))
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

    #[test]
    fn parses_tool_use_from_response() {
        let response = MessageResponse {
            content: vec![
                ResponseContentBlock {
                    kind: "text".to_string(),
                    text: Some("Let me read the file.".to_string()),
                    id: None,
                    name: None,
                    input: None,
                },
                ResponseContentBlock {
                    kind: "tool_use".to_string(),
                    text: None,
                    id: Some("toolu_123".to_string()),
                    name: Some("Read".to_string()),
                    input: Some(serde_json::json!({"path": "src/main.rs"})),
                },
            ],
            usage: None,
            stop_reason: Some("tool_use".to_string()),
        };
        let chat = AnthropicBackend::parse_chat_response(response);
        assert_eq!(chat.stop_reason, StopReason::ToolUse);
        assert_eq!(chat.content.len(), 2);
        assert!(matches!(&chat.content[1], ContentBlock::ToolUse { name, .. } if name == "Read"));
    }
}
