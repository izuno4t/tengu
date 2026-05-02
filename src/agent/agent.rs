// Agent module
// 自律的エージェント実行ループ

use anyhow::Result;
use futures_util::future::BoxFuture;
use std::sync::{Arc, Mutex};

use crate::llm::{
    ChatRequest, ChatStreamEvent, ContentBlock, LlmClient, LlmRequest, LlmResponse, LlmStream,
    LlmUsage, Message, StopReason, ToolDefinition,
};
use crate::mcp::{mcp_result_to_text, McpServerConfig};
use crate::tools::{
    builtin_tool_definitions, estimate_tokens, ToolApprovalDecision, ToolApprovalRequest,
    ToolExecutor, ToolPolicy, ToolResult,
};
use futures_util::StreamExt;

const MAX_AGENT_TURNS: usize = 50;
/// Maximum number of turns for sub-agents.
const MAX_SUB_AGENT_TURNS: usize = 20;
/// Maximum number of messages to send in a single request.
/// Older messages are trimmed to prevent context overflow.
const MAX_CONTEXT_MESSAGES: usize = 100;
/// Number of recent messages to preserve during compaction.
const COMPACT_PRESERVE_MESSAGES: usize = 30;
/// Force compaction when message count exceeds this threshold.
const FORCE_COMPACT_THRESHOLD: usize = 300;

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
pub type ToolEventHandler = Arc<dyn Fn(ToolEvent) -> BoxFuture<'static, ()> + Send + Sync>;

