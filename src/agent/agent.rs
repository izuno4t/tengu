// Agent module
// エージェント実行ループ（最小）

use anyhow::Result;
use serde::Deserialize;
use std::path::PathBuf;

use crate::llm::{LlmClient, LlmResponse};
use crate::tools::{ToolExecutor, ToolInput, ToolResult};

pub struct Agent {
    pub name: String,
    pub description: String,
    pub prompt: String,
}

impl Agent {
    pub fn new(name: String) -> Self {
        Self {
            name,
            description: String::new(),
            prompt: String::new(),
        }
    }
}

pub struct AgentRunner {
    client: LlmClient,
    model_name: String,
}

pub struct AgentOutput {
    pub response: LlmResponse,
    pub tool_result: Option<ToolResult>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "tool", rename_all = "lowercase")]
enum ToolCall {
    Read { path: String },
    Write { path: String, content: String },
    Grep { pattern: String, paths: Vec<String> },
    Glob { pattern: String, root: Option<String> },
}

impl AgentRunner {
    pub fn new(client: LlmClient, model_name: String) -> Self {
        Self { client, model_name }
    }

    pub async fn handle_prompt(&self, input: &str) -> Result<AgentOutput> {
        let response = self.client.generate(&self.model_name, input).await?;
        let tool_result = self.try_execute_tool(&response.content)?;
        Ok(AgentOutput {
            response,
            tool_result,
        })
    }

    fn try_execute_tool(&self, content: &str) -> Result<Option<ToolResult>> {
        let call = parse_tool_call(content)?;
        let Some(call) = call else {
            return Ok(None);
        };
        let executor = ToolExecutor::new();
        let result = match call {
            ToolCall::Read { path } => {
                executor.execute(ToolInput::Read { path: PathBuf::from(path) })?
            }
            ToolCall::Write { path, content } => executor.execute(ToolInput::Write {
                path: PathBuf::from(path),
                content,
            })?,
            ToolCall::Grep { pattern, paths } => executor.execute(ToolInput::Grep {
                pattern,
                paths: paths.into_iter().map(PathBuf::from).collect(),
            })?,
            ToolCall::Glob { pattern, root } => executor.execute(ToolInput::Glob {
                pattern,
                root: root.map(PathBuf::from),
            })?,
        };
        Ok(Some(result))
    }
}

fn parse_tool_call(content: &str) -> Result<Option<ToolCall>> {
    let trimmed = content.trim();
    if !trimmed.starts_with('{') {
        return Ok(None);
    }
    let call: ToolCall = serde_json::from_str(trimmed)?;
    Ok(Some(call))
}
