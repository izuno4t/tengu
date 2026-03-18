// Agent module
// 自律的エージェント実行ループ

use anyhow::Result;
use futures_util::future::BoxFuture;
use std::sync::{Arc, Mutex};

use crate::llm::{
    ChatRequest, ChatStreamEvent, ContentBlock, LlmClient, LlmRequest, LlmResponse, LlmStream,
    LlmUsage, Message, StopReason,
};
use futures_util::StreamExt;
use crate::tools::{
    builtin_tool_definitions, ToolApprovalDecision, ToolApprovalRequest, ToolExecutor, ToolPolicy,
    ToolResult,
};

const MAX_AGENT_TURNS: usize = 50;
/// Maximum number of messages to send in a single request.
/// Older messages are trimmed to prevent context overflow.
const MAX_CONTEXT_MESSAGES: usize = 100;

#[allow(dead_code)]
pub struct Agent {
    pub name: String,
    pub description: String,
    pub prompt: String,
}

#[allow(dead_code)]
impl Agent {
    pub fn new(name: String) -> Self {
        Self {
            name,
            description: String::new(),
            prompt: String::new(),
        }
    }
}

/// Callback type for reporting tool executions to the UI.
pub type ToolEventHandler = Arc<
    dyn Fn(ToolEvent) -> BoxFuture<'static, ()> + Send + Sync,
>;

/// Events emitted during the agent loop for UI display.
#[derive(Debug, Clone)]
pub enum ToolEvent {
    /// LLM produced text output
    Text(String),
    /// A tool is about to be called
    ToolCall {
        name: String,
        input: serde_json::Value,
    },
    /// A tool completed
    ToolResult {
        name: String,
        result: String,
        is_error: bool,
    },
    /// LLM usage statistics for this turn
    Usage(LlmUsage),
}

pub struct AgentRunner {
    client: LlmClient,
    model_name: String,
    tool_policy: ToolPolicy,
    approval_handler: Mutex<Option<ApprovalHandler>>,
    tool_event_handler: Mutex<Option<ToolEventHandler>>,
    system_prompt: Mutex<Option<String>>,
    /// Persistent conversation history across agent loop invocations
    conversation_messages: Mutex<Vec<Message>>,
}

#[allow(dead_code)]
pub struct AgentOutput {
    pub response: LlmResponse,
    pub tool_result: Option<ToolResult>,
    pub messages: Vec<Message>,
}

#[allow(dead_code)]
pub struct AgentLoopResult {
    pub final_text: String,
    pub messages: Vec<Message>,
    pub total_turns: usize,
}

impl AgentRunner {
    pub fn new(client: LlmClient, model_name: String, tool_policy: ToolPolicy) -> Self {
        Self {
            client,
            model_name,
            tool_policy,
            approval_handler: Mutex::new(None),
            tool_event_handler: Mutex::new(None),
            system_prompt: Mutex::new(None),
            conversation_messages: Mutex::new(Vec::new()),
        }
    }

    pub fn set_approval_handler(&self, handler: ApprovalHandler) {
        if let Ok(mut guard) = self.approval_handler.lock() {
            *guard = Some(handler);
        }
    }

    pub fn set_tool_event_handler(&self, handler: ToolEventHandler) {
        if let Ok(mut guard) = self.tool_event_handler.lock() {
            *guard = Some(handler);
        }
    }

    pub fn set_system_prompt(&self, prompt: String) {
        if let Ok(mut guard) = self.system_prompt.lock() {
            *guard = Some(prompt);
        }
    }

    fn get_system_prompt(&self) -> Option<String> {
        self.system_prompt
            .lock()
            .ok()
            .and_then(|guard| guard.clone())
    }

    /// Get a copy of the persistent conversation messages.
    pub fn get_conversation_messages(&self) -> Vec<Message> {
        self.conversation_messages
            .lock()
            .ok()
            .map(|guard| guard.clone())
            .unwrap_or_default()
    }

    /// Replace the persistent conversation messages.
    fn save_conversation_messages(&self, messages: &[Message]) {
        if let Ok(mut guard) = self.conversation_messages.lock() {
            *guard = messages.to_vec();
        }
    }

    /// Clear the persistent conversation messages.
    #[allow(dead_code)]
    pub fn clear_conversation(&self) {
        if let Ok(mut guard) = self.conversation_messages.lock() {
            guard.clear();
        }
    }

    // -----------------------------------------------------------------------
    // Agentic loop (new): LLM → tool_use → result → LLM → ... until end_turn
    // -----------------------------------------------------------------------

