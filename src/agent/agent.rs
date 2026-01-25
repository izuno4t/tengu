// Agent module
// エージェント実行ループ（最小）

use anyhow::Result;
use futures_util::stream::{self, BoxStream, StreamExt};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::llm::{LlmClient, LlmResponse, LlmStream};
use crate::tools::{ToolExecutor, ToolInput, ToolPolicy, ToolResult};

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

pub struct AgentRunner {
    client: LlmClient,
    model_name: String,
    tool_policy: ToolPolicy,
}

pub struct AgentOutput {
    pub response: LlmResponse,
    pub tool_result: Option<ToolResult>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(tag = "tool", rename_all = "lowercase")]
enum ToolCall {
    Read {
        path: String,
    },
    Write {
        path: String,
        content: String,
    },
    Grep {
        pattern: String,
        paths: Vec<String>,
    },
    Glob {
        pattern: String,
        root: Option<String>,
    },
}

impl AgentRunner {
    pub fn new(client: LlmClient, model_name: String, tool_policy: ToolPolicy) -> Self {
        Self {
            client,
            model_name,
            tool_policy,
        }
    }

    pub async fn handle_prompt(&self, input: &str) -> Result<AgentOutput> {
        let (plan, final_prompt, tool_result) = self.resolve_final_prompt(input).await?;
        let final_response = self
            .client
            .generate(&self.model_name, &final_prompt)
            .await?;
        let response = LlmResponse {
            content: format!(
                "計画:\n{}\n\n{}",
                plan.trim(),
                final_response.content.trim()
            ),
        };
        Ok(AgentOutput { response, tool_result })
    }

    pub async fn handle_prompt_stream(&self, input: &str) -> Result<LlmStream> {
        let (plan, final_prompt, _tool_result) = self.resolve_final_prompt(input).await?;
        let stream = self
            .client
            .generate_stream(&self.model_name, &final_prompt)
            .await?;
        let prefix = format!("計画:\n{}\n\n", plan.trim());
        let prefix_stream = stream::once(async move { Ok(prefix) });
        Ok(Box::pin(prefix_stream.chain(stream)) as BoxStream<'static, Result<String>>)
    }

    fn execute_tool_call(&self, call: ToolCall) -> Result<ToolResult> {
        let executor = ToolExecutor::with_policy(self.tool_policy.clone());
        match call {
            ToolCall::Read { path } => executor.execute(ToolInput::Read {
                path: PathBuf::from(path),
            }),
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

    async fn resolve_final_prompt(
        &self,
        input: &str,
    ) -> Result<(String, String, Option<ToolResult>)> {
        let plan = self.generate_plan(input).await?;
        let mut last_error: Option<String> = None;
        let mut last_call: Option<ToolCall> = None;
        let mut tool_result: Option<ToolResult> = None;

        for attempt in 0..=MAX_TOOL_RETRIES {
            let selection = self
                .select_tool(input, &plan, last_error.as_deref(), last_call.as_ref())
                .await?;
            let Some(call) = selection else {
                let execute_prompt = build_execute_prompt(input, &plan);
                return Ok((plan, execute_prompt, tool_result));
            };

            match self.execute_tool_call(call.clone()) {
                Ok(result) => {
                    let follow_prompt =
                        build_followup_prompt(input, &plan, &format_tool_result(&result));
                    tool_result = Some(result);
                    return Ok((plan, follow_prompt, tool_result));
                }
                Err(err) => {
                    last_error = Some(err.to_string());
                    last_call = Some(call);
                    if attempt >= MAX_TOOL_RETRIES {
                        let fallback_prompt =
                            build_failed_followup_prompt(input, &plan, last_error.as_deref());
                        return Ok((plan, fallback_prompt, tool_result));
                    }
                }
            }
        }

        Err(anyhow::anyhow!("final prompt is missing"))
    }

    async fn select_tool(
        &self,
        input: &str,
        plan: &str,
        last_error: Option<&str>,
        last_call: Option<&ToolCall>,
    ) -> Result<Option<ToolCall>> {
        let prompt = build_tool_select_prompt(input, plan, last_error, last_call);
        let response = self.client.generate(&self.model_name, &prompt).await?;
        Ok(parse_tool_call_loose(&response.content))
    }
}

const MAX_TOOL_RETRIES: usize = 2;

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

fn build_tool_select_prompt(
    input: &str,
    plan: &str,
    last_error: Option<&str>,
    last_call: Option<&ToolCall>,
) -> String {
    let mut context = String::new();
    if let Some(error) = last_error {
        context.push_str("\n前回の失敗理由:\n");
        context.push_str(error);
        context.push('\n');
    }
    if let Some(call) = last_call {
        if let Ok(json) = serde_json::to_string(call) {
            context.push_str("前回のツール呼び出し:\n");
            context.push_str(&json);
            context.push('\n');
        }
    }
    format!(
        "次の計画を進めるために必要なツールがあれば、JSONのみで出力してください。\n\
ツールが不要なら {{\"tool\":\"none\"}} とだけ出力してください。{}\n\n\
計画:\n{}\n\n指示:\n{}",
        context, plan, input
    )
}

fn build_followup_prompt(input: &str, plan: &str, tool_result: &str) -> String {
    format!(
        "実行結果を踏まえて最終回答を簡潔に出力してください。\n\n指示:\n{}\n\n計画:\n{}\n\nツール結果:\n{}",
        input, plan, tool_result
    )
}

fn build_failed_followup_prompt(input: &str, plan: &str, error: Option<&str>) -> String {
    let mut prompt = format!(
        "ツール実行に失敗したため、失敗理由を踏まえて最終回答を簡潔に出力してください。\n\n指示:\n{}\n\n計画:\n{}",
        input, plan
    );
    if let Some(error) = error {
        prompt.push_str("\n\n失敗理由:\n");
        prompt.push_str(error);
    }
    prompt
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
