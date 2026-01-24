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
        let plan = self.generate_plan(input).await?;
        let selection = self.select_tool(input, &plan).await?;
        let (tool_result, final_response) = if let Some(call) = selection {
            let result = self.execute_tool_call(call)?;
            let follow_prompt = build_followup_prompt(input, &plan, &format_tool_result(&result));
            let response = self
                .client
                .generate(&self.model_name, &follow_prompt)
                .await?;
            (Some(result), response)
        } else {
            let execute_prompt = build_execute_prompt(input, &plan);
            let response = self
                .client
                .generate(&self.model_name, &execute_prompt)
                .await?;
            (None, response)
        };
        let response = LlmResponse {
            content: format!("計画:\n{}\n\n{}", plan.trim(), final_response.content.trim()),
        };
        Ok(AgentOutput {
            response,
            tool_result,
        })
    }

    fn execute_tool_call(&self, call: ToolCall) -> Result<ToolResult> {
        let executor = ToolExecutor::new();
        match call {
            ToolCall::Read { path } => {
                executor.execute(ToolInput::Read { path: PathBuf::from(path) })
            }
            ToolCall::Write { path, content } => {
                executor.preview_write(PathBuf::from(path), content)
            }
            ToolCall::Grep { pattern, paths } => executor.execute(ToolInput::Grep {
                pattern,
                paths: paths.into_iter().map(PathBuf::from).collect(),
            }),
            ToolCall::Glob { pattern, root } => executor.execute(ToolInput::Glob {
                pattern,
                root: root.map(PathBuf::from),
            }),
        }
    }
}

impl AgentRunner {
    async fn generate_plan(&self, input: &str) -> Result<String> {
        let prompt = build_plan_prompt(input);
        let response = self.client.generate(&self.model_name, &prompt).await?;
        Ok(response.content)
    }

    async fn select_tool(&self, input: &str, plan: &str) -> Result<Option<ToolCall>> {
        let prompt = build_tool_select_prompt(input, plan);
        let response = self.client.generate(&self.model_name, &prompt).await?;
        Ok(parse_tool_call_loose(&response.content))
    }
}

fn build_plan_prompt(input: &str) -> String {
    format!(
        "次の指示に対して、最小の計画を1-3項目で日本語の箇条書きで作成してください。\n\n指示:\n{}",
        input
    )
}

fn build_execute_prompt(input: &str, plan: &str) -> String {
    format!(
        "次の計画に従って実行してください。\n\n計画:\n{}\n\n指示:\n{}",
        plan, input
    )
}

fn build_tool_select_prompt(input: &str, plan: &str) -> String {
    format!(
        "次の計画を進めるために必要なツールがあれば、JSONのみで出力してください。\n\
ツールが不要なら {{\"tool\":\"none\"}} とだけ出力してください。\n\n\
計画:\n{}\n\n指示:\n{}",
        plan, input
    )
}

fn build_followup_prompt(input: &str, plan: &str, tool_result: &str) -> String {
    format!(
        "実行結果を踏まえて最終回答を簡潔に出力してください。\n\n指示:\n{}\n\n計画:\n{}\n\nツール結果:\n{}",
        input, plan, tool_result
    )
}

fn format_tool_result(result: &ToolResult) -> String {
    match result {
        ToolResult::Text(text) => text.clone(),
        ToolResult::Lines(lines) => lines.join("\n"),
        ToolResult::Paths(paths) => paths
            .iter()
            .map(|p| p.display().to_string())
            .collect::<Vec<_>>()
            .join("\n"),
        ToolResult::Status(code) => format!("status: {}", code),
        ToolResult::PreviewWrite { diff, .. } => diff.clone(),
    }
}

fn parse_tool_call_loose(content: &str) -> Option<ToolCall> {
    let trimmed = content.trim();
    if !trimmed.starts_with('{') {
        return None;
    }
    let call: ToolCall = serde_json::from_str(trimmed).ok()?;
    match call {
        ToolCall::Read { .. }
        | ToolCall::Write { .. }
        | ToolCall::Grep { .. }
        | ToolCall::Glob { .. } => Some(call),
    }
}