    /// Run the full agentic loop with tool use support.
    /// Uses streaming for real-time text output.
    pub async fn run_agent_loop(
        &self,
        mut messages: Vec<Message>,
    ) -> Result<AgentLoopResult> {
        let tools = builtin_tool_definitions();
        let system = self.get_system_prompt();
        let executor = ToolExecutor::with_policy(self.tool_policy.clone());
        let mut total_turns = 0;

        loop {
            if total_turns >= MAX_AGENT_TURNS {
                break;
            }
            total_turns += 1;

            // Trim old messages to prevent context overflow
            let trimmed_messages = if messages.len() > MAX_CONTEXT_MESSAGES {
                let skip = messages.len() - MAX_CONTEXT_MESSAGES;
                messages[skip..].to_vec()
            } else {
                messages.clone()
            };

            let request = ChatRequest {
                system: system.clone(),
                messages: trimmed_messages,
                tools: tools.clone(),
                max_tokens: 16384,
            };

            // Retry on transient errors
            let mut stream = {
                let mut attempt = 0u32;
                loop {
                    match self.client.chat_stream(&self.model_name, &request).await {
                        Ok(s) => break s,
                        Err(e) => {
                            attempt += 1;
                            let err_str = e.to_string();
                            let is_retryable = err_str.contains("429")
                                || err_str.contains("529")
                                || err_str.contains("rate")
                                || err_str.contains("overloaded")
                                || err_str.contains("500")
                                || err_str.contains("502")
                                || err_str.contains("503");
                            if attempt >= 3 || !is_retryable {
                                return Err(e);
                            }
                            let delay = std::time::Duration::from_secs(2u64.pow(attempt));
                            tokio::time::sleep(delay).await;
                        }
                    }
                }
            };

            // Collect response from stream
            let mut response_content: Vec<ContentBlock> = Vec::new();
            let mut text_buffer = String::new();
            let mut stop_reason = StopReason::EndTurn;

            while let Some(event_result) = stream.next().await {
                match event_result? {
                    ChatStreamEvent::TextDelta(delta) => {
                        // Emit text incrementally for real-time display
                        self.emit_tool_event(ToolEvent::Text(delta.clone())).await;
                        text_buffer.push_str(&delta);
                    }
                    ChatStreamEvent::ToolUse { id, name, input } => {
                        // Flush accumulated text as a content block
                        if !text_buffer.is_empty() {
                            response_content.push(ContentBlock::Text {
                                text: std::mem::take(&mut text_buffer),
                            });
                        }
                        response_content.push(ContentBlock::ToolUse {
                            id,
                            name,
                            input,
                        });
                    }
                    ChatStreamEvent::Usage(usage) => {
                        self.emit_tool_event(ToolEvent::Usage(usage)).await;
                    }
                    ChatStreamEvent::Done(reason) => {
                        stop_reason = reason;
                    }
                }
            }

            // Flush remaining text
            if !text_buffer.is_empty() {
                response_content.push(ContentBlock::Text {
                    text: text_buffer,
                });
            }

            // Add assistant response to messages
            messages.push(Message {
                role: crate::llm::MessageRole::Assistant,
                content: response_content.clone(),
            });

            // If no tool_use, we're done
            if stop_reason != StopReason::ToolUse {
                break;
            }

            // Execute each tool call
            let tool_uses: Vec<(String, String, serde_json::Value)> = response_content
                .iter()
                .filter_map(|b| match b {
                    ContentBlock::ToolUse { id, name, input } => {
                        Some((id.clone(), name.clone(), input.clone()))
                    }
                    _ => None,
                })
                .collect();

            if tool_uses.is_empty() {
                break;
            }

            let mut tool_results = Vec::new();
            for (id, name, input) in &tool_uses {
                self.emit_tool_event(ToolEvent::ToolCall {
                    name: name.clone(),
                    input: input.clone(),
                })
                .await;

                let (result_text, is_error) = executor.execute_from_json(name, input);

                self.emit_tool_event(ToolEvent::ToolResult {
                    name: name.clone(),
                    result: result_text.clone(),
                    is_error,
                })
                .await;

                tool_results.push(ContentBlock::ToolResult {
                    tool_use_id: id.clone(),
                    content: result_text,
                    is_error,
                });
            }

            // Add tool results as user message
            messages.push(Message::tool_results(tool_results));
        }

        // Extract final text from the last assistant message
        let final_text = messages
            .iter()
            .rev()
            .find(|m| m.role == crate::llm::MessageRole::Assistant)
            .map(|m| m.text_content())
            .unwrap_or_default();

        // Persist messages for multi-turn conversation
        self.save_conversation_messages(&messages);

        Ok(AgentLoopResult {
            final_text,
            messages,
            total_turns,
        })
    }

    /// Convenience: start a new agent loop from a single user prompt.
    pub async fn run_prompt(&self, input: &str) -> Result<AgentLoopResult> {
        let messages = vec![Message::user_text(input)];
        self.run_agent_loop(messages).await
    }