/// Events emitted during the agent loop for UI display.
#[derive(Debug, Clone)]
pub enum ToolEvent {
    /// LLM produced text output
    Text(String),
    /// LLM extended thinking output
    Thinking(String),
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
    /// MCP server configurations for external tool integration
    mcp_servers: Mutex<Vec<(String, McpServerConfig)>>,
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
    pub estimated_tokens: usize,
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
            mcp_servers: Mutex::new(Vec::new()),
        }
    }

    /// Register MCP servers for tool integration.
    #[allow(dead_code)]
    pub fn set_mcp_servers(&self, servers: Vec<(String, McpServerConfig)>) {
        if let Ok(mut guard) = self.mcp_servers.lock() {
            *guard = servers;
        }
    }

    /// Get MCP tool definitions for inclusion in LLM requests.
    fn get_mcp_tool_definitions(&self) -> Vec<ToolDefinition> {
        let servers = self.mcp_servers.lock().ok();
        let servers = match servers {
            Some(guard) => guard.clone(),
            None => return Vec::new(),
        };
        let mut defs = Vec::new();
        for (name, server) in &servers {
            if server.url.is_some() {
                // HTTP servers: try async list (skip if can't list)
                // We can't call async from here, so skip dynamic listing
                // MCP tools should be pre-loaded
            } else if server.command.is_some() {
                if let Ok(tools) = crate::mcp::list_tools_stdio(server) {
                    for tool in tools {
                        defs.push(tool.to_tool_definition(name));
                    }
                }
            }
        }
        defs
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

    /// Compact messages by dropping older messages while preserving recent context.
    /// Keeps the last COMPACT_PRESERVE_MESSAGES messages.
    /// Returns the compacted message list.
    fn compact_messages(messages: &[Message]) -> Vec<Message> {
        if messages.len() <= COMPACT_PRESERVE_MESSAGES {
            return messages.to_vec();
        }
        let skip = messages.len() - COMPACT_PRESERVE_MESSAGES;
        let mut compacted = messages[skip..].to_vec();
        // Ensure we don't start with an orphaned tool_result
        while !compacted.is_empty() {
            let first = &compacted[0];
            let is_orphaned_tool_result = first
                .content
                .iter()
                .any(|b| matches!(b, ContentBlock::ToolResult { .. }))
                && first.role == crate::llm::MessageRole::User;
            if is_orphaned_tool_result {
                compacted.remove(0);
            } else {
                break;
            }
        }
        // Ensure conversation starts with a user message
        while !compacted.is_empty() && compacted[0].role != crate::llm::MessageRole::User {
            compacted.remove(0);
        }
        // Safety: never return empty
        if compacted.is_empty() && !messages.is_empty() {
            return vec![messages.last().unwrap().clone()];
        }
        compacted
    }

    // -----------------------------------------------------------------------
    // Agentic loop (new): LLM -> tool_use -> result -> LLM -> ... until end_turn
    // -----------------------------------------------------------------------

    /// Run the full agentic loop with tool use support.
    /// Uses streaming for real-time text output.
    pub async fn run_agent_loop(&self, messages: Vec<Message>) -> Result<AgentLoopResult> {
        self.run_agent_loop_with_max_turns(messages, MAX_AGENT_TURNS)
            .await
    }

    /// Run the agentic loop with a custom maximum number of turns.
    pub fn run_agent_loop_with_max_turns(
        &self,
        mut messages: Vec<Message>,
        max_turns: usize,
    ) -> futures_util::future::BoxFuture<'_, Result<AgentLoopResult>> {
        Box::pin(async move {
            let mut tools = builtin_tool_definitions();
            // Merge MCP tool definitions
            tools.extend(self.get_mcp_tool_definitions());
            let system = self.get_system_prompt();
            let executor = ToolExecutor::with_policy(self.tool_policy.clone());
            let mut total_turns = 0;

            loop {
                if total_turns >= max_turns {
                    break;
                }
                total_turns += 1;

                // Compact messages to prevent context overflow
                let trimmed_messages = if messages.len() > FORCE_COMPACT_THRESHOLD
                    || messages.len() > MAX_CONTEXT_MESSAGES
                {
                    Self::compact_messages(&messages)
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
                let mut thinking_buffer = String::new();
                let mut stop_reason = StopReason::EndTurn;

                while let Some(event_result) = stream.next().await {
                    match event_result? {
                        ChatStreamEvent::TextDelta(delta) => {
                            // Emit text incrementally for real-time display
                            self.emit_tool_event(ToolEvent::Text(delta.clone())).await;
                            text_buffer.push_str(&delta);
                        }
                        ChatStreamEvent::ThinkingDelta(delta) => {
                            // Accumulate thinking content (extended thinking)
                            self.emit_tool_event(ToolEvent::Thinking(delta.clone()))
                                .await;
                            thinking_buffer.push_str(&delta);
                        }
                        ChatStreamEvent::ToolUse { id, name, input } => {
                            // Flush accumulated thinking
                            if !thinking_buffer.is_empty() {
                                response_content.push(ContentBlock::Thinking {
                                    thinking: std::mem::take(&mut thinking_buffer),
                                });
                            }
                            // Flush accumulated text as a content block
                            if !text_buffer.is_empty() {
                                response_content.push(ContentBlock::Text {
                                    text: std::mem::take(&mut text_buffer),
                                });
                            }
                            response_content.push(ContentBlock::ToolUse { id, name, input });
                        }
                        ChatStreamEvent::Usage(usage) => {
                            self.emit_tool_event(ToolEvent::Usage(usage)).await;
                        }
                        ChatStreamEvent::Done(reason) => {
                            stop_reason = reason;
                        }
                    }
                }

                // Flush remaining thinking
                if !thinking_buffer.is_empty() {
                    response_content.push(ContentBlock::Thinking {
                        thinking: thinking_buffer,
                    });
                }
                // Flush remaining text
                if !text_buffer.is_empty() {
                    response_content.push(ContentBlock::Text { text: text_buffer });
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

                    let (result_text, is_error) = if name.starts_with("mcp__") {
                        self.execute_mcp_tool(name, input).await
                    } else {
                        match name.as_str() {
                            "SubAgent" => self.execute_sub_agent(input).await,
                            "ParallelAgents" => self.execute_parallel_agents(input).await,
                            _ => executor.execute_from_json(name, input),
                        }
                    };

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

            // Estimate token usage from messages
            let estimated_tokens = messages
                .iter()
                .map(|m| {
                    m.content
                        .iter()
                        .map(|b| match b {
                            ContentBlock::Text { text } => estimate_tokens(text),
                            ContentBlock::Thinking { thinking } => estimate_tokens(thinking),
                            ContentBlock::ToolUse { input, .. } => {
                                estimate_tokens(&input.to_string())
                            }
                            ContentBlock::ToolResult { content, .. } => estimate_tokens(content),
                        })
                        .sum::<usize>()
                })
                .sum();

            // Persist messages for multi-turn conversation
            self.save_conversation_messages(&messages);

            Ok(AgentLoopResult {
                final_text,
                messages,
                total_turns,
                estimated_tokens,
            })
        }) // end Box::pin
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

    // -----------------------------------------------------------------------
    // MCP tool execution
    // -----------------------------------------------------------------------

    /// Execute an MCP tool call by parsing the tool name and dispatching to the right server.
    async fn execute_mcp_tool(&self, tool_name: &str, input: &serde_json::Value) -> (String, bool) {
        // Tool name format: mcp__{server_name}_{tool_name}
        let rest = match tool_name.strip_prefix("mcp__") {
            Some(r) => r,
            None => return (format!("Error: invalid MCP tool name: {}", tool_name), true),
        };

        // Find the first underscore after server name
        let (server_name, mcp_tool_name) = match rest.find('_') {
            Some(idx) => (&rest[..idx], &rest[idx + 1..]),
            None => {
                return (
                    format!("Error: invalid MCP tool name format: {}", tool_name),
                    true,
                )
            }
        };

        let server_config = {
            let servers = self.mcp_servers.lock().ok();
            servers.as_ref().and_then(|guard| {
                guard
                    .iter()
                    .find(|(name, _)| name == server_name)
                    .map(|(_, config)| config.clone())
            })
        };

        let server = match server_config {
            Some(s) => s,
            None => {
                return (
                    format!("Error: MCP server '{}' not found", server_name),
                    true,
                )
            }
        };

        if server.url.is_some() {
            // HTTP transport
            match crate::mcp::call_tool_http(&server, mcp_tool_name, input).await {
                Ok(result) => (mcp_result_to_text(&result), false),
                Err(e) => (format!("MCP error: {}", e), true),
            }
        } else if server.command.is_some() {
            // Stdio transport (blocking)
            let server_clone = server.clone();
            let tool = mcp_tool_name.to_string();
            let args = input.clone();
            match tokio::task::spawn_blocking(move || {
                crate::mcp::call_tool_stdio(&server_clone, &tool, &args)
            })
            .await
            {
                Ok(Ok(result)) => (mcp_result_to_text(&result), false),
                Ok(Err(e)) => (format!("MCP error: {}", e), true),
                Err(e) => (format!("MCP task error: {}", e), true),
            }
        } else {
            (
                format!(
                    "Error: MCP server '{}' has no transport configured",
                    server_name
                ),
                true,
            )
        }
    }

    // -----------------------------------------------------------------------
    // SubAgent / ParallelAgents execution
    // -----------------------------------------------------------------------

    /// Execute a SubAgent tool call by launching a new agent loop.
    async fn execute_sub_agent(&self, input: &serde_json::Value) -> (String, bool) {
        let prompt = input
            .get("prompt")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("")
            .to_string();
        let max_turns = input
            .get("max_turns")
            .and_then(serde_json::Value::as_u64)
            .map(|v| (v as usize).min(MAX_SUB_AGENT_TURNS))
            .unwrap_or(MAX_SUB_AGENT_TURNS);

        if prompt.is_empty() {
            return (
                "Error: SubAgent requires a non-empty prompt".to_string(),
                true,
            );
        }

        let sub_runner = AgentRunner::new(
            self.client.clone(),
            self.model_name.clone(),
            ToolPolicy::read_only(),
        );
        if let Some(sys) = self.get_system_prompt() {
            sub_runner.set_system_prompt(sys);
        }

        let messages = vec![Message::user_text(&prompt)];
        match sub_runner
            .run_agent_loop_with_max_turns(messages, max_turns)
            .await
        {
            Ok(result) => (result.final_text, false),
            Err(e) => (format!("SubAgent error: {}", e), true),
        }
    }

    /// Execute a ParallelAgents tool call by launching multiple sub-agents concurrently.
    async fn execute_parallel_agents(&self, input: &serde_json::Value) -> (String, bool) {
        let tasks: Vec<String> = input
            .get("tasks")
            .and_then(serde_json::Value::as_array)
            .map(|arr| {
                arr.iter()
                    .filter_map(serde_json::Value::as_str)
                    .map(String::from)
                    .collect()
            })
            .unwrap_or_default();

        if tasks.len() < 2 {
            return (
                "Error: ParallelAgents requires at least 2 tasks".to_string(),
                true,
            );
        }
        if tasks.len() > 6 {
            return (
                "Error: ParallelAgents supports at most 6 tasks".to_string(),
                true,
            );
        }

        let mut handles = Vec::new();
        for (i, task) in tasks.into_iter().enumerate() {
            let client = self.client.clone();
            let model_name = self.model_name.clone();
            let system_prompt = self.get_system_prompt();

            let handle = tokio::spawn(async move {
                let sub_runner = AgentRunner::new(client, model_name, ToolPolicy::read_only());
                if let Some(sys) = system_prompt {
                    sub_runner.set_system_prompt(sys);
                }
                let messages = vec![Message::user_text(&task)];
                match sub_runner
                    .run_agent_loop_with_max_turns(messages, MAX_SUB_AGENT_TURNS)
                    .await
                {
                    Ok(result) => (i, result.final_text, false),
                    Err(e) => (i, format!("Error: {}", e), true),
                }
            });
            handles.push(handle);
        }

        let mut results: Vec<(usize, String, bool)> = Vec::new();
        let mut any_error = false;
        for handle in handles {
            match handle.await {
                Ok((idx, text, is_error)) => {
                    if is_error {
                        any_error = true;
                    }
                    results.push((idx, text, is_error));
                }
                Err(e) => {
                    any_error = true;
                    results.push((results.len(), format!("Task join error: {}", e), true));
                }
            }
        }
        results.sort_by_key(|(idx, _, _)| *idx);

        let combined: Vec<String> = results
            .iter()
            .enumerate()
            .map(|(i, (_, text, is_error))| {
                let status = if *is_error { " [ERROR]" } else { "" };
                format!("--- Task {} result{} ---\n{}", i + 1, status, text)
            })
            .collect();

        (combined.join("\n\n"), any_error)
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
            messages.push(Message::assistant_text("Understood. I have the context."));
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::{
        ChatRequest, ChatResponse, ContentBlock, LlmBackend, LlmClient, LlmProvider, LlmRequest,
        LlmResponse, LlmStream, LlmUsage, StopReason,
    };
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// Mock backend that returns a fixed text response.
    struct MockBackend {
        response_text: String,
        call_count: AtomicUsize,
    }

    impl MockBackend {
        fn new(text: &str) -> Self {
            Self {
                response_text: text.to_string(),
                call_count: AtomicUsize::new(0),
            }
        }
    }

    #[async_trait::async_trait]
    impl LlmBackend for MockBackend {
        fn provider(&self) -> LlmProvider {
            LlmProvider::Anthropic
        }

        async fn generate(&self, _model: &str, _request: &LlmRequest) -> Result<LlmResponse> {
            Ok(LlmResponse {
                content: self.response_text.clone(),
                usage: None,
            })
        }

        async fn generate_stream(&self, _model: &str, _request: &LlmRequest) -> Result<LlmStream> {
            let text = self.response_text.clone();
            let events = vec![Ok(crate::llm::LlmStreamEvent::Text(text))];
            Ok(Box::pin(futures_util::stream::iter(events)))
        }

        async fn chat(&self, _model: &str, _request: &ChatRequest) -> Result<ChatResponse> {
            self.call_count.fetch_add(1, Ordering::Relaxed);
            Ok(ChatResponse {
                content: vec![ContentBlock::Text {
                    text: self.response_text.clone(),
                }],
                stop_reason: StopReason::EndTurn,
                usage: Some(LlmUsage {
                    provider: "mock".to_string(),
                    input_tokens: Some(10),
                    output_tokens: Some(5),
                    total_tokens: Some(15),
                    cache_creation_input_tokens: None,
                    cache_read_input_tokens: None,
                    reasoning_tokens: None,
                    raw: None,
                }),
            })
        }
    }

    /// Mock backend that returns a tool_use followed by end_turn.
    struct MockToolBackend {
        call_count: AtomicUsize,
    }

    #[async_trait::async_trait]
    impl LlmBackend for MockToolBackend {
        fn provider(&self) -> LlmProvider {
            LlmProvider::Anthropic
        }

        async fn generate(&self, _model: &str, _request: &LlmRequest) -> Result<LlmResponse> {
            Ok(LlmResponse {
                content: String::new(),
                usage: None,
            })
        }

        async fn generate_stream(&self, _model: &str, _request: &LlmRequest) -> Result<LlmStream> {
            Ok(Box::pin(futures_util::stream::iter(vec![])))
        }

        async fn chat(&self, _model: &str, _request: &ChatRequest) -> Result<ChatResponse> {
            let count = self.call_count.fetch_add(1, Ordering::Relaxed);
            if count == 0 {
                // First call: return tool_use for Read
                Ok(ChatResponse {
                    content: vec![ContentBlock::ToolUse {
                        id: "tool-1".to_string(),
                        name: "Read".to_string(),
                        input: serde_json::json!({"file_path": "/nonexistent-test-file.txt"}),
                    }],
                    stop_reason: StopReason::ToolUse,
                    usage: None,
                })
            } else {
                // Second call: return text
                Ok(ChatResponse {
                    content: vec![ContentBlock::Text {
                        text: "File not found, as expected.".to_string(),
                    }],
                    stop_reason: StopReason::EndTurn,
                    usage: None,
                })
            }
        }
    }

    fn make_runner(backend: impl LlmBackend + Send + Sync + 'static) -> AgentRunner {
        let client = LlmClient::new(Box::new(backend));
        AgentRunner::new(client, "test-model".to_string(), ToolPolicy::default())
    }

    #[tokio::test]
    async fn agent_runner_simple_text_response() {
        let runner = make_runner(MockBackend::new("Hello from mock!"));
        let result = runner.run_prompt("hi").await.unwrap();
        assert_eq!(result.final_text, "Hello from mock!");
        assert_eq!(result.total_turns, 1);
    }

    #[tokio::test]
    async fn agent_runner_preserves_conversation() {
        let runner = make_runner(MockBackend::new("Response 1"));
        runner.run_prompt("first").await.unwrap();

        let messages = runner.get_conversation_messages();
        assert!(messages.len() >= 2); // user + assistant
        assert_eq!(messages[0].text_content(), "first");
    }

    #[tokio::test]
    async fn agent_runner_clear_conversation() {
        let runner = make_runner(MockBackend::new("test"));
        runner.run_prompt("hello").await.unwrap();
        assert!(!runner.get_conversation_messages().is_empty());

        runner.clear_conversation();
        assert!(runner.get_conversation_messages().is_empty());
    }

    #[tokio::test]
    async fn agent_runner_system_prompt() {
        let runner = make_runner(MockBackend::new("ok"));
        assert!(runner.get_system_prompt().is_none());

        runner.set_system_prompt("You are a test agent.".to_string());
        assert_eq!(runner.get_system_prompt().unwrap(), "You are a test agent.");
    }

    #[tokio::test]
    async fn agent_runner_tool_use_loop() {
        let runner = make_runner(MockToolBackend {
            call_count: AtomicUsize::new(0),
        });
        let result = runner.run_prompt("read a file").await.unwrap();
        // Should have made 2 turns: tool_use + end_turn
        assert_eq!(result.total_turns, 2);
        assert_eq!(result.final_text, "File not found, as expected.");
    }

    #[tokio::test]
    async fn agent_runner_collects_tool_events() {
        let events: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let events_clone = events.clone();

        let runner = make_runner(MockBackend::new("hi"));
        runner.set_tool_event_handler(Arc::new(move |event| {
            let events = events_clone.clone();
            Box::pin(async move {
                let label = match &event {
                    ToolEvent::Text(_) => "text",
                    ToolEvent::Thinking(_) => "thinking",
                    ToolEvent::ToolCall { .. } => "tool_call",
                    ToolEvent::ToolResult { .. } => "tool_result",
                    ToolEvent::Usage(_) => "usage",
                };
                events.lock().unwrap().push(label.to_string());
            })
        }));

        runner.run_prompt("hello").await.unwrap();

        let collected = events.lock().unwrap();
        assert!(collected.contains(&"text".to_string()));
        assert!(collected.contains(&"usage".to_string()));
    }

    #[tokio::test]
    async fn agent_runner_handle_prompt_uses_agent_loop() {
        let runner = make_runner(MockBackend::new("agent loop response"));
        let output = runner.handle_prompt("test").await.unwrap();
        assert_eq!(output.response.content, "agent loop response");
    }

    #[tokio::test]
    async fn agent_runner_with_context() {
        let runner = make_runner(MockBackend::new("contextual response"));
        let output = runner
            .handle_prompt_with_context("question", "prior context")
            .await
            .unwrap();
        assert_eq!(output.response.content, "contextual response");
        // Should have context message + acknowledgment + user message = 3 input messages
        // + 1 assistant response
        assert!(output.messages.len() >= 4);
    }

    #[test]
    fn tool_event_debug() {
        let event = ToolEvent::Text("hello".to_string());
        let debug = format!("{:?}", event);
        assert!(debug.contains("Text"));
    }

    #[test]
    fn agent_loop_result_fields() {
        let result = AgentLoopResult {
            final_text: "done".to_string(),
            messages: vec![],
            total_turns: 3,
            estimated_tokens: 0,
        };
        assert_eq!(result.final_text, "done");
        assert_eq!(result.total_turns, 3);
        assert!(result.messages.is_empty());
    }

    #[test]
    fn compact_messages_no_op_when_small() {
        let messages = vec![Message::user_text("hello"), Message::assistant_text("hi")];
        let compacted = AgentRunner::compact_messages(&messages);
        assert_eq!(compacted.len(), 2);
    }

    #[test]
    fn compact_messages_trims_old() {
        let mut messages = Vec::new();
        for i in 0..50 {
            messages.push(Message::user_text(format!("msg {}", i)));
            messages.push(Message::assistant_text(format!("reply {}", i)));
        }
        assert_eq!(messages.len(), 100);
        let compacted = AgentRunner::compact_messages(&messages);
        assert!(compacted.len() <= COMPACT_PRESERVE_MESSAGES);
        assert!(compacted.len() > 0);
    }

    #[test]
    fn compact_messages_starts_with_user() {
        let mut messages = Vec::new();
        messages.push(Message::user_text("first"));
        // Simulate many assistant then user messages
        for i in 0..40 {
            messages.push(Message::assistant_text(format!("reply {}", i)));
            messages.push(Message::user_text(format!("msg {}", i)));
        }
        let compacted = AgentRunner::compact_messages(&messages);
        assert_eq!(compacted[0].role, crate::llm::MessageRole::User);
    }

    #[test]
    fn compact_messages_drops_orphaned_tool_results() {
        use crate::llm::ContentBlock;
        let mut messages = Vec::new();
        for i in 0..40 {
            messages.push(Message::user_text(format!("msg {}", i)));
            messages.push(Message::assistant_text(format!("reply {}", i)));
        }
        // Add a tool_result at a position that would be the first after trim
        let tool_result_msg = Message::tool_results(vec![ContentBlock::ToolResult {
            tool_use_id: "test-id".to_string(),
            content: "result".to_string(),
            is_error: false,
        }]);
        // Insert at position that will be first after compaction
        let insert_pos = messages.len() - COMPACT_PRESERVE_MESSAGES;
        messages.insert(insert_pos, tool_result_msg);

        let compacted = AgentRunner::compact_messages(&messages);
        // Should not start with a tool_result
        let first_has_tool_result = compacted[0]
            .content
            .iter()
            .any(|b| matches!(b, ContentBlock::ToolResult { .. }));
        assert!(!first_has_tool_result);
    }

    #[test]
    fn compact_messages_never_returns_empty() {
        let messages = vec![Message::assistant_text("only assistant")];
        let compacted = AgentRunner::compact_messages(&messages);
        assert!(!compacted.is_empty());
    }

    #[test]
    fn agent_struct() {
        let mut agent = Agent::new("test-agent".to_string());
        assert_eq!(agent.name, "test-agent");
        assert!(agent.description.is_empty());
        assert!(agent.prompt.is_empty());
        agent.description = "A test agent".to_string();
        agent.prompt = "Do things".to_string();
        assert_eq!(agent.description, "A test agent");
    }
}
