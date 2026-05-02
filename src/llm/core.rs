use anyhow::{anyhow, Result};
use futures_util::stream::BoxStream;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Arc;

// ---------------------------------------------------------------------------
// Provider enum
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LlmProvider {
    Anthropic,
    OpenAI,
    Google,
    Local,
}

impl LlmProvider {
    pub fn from_str(input: &str) -> Result<Self> {
        match input.trim().to_ascii_lowercase().as_str() {
            "anthropic" => Ok(Self::Anthropic),
            "openai" => Ok(Self::OpenAI),
            "google" | "gemini" => Ok(Self::Google),
            "local" | "ollama" | "lm-studio" | "lmstudio" => Ok(Self::Local),
            other => Err(anyhow!("unsupported provider: {}", other)),
        }
    }
}

// ---------------------------------------------------------------------------
// Message model for agentic loop (tool_use support)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MessageRole {
    User,
    Assistant,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentBlock {
    Text {
        text: String,
    },
    #[serde(alias = "thinking")]
    Thinking {
        thinking: String,
    },
    ToolUse {
        id: String,
        name: String,
        input: Value,
    },
    ToolResult {
        tool_use_id: String,
        content: String,
        #[serde(default)]
        is_error: bool,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: MessageRole,
    pub content: Vec<ContentBlock>,
}

impl Message {
    pub fn user_text(text: impl Into<String>) -> Self {
        Self {
            role: MessageRole::User,
            content: vec![ContentBlock::Text {
                text: text.into(),
            }],
        }
    }

    pub fn assistant_text(text: impl Into<String>) -> Self {
        Self {
            role: MessageRole::Assistant,
            content: vec![ContentBlock::Text {
                text: text.into(),
            }],
        }
    }

    pub fn tool_results(results: Vec<ContentBlock>) -> Self {
        Self {
            role: MessageRole::User,
            content: results,
        }
    }

    pub fn text_content(&self) -> String {
        self.content
            .iter()
            .filter_map(|block| match block {
                ContentBlock::Text { text } => Some(text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("")
    }

    pub fn tool_uses(&self) -> Vec<(&str, &str, &Value)> {
        self.content
            .iter()
            .filter_map(|block| match block {
                ContentBlock::ToolUse { id, name, input } => Some((id.as_str(), name.as_str(), input)),
                _ => None,
            })
            .collect()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct ChatRequest {
    pub system: Option<String>,
    pub messages: Vec<Message>,
    pub tools: Vec<ToolDefinition>,
    pub max_tokens: u32,
}

impl ChatRequest {
    #[allow(dead_code)]
    pub fn new(messages: Vec<Message>, tools: Vec<ToolDefinition>) -> Self {
        Self {
            system: None,
            messages,
            tools,
            max_tokens: 8192,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopReason {
    EndTurn,
    ToolUse,
    MaxTokens,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct ChatResponse {
    pub content: Vec<ContentBlock>,
    pub stop_reason: StopReason,
    pub usage: Option<LlmUsage>,
}

impl ChatResponse {
    pub fn text_content(&self) -> String {
        self.content
            .iter()
            .filter_map(|block| match block {
                ContentBlock::Text { text } => Some(text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("")
    }

    pub fn tool_uses(&self) -> Vec<(&str, &str, &Value)> {
        self.content
            .iter()
            .filter_map(|block| match block {
                ContentBlock::ToolUse { id, name, input } => {
                    Some((id.as_str(), name.as_str(), input))
                }
                _ => None,
            })
            .collect()
    }
}

// ---------------------------------------------------------------------------
// Legacy types (kept for backward compatibility)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct LlmResponse {
    pub content: String,
    pub usage: Option<LlmUsage>,
}

#[derive(Debug, Clone)]
pub struct LlmUsage {
    pub provider: String,
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub total_tokens: Option<u64>,
    pub cache_creation_input_tokens: Option<u64>,
    pub cache_read_input_tokens: Option<u64>,
    pub reasoning_tokens: Option<u64>,
    pub raw: Option<Value>,
}

#[derive(Debug, Clone)]
pub enum LlmStreamEvent {
    Text(String),
    Usage(LlmUsage),
}

#[derive(Debug, Clone)]
pub struct LlmImage {
    pub media_type: String,
    pub data_base64: String,
}

#[derive(Debug, Clone)]
pub struct LlmRequest {
    pub prompt: String,
    pub images: Vec<LlmImage>,
}

impl LlmRequest {
    pub fn text(prompt: impl Into<String>) -> Self {
        Self {
            prompt: prompt.into(),
            images: Vec::new(),
        }
    }
}

pub type LlmStream = BoxStream<'static, Result<LlmStreamEvent>>;

/// Events emitted during a streaming chat response.
#[derive(Debug, Clone)]
pub enum ChatStreamEvent {
    /// Incremental text token
    TextDelta(String),
    /// Incremental thinking token (extended thinking)
    ThinkingDelta(String),
    /// A tool_use block has been fully received
    ToolUse {
        id: String,
        name: String,
        input: Value,
    },
    /// Usage information
    Usage(LlmUsage),
    /// Stop reason (final event)
    Done(StopReason),
}

pub type ChatStream = BoxStream<'static, Result<ChatStreamEvent>>;

// ---------------------------------------------------------------------------
// LLM Client & Backend trait
// ---------------------------------------------------------------------------

pub struct LlmClient {
    backend: Arc<dyn LlmBackend + Send + Sync>,
}

impl Clone for LlmClient {
    fn clone(&self) -> Self {
        Self {
            backend: Arc::clone(&self.backend),
        }
    }
}

impl LlmClient {
    pub fn new(backend: Box<dyn LlmBackend + Send + Sync>) -> Self {
        Self {
            backend: Arc::from(backend),
        }
    }

    #[allow(dead_code)]
    pub fn provider(&self) -> LlmProvider {
        self.backend.provider()
    }

    pub async fn generate(&self, model: &str, request: &LlmRequest) -> Result<LlmResponse> {
        self.backend.generate(model, request).await
    }

    pub async fn generate_stream(&self, model: &str, request: &LlmRequest) -> Result<LlmStream> {
        self.backend.generate_stream(model, request).await
    }

    pub async fn chat(&self, model: &str, request: &ChatRequest) -> Result<ChatResponse> {
        self.backend.chat(model, request).await
    }

    pub async fn chat_stream(&self, model: &str, request: &ChatRequest) -> Result<ChatStream> {
        self.backend.chat_stream(model, request).await
    }
}

#[async_trait::async_trait]
pub trait LlmBackend {
    #[allow(dead_code)]
    fn provider(&self) -> LlmProvider;
    async fn generate(&self, model: &str, request: &LlmRequest) -> Result<LlmResponse>;
    async fn generate_stream(&self, model: &str, request: &LlmRequest) -> Result<LlmStream>;
    async fn chat(&self, model: &str, request: &ChatRequest) -> Result<ChatResponse>;

    /// Streaming chat with tool_use support. Default falls back to non-streaming chat.
    async fn chat_stream(&self, model: &str, request: &ChatRequest) -> Result<ChatStream> {
        let response = self.chat(model, request).await?;
        let mut events: Vec<Result<ChatStreamEvent>> = Vec::new();
        if let Some(usage) = response.usage {
            events.push(Ok(ChatStreamEvent::Usage(usage)));
        }
        for block in &response.content {
            match block {
                ContentBlock::Text { text } => {
                    events.push(Ok(ChatStreamEvent::TextDelta(text.clone())));
                }
                ContentBlock::ToolUse { id, name, input } => {
                    events.push(Ok(ChatStreamEvent::ToolUse {
                        id: id.clone(),
                        name: name.clone(),
                        input: input.clone(),
                    }));
                }
                _ => {}
            }
        }
        events.push(Ok(ChatStreamEvent::Done(response.stop_reason)));
        Ok(Box::pin(futures_util::stream::iter(events)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ═══════════════════════════════════════════════════════════════════════
    // Message Model Tests
    // ═══════════════════════════════════════════════════════════════════════

    #[test]
    fn message_user_text_creates_correct_role() {
        let msg = Message::user_text("Hello");
        assert_eq!(msg.role, MessageRole::User);
        assert_eq!(msg.content.len(), 1);
        assert_eq!(msg.text_content(), "Hello");
    }

    #[test]
    fn message_assistant_text_creates_correct_role() {
        let msg = Message::assistant_text("Response");
        assert_eq!(msg.role, MessageRole::Assistant);
        assert_eq!(msg.text_content(), "Response");
    }

    #[test]
    fn message_tool_results_has_user_role() {
        let results = vec![ContentBlock::ToolResult {
            tool_use_id: "id1".to_string(),
            content: "result".to_string(),
            is_error: false,
        }];
        let msg = Message::tool_results(results);
        assert_eq!(msg.role, MessageRole::User);
        assert_eq!(msg.content.len(), 1);
    }

    #[test]
    fn message_text_content_joins_multiple_blocks() {
        let msg = Message {
            role: MessageRole::Assistant,
            content: vec![
                ContentBlock::Text {
                    text: "Hello ".to_string(),
                },
                ContentBlock::Text {
                    text: "World".to_string(),
                },
            ],
        };
        assert_eq!(msg.text_content(), "Hello World");
    }

    #[test]
    fn message_text_content_ignores_non_text_blocks() {
        let msg = Message {
            role: MessageRole::Assistant,
            content: vec![
                ContentBlock::Text {
                    text: "text".to_string(),
                },
                ContentBlock::ToolUse {
                    id: "id1".to_string(),
                    name: "Read".to_string(),
                    input: Value::Object(Default::default()),
                },
            ],
        };
        assert_eq!(msg.text_content(), "text");
    }

    #[test]
    fn message_tool_uses_extracts_tool_use_blocks() {
        let msg = Message {
            role: MessageRole::Assistant,
            content: vec![
                ContentBlock::Text {
                    text: "Let me read".to_string(),
                },
                ContentBlock::ToolUse {
                    id: "id1".to_string(),
                    name: "Read".to_string(),
                    input: serde_json::json!({"path": "test.rs"}),
                },
                ContentBlock::ToolUse {
                    id: "id2".to_string(),
                    name: "Bash".to_string(),
                    input: serde_json::json!({"command": "ls"}),
                },
            ],
        };
        let uses = msg.tool_uses();
        assert_eq!(uses.len(), 2);
        assert_eq!(uses[0].1, "Read");
        assert_eq!(uses[1].1, "Bash");
    }

    #[test]
    fn message_tool_uses_empty_when_no_tool_use() {
        let msg = Message::assistant_text("Just text");
        assert!(msg.tool_uses().is_empty());
    }

    // ═══════════════════════════════════════════════════════════════════════
    // ContentBlock Serialization Tests
    // ═══════════════════════════════════════════════════════════════════════

    #[test]
    fn content_block_text_serializes_correctly() {
        let block = ContentBlock::Text {
            text: "hello".to_string(),
        };
        let json = serde_json::to_value(&block).unwrap();
        assert_eq!(json["type"], "text");
        assert_eq!(json["text"], "hello");
    }

    #[test]
    fn content_block_tool_use_serializes_correctly() {
        let block = ContentBlock::ToolUse {
            id: "tool_1".to_string(),
            name: "Read".to_string(),
            input: serde_json::json!({"path": "file.rs"}),
        };
        let json = serde_json::to_value(&block).unwrap();
        assert_eq!(json["type"], "tool_use");
        assert_eq!(json["id"], "tool_1");
        assert_eq!(json["name"], "Read");
    }

    #[test]
    fn content_block_tool_result_serializes_correctly() {
        let block = ContentBlock::ToolResult {
            tool_use_id: "tool_1".to_string(),
            content: "file contents".to_string(),
            is_error: false,
        };
        let json = serde_json::to_value(&block).unwrap();
        assert_eq!(json["type"], "tool_result");
        assert_eq!(json["tool_use_id"], "tool_1");
        assert_eq!(json["is_error"], false);
    }

    #[test]
    fn content_block_roundtrip_deserialization() {
        let original = ContentBlock::ToolUse {
            id: "abc".to_string(),
            name: "Bash".to_string(),
            input: serde_json::json!({"command": "ls"}),
        };
        let json_str = serde_json::to_string(&original).unwrap();
        let deserialized: ContentBlock = serde_json::from_str(&json_str).unwrap();
        match deserialized {
            ContentBlock::ToolUse { id, name, input } => {
                assert_eq!(id, "abc");
                assert_eq!(name, "Bash");
                assert_eq!(input["command"], "ls");
            }
            _ => panic!("Expected ToolUse"),
        }
    }

    // ═══════════════════════════════════════════════════════════════════════
    // ChatResponse Tests
    // ═══════════════════════════════════════════════════════════════════════

    #[test]
    fn chat_response_text_content() {
        let response = ChatResponse {
            content: vec![
                ContentBlock::Text {
                    text: "Hello".to_string(),
                },
                ContentBlock::ToolUse {
                    id: "id".to_string(),
                    name: "Read".to_string(),
                    input: Value::Object(Default::default()),
                },
            ],
            stop_reason: StopReason::ToolUse,
            usage: None,
        };
        assert_eq!(response.text_content(), "Hello");
    }

    #[test]
    fn chat_response_tool_uses() {
        let response = ChatResponse {
            content: vec![ContentBlock::ToolUse {
                id: "id1".to_string(),
                name: "Grep".to_string(),
                input: serde_json::json!({"pattern": "test"}),
            }],
            stop_reason: StopReason::ToolUse,
            usage: None,
        };
        let uses = response.tool_uses();
        assert_eq!(uses.len(), 1);
        assert_eq!(uses[0].1, "Grep");
    }

    // ═══════════════════════════════════════════════════════════════════════
    // LlmProvider Tests
    // ═══════════════════════════════════════════════════════════════════════

    #[test]
    fn provider_from_str_valid() {
        assert_eq!(LlmProvider::from_str("anthropic").unwrap(), LlmProvider::Anthropic);
        assert_eq!(LlmProvider::from_str("openai").unwrap(), LlmProvider::OpenAI);
        assert_eq!(LlmProvider::from_str("google").unwrap(), LlmProvider::Google);
        assert_eq!(LlmProvider::from_str("gemini").unwrap(), LlmProvider::Google);
        assert_eq!(LlmProvider::from_str("ollama").unwrap(), LlmProvider::Local);
        assert_eq!(LlmProvider::from_str("local").unwrap(), LlmProvider::Local);
    }

    #[test]
    fn provider_from_str_case_insensitive() {
        assert_eq!(LlmProvider::from_str("ANTHROPIC").unwrap(), LlmProvider::Anthropic);
        assert_eq!(LlmProvider::from_str("OpenAI").unwrap(), LlmProvider::OpenAI);
    }

    #[test]
    fn provider_from_str_invalid() {
        assert!(LlmProvider::from_str("unknown_provider").is_err());
    }

    // ═══════════════════════════════════════════════════════════════════════
    // StopReason / ChatRequest Tests
    // ═══════════════════════════════════════════════════════════════════════

    #[test]
    fn stop_reason_equality() {
        assert_eq!(StopReason::EndTurn, StopReason::EndTurn);
        assert_ne!(StopReason::EndTurn, StopReason::ToolUse);
        assert_ne!(StopReason::ToolUse, StopReason::MaxTokens);
    }

    #[test]
    fn chat_request_new_defaults() {
        let req = ChatRequest::new(vec![], vec![]);
        assert!(req.system.is_none());
        assert_eq!(req.max_tokens, 8192);
        assert!(req.messages.is_empty());
        assert!(req.tools.is_empty());
    }

    // ═══════════════════════════════════════════════════════════════════════
    // LlmClient Clone Tests
    // ═══════════════════════════════════════════════════════════════════════

    #[test]
    fn llm_client_clone_shares_backend() {
        // Verify that LlmClient can be cloned (compile-time check).
        // We cannot easily construct one without a full backend in unit tests,
        // but the Clone impl existing is verified by the compiler.
        fn _assert_clone<T: Clone>() {}
        _assert_clone::<LlmClient>();
    }
}