    /// Convenience: continue an existing conversation with a new user message.
    #[allow(dead_code)]
    pub async fn continue_conversation(
        &self,
        mut messages: Vec<Message>,
        input: &str,
    ) -> Result<AgentLoopResult> {
        messages.push(Message::user_text(input));
        self.run_agent_loop(messages).await
    }

    async fn emit_tool_event(&self, event: ToolEvent) {
        let handler = self
            .tool_event_handler
            .lock()
            .ok()
            .and_then(|guard| guard.clone());
        if let Some(handler) = handler {
            handler(event).await;
        }
    }

    // -----------------------------------------------------------------------
    // Legacy API (kept for backward compatibility with TUI/CLI)
    // -----------------------------------------------------------------------

    pub async fn handle_prompt(&self, input: &str) -> Result<AgentOutput> {
        self.handle_prompt_with_context(input, "").await
    }

    #[allow(dead_code)]
    pub async fn handle_request_with_context(
        &self,
        request: LlmRequest,
        context: &str,
    ) -> Result<AgentOutput> {
        if request.images.is_empty() {
            return self
                .handle_prompt_with_context(&request.prompt, context)
                .await;
        }
        let final_prompt = if context.trim().is_empty() {
            request.prompt
        } else {
            format!(
                "Conversation context:\n{}\n\nUser request:\n{}",
                context, request.prompt
            )
        };
        let final_response = self
            .client
            .generate(
                &self.model_name,
                &LlmRequest {
                    prompt: final_prompt,
                    images: request.images,
                },
            )
            .await?;
        Ok(AgentOutput {
            response: LlmResponse {
                content: final_response.content.trim().to_string(),
                usage: final_response.usage,
            },
            tool_result: None,
            messages: Vec::new(),
        })
    }

    pub async fn handle_prompt_with_context(
        &self,
        input: &str,
        context: &str,
    ) -> Result<AgentOutput> {
        // Use the new agentic loop
        let mut messages = Vec::new();
        if !context.trim().is_empty() {
            // Add context as a prior user/assistant exchange
            messages.push(Message::user_text(format!(
                "Previous conversation context:\n{}",
                context
            )));
            messages.push(Message::assistant_text(
                "Understood. I have the context.",
            ));
        }
        messages.push(Message::user_text(input));

        let result = self.run_agent_loop(messages).await?;
        Ok(AgentOutput {
            response: LlmResponse {
                content: result.final_text,
                usage: None,
            },
            tool_result: None,
            messages: result.messages,
        })
    }

    #[allow(dead_code)]
    pub async fn handle_prompt_stream_with_context(
        &self,
        input: &str,
        context: &str,
    ) -> Result<LlmStream> {
        let (stream, _tool_result) = self
            .handle_prompt_stream_with_tool_context(input, context)
            .await?;
        Ok(stream)
    }

    pub async fn handle_request_stream_with_context(
        &self,
        request: LlmRequest,
        context: &str,
    ) -> Result<(LlmStream, Option<ToolResult>)> {
        if request.images.is_empty() {
            return self
                .handle_prompt_stream_with_tool_context(&request.prompt, context)
                .await;
        }
        let final_prompt = if context.trim().is_empty() {
            request.prompt
        } else {
            format!(
                "Conversation context:\n{}\n\nUser request:\n{}",
                context, request.prompt
            )
        };
        let stream = self
            .client
            .generate_stream(
                &self.model_name,
                &LlmRequest {
                    prompt: final_prompt,
                    images: request.images,
                },
            )
            .await?;
        Ok((stream, None))
    }

    pub async fn handle_prompt_stream_with_tool_context(
        &self,
        input: &str,
        context: &str,
    ) -> Result<(LlmStream, Option<ToolResult>)> {
        let final_prompt = if context.trim().is_empty() {
            input.to_string()
        } else {
            format!(
                "Conversation context:\n{}\n\nUser request:\n{}",
                context, input
            )
        };
        let stream = self
            .client
            .generate_stream(&self.model_name, &LlmRequest::text(final_prompt))
            .await?;
        Ok((stream, None))
    }

    pub async fn generate_plan_text_with_context(
        &self,
        input: &str,
        context: &str,
    ) -> Result<String> {
        let prompt = if context.trim().is_empty() {
            format!(
                "次の指示に対して、最小の計画を1-3項目で日本語の箇条書きで作成してください。\n\n指示:\n{}",
                input
            )
        } else {
            format!(
                "次の過去の会話を踏まえて、指示に対する最小の計画を1-3項目で日本語の箇条書きで作成してください。\n\n過去の会話:\n{}\n\n指示:\n{}",
                context, input
            )
        };
        let response = self
            .client
            .generate(&self.model_name, &LlmRequest::text(prompt))
            .await?;
        Ok(response.content)
    }
}

type ApprovalHandler =
    Arc<dyn Fn(ToolApprovalRequest) -> BoxFuture<'static, ToolApprovalDecision> + Send + Sync>;
