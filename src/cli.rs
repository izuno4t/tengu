use crate::agent::{AgentOutput, AgentRunner, AgentStore, StoredAgent};
use crate::config::Config;
use crate::llm::{
    AnthropicBackend, GoogleBackend, LlmBackend, LlmClient, LlmImage, LlmProvider, LlmRequest,
    LlmStreamEvent, LlmUsage, OllamaBackend, OpenAiBackend,
};
use crate::mcp::{list_tools_http, list_tools_stdio, McpServerConfig, McpStore};
use crate::review::{build_review_prompt, ReviewOptions};
use crate::session::{Session, SessionStore};
use crate::tools::{ToolExecutor, ToolInput, ToolPolicy, ToolResult};
use crate::tui::App;
use anyhow::{anyhow, Result};
use base64::Engine;
use chrono::Utc;
use clap::{Parser, Subcommand};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Parser, Debug)]
#[command(
    name = "tengu",
    version,
    about = "👺 天狗のように高みから見渡し、複数のAIを統べるコーディングエージェントCLI",
    long_about = None
)]
pub struct Cli {
    /// プロンプト（ワンショット実行）
    #[arg(short, long)]
    pub prompt: Option<String>,

    /// 使用するモデル
    #[arg(long)]
    pub model: Option<String>,

    /// 画像入力（カンマ区切りまたは複数指定）
    #[arg(long, value_delimiter = ',')]
    pub image: Vec<PathBuf>,

    /// OllamaベースURL（例: http://localhost:11434）
    #[arg(long)]
    pub ollama_base_url: Option<String>,

    /// 許可するツール（カンマ区切り）
    #[arg(long)]
    pub allowed_tools: Option<String>,

    /// システムプロンプト（完全置換）
    #[arg(long)]
    pub system_prompt: Option<String>,

    /// システムプロンプトファイル
    #[arg(long)]
    pub system_prompt_file: Option<PathBuf>,

    /// システムプロンプトに追加
    #[arg(long)]
    pub append_system_prompt: Option<String>,

    /// 追加システムプロンプトファイル
    #[arg(long)]
    pub append_system_prompt_file: Option<PathBuf>,

    /// 出力フォーマット (text/json/stream-json)
    #[arg(long, default_value = "text")]
    pub output_format: String,

    /// カスタムエージェント
    #[arg(long)]
    pub agent: Option<String>,

    /// 作業ディレクトリ
    #[arg(long)]
    pub cwd: Option<PathBuf>,

    /// 追加ディレクトリ
    #[arg(long)]
    pub add_dir: Vec<PathBuf>,

    /// 詳細ログ
    #[arg(short, long)]
    pub verbose: bool,

    /// サブコマンド
    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// MCPサーバー管理
    Mcp {
        #[command(subcommand)]
        command: McpCommands,
    },

    /// エージェント管理
    Agent {
        #[command(subcommand)]
        command: AgentCommands,
    },

    /// セッション管理
    Sessions {
        #[command(subcommand)]
        command: SessionCommands,
    },

    /// セッション再開
    Resume {
        /// セッションID（省略時は選択画面）
        session_id: Option<String>,

        /// 最新セッションを再開
        #[arg(long)]
        last: bool,
    },

    /// 新規セッション開始
    New,

    /// Git diff review
    Review {
        /// Base ref for diff range (<base>...HEAD)
        #[arg(long)]
        base: Option<String>,

        /// Review preset (general/security/performance/correctness)
        #[arg(long)]
        preset: Option<String>,
    },

    /// 認証管理
    Auth {
        #[command(subcommand)]
        command: AuthCommands,
    },

    /// ツール実行（確認用）
    Tool {
        #[command(subcommand)]
        command: ToolCommands,
    },

    /// TUI起動（確認用）
    Tui,
}

#[derive(Subcommand, Debug)]
pub enum McpCommands {
    /// MCPサーバー追加
    Add {
        /// サーバー名
        name: String,

        /// コマンド（-- の後に指定）
        #[arg(last = true)]
        command: Vec<String>,
    },

    /// MCPサーバー一覧
    List,

    /// MCPサーバー削除
    Remove {
        /// サーバー名
        name: String,
    },

    /// MCPサーバーのツール一覧
    Tools {
        /// サーバー名
        name: String,
    },
}

#[derive(Subcommand, Debug)]
pub enum AgentCommands {
    /// エージェント一覧
    List,

    /// エージェント作成
    Create {
        /// エージェント名
        name: String,
    },

    /// エージェント削除
    Remove {
        /// エージェント名
        name: String,
    },

    /// AI支援でエージェント生成
    Generate,
}

#[derive(Subcommand, Debug)]
pub enum SessionCommands {
    /// セッション一覧
    List,

    /// セッション削除
    Delete {
        /// セッションID
        session_id: String,
    },

    /// 全セッション削除
    Clear,
}

#[derive(Subcommand, Debug)]
pub enum AuthCommands {
    /// ログイン
    Login,

    /// ログアウト
    Logout,

    /// ステータス確認
    Status,
}

#[derive(Subcommand, Debug)]
pub enum ToolCommands {
    /// ファイル読み込み
    Read {
        /// 読み込みパス
        path: PathBuf,
    },
    /// ファイル書き込み
    Write {
        /// 書き込みパス
        path: PathBuf,
        /// 書き込み内容
        content: String,
    },
    /// シェルコマンド実行
    Shell {
        /// 実行コマンド
        command: String,
        /// 引数（複数可）
        args: Vec<String>,
    },
    /// 文字列検索
    Grep {
        /// 検索文字列
        pattern: String,
        /// 対象パス（複数可）
        paths: Vec<PathBuf>,
    },
    /// グロブ検索
    Glob {
        /// パターン
        pattern: String,
        /// ルートパス
        root: Option<PathBuf>,
    },
}

impl Cli {
    pub async fn execute(self) -> Result<()> {
        if let Some(command) = &self.command {
            self.execute_command(command).await
        } else if self.prompt.is_some() {
            self.execute_headless().await
        } else {
            self.execute_tui().await
        }
    }

    async fn execute_command(&self, command: &Commands) -> Result<()> {
        match command {
            Commands::Mcp { command } => self.execute_mcp_command(command).await,
            Commands::Agent { command } => self.execute_agent_command(command).await,
            Commands::Sessions { command } => self.execute_session_command(command).await,
            Commands::Tool { command } => self.execute_tool_command(command).await,
            Commands::Tui => self.execute_tui().await,
            Commands::Review { base, preset } => {
                self.execute_review_command(base.clone(), preset.clone())
                    .await
            }
            Commands::Resume { session_id, last } => {
                let store = SessionStore::new(SessionStore::default_root()?);
                if *last {
                    if let Some(entry) = store.latest()? {
                        let session = store.load(&entry.id)?;
                        println!("resume: {} {}", session.id, session.updated_at);
                    } else {
                        println!("no sessions");
                    }
                } else if let Some(session_id) = session_id {
                    let session = store.load(session_id)?;
                    println!("resume: {} {}", session.id, session.updated_at);
                } else {
                    println!("session id required (use --last for latest)");
                }
                Ok(())
            }
            Commands::New => {
                let store = SessionStore::new(SessionStore::default_root()?);
                let session = Session::new();
                store.save(&session)?;
                println!("new session: {}", session.id);
                Ok(())
            }
            Commands::Auth { command } => self.execute_auth_command(command).await,
        }
    }

    async fn execute_mcp_command(&self, command: &McpCommands) -> Result<()> {
        let path = McpStore::default_path();
        let mut config = McpStore::load(&path)?;
        match command {
            McpCommands::Add { name, command } => {
                if command.is_empty() {
                    return Err(anyhow!("mcp add requires a command"));
                }
                let mut iter = command.iter();
                let cmd = iter.next().cloned().unwrap_or_default();
                let args: Vec<String> = iter.cloned().collect();
                let entry = McpServerConfig {
                    command: Some(cmd),
                    args: if args.is_empty() { None } else { Some(args) },
                    env: None,
                    url: None,
                    bearer_token_env_var: None,
                    http_headers: None,
                    timeout_sec: None,
                };
                config.mcp_servers.insert(name.clone(), entry);
                McpStore::save(&path, &config)?;
                println!("mcp server added: {}", name);
                Ok(())
            }
            McpCommands::List => {
                if config.mcp_servers.is_empty() {
                    println!("no mcp servers");
                    return Ok(());
                }
                for (name, server) in config.mcp_servers.iter() {
                    let summary = if let Some(url) = &server.url {
                        format!("http {}", url)
                    } else if let Some(cmd) = &server.command {
                        let args = server
                            .args
                            .as_ref()
                            .map(|a| a.join(" "))
                            .unwrap_or_default();
                        if args.is_empty() {
                            format!("stdio {}", cmd)
                        } else {
                            format!("stdio {} {}", cmd, args)
                        }
                    } else {
                        "unknown".to_string()
                    };
                    println!("{} {}", name, summary.trim());
                }
                Ok(())
            }
            McpCommands::Remove { name } => {
                if config.mcp_servers.remove(name).is_some() {
                    McpStore::save(&path, &config)?;
                    println!("mcp server removed: {}", name);
                } else {
                    println!("mcp server not found: {}", name);
                }
                Ok(())
            }
            McpCommands::Tools { name } => {
                let Some(server) = config.mcp_servers.get(name) else {
                    println!("mcp server not found: {}", name);
                    return Ok(());
                };
                if server.url.is_some() {
                    let tools = list_tools_http(server).await?;
                    for tool in tools {
                        println!("@{}/{}", name, tool.name);
                    }
                    return Ok(());
                }
                let tools = tokio::task::spawn_blocking({
                    let server = server.clone();
                    move || list_tools_stdio(&server)
                })
                .await??;
                for tool in tools {
                    println!("@{}/{}", name, tool.name);
                }
                Ok(())
            }
        }
    }

    async fn execute_agent_command(&self, command: &AgentCommands) -> Result<()> {
        let store = AgentStore::new();
        match command {
            AgentCommands::List => {
                let agents = store.list()?;
                if agents.is_empty() {
                    println!("no agents");
                } else {
                    for agent in agents {
                        println!("{} {}", agent.name, agent.description);
                    }
                }
                Ok(())
            }
            AgentCommands::Create { name } => {
                let agent = StoredAgent::scaffold(name);
                let path = store.save_local(&agent)?;
                println!("agent created: {}", path.display());
                Ok(())
            }
            AgentCommands::Remove { name } => {
                if store.remove(name)? {
                    println!("agent removed: {}", name);
                } else {
                    println!("agent not found: {}", name);
                }
                Ok(())
            }
            AgentCommands::Generate => self.generate_agent_with_llm(&store).await,
        }
    }

    async fn execute_session_command(&self, command: &SessionCommands) -> Result<()> {
        let store = SessionStore::new(SessionStore::default_root()?);
        match command {
            SessionCommands::List => {
                let sessions = store.list()?;
                if sessions.is_empty() {
                    println!("no sessions");
                } else {
                    for entry in sessions {
                        println!("{} {} {}", entry.id, entry.created_at, entry.updated_at);
                    }
                }
                Ok(())
            }
            SessionCommands::Delete { session_id } => {
                store.delete(session_id)?;
                println!("deleted: {}", session_id);
                Ok(())
            }
            SessionCommands::Clear => {
                store.clear()?;
                println!("cleared");
                Ok(())
            }
        }
    }

    async fn execute_auth_command(&self, command: &AuthCommands) -> Result<()> {
        let config = load_config().unwrap_or_default();
        let provider_name = if !config.model.provider.trim().is_empty() {
            config.model.provider.as_str()
        } else {
            "anthropic"
        };
        let required_env = auth_env_var_for_provider(provider_name);
        match command {
            AuthCommands::Login => {
                let Some(env_name) = required_env else {
                    return Err(anyhow!(
                        "auth login is unsupported for provider: {}",
                        provider_name
                    ));
                };
                if std::env::var(env_name).is_err() {
                    return Err(anyhow!("{} is not set", env_name));
                }
                save_auth_session(provider_name, env_name)?;
                println!("auth ready: provider={} via {}", provider_name, env_name);
                Ok(())
            }
            AuthCommands::Logout => {
                clear_auth_session()?;
                println!("auth session cleared");
                Ok(())
            }
            AuthCommands::Status => {
                let env_status = required_env
                    .map(|env_name| {
                        if std::env::var(env_name).is_ok() {
                            format!("{}=set", env_name)
                        } else {
                            format!("{}=missing", env_name)
                        }
                    })
                    .unwrap_or_else(|| "env=unsupported".to_string());
                let session_status = load_auth_session()
                    .map(|session| {
                        format!(
                            "session=ready provider={} updated_at={}",
                            session.provider, session.updated_at
                        )
                    })
                    .unwrap_or_else(|| "session=none".to_string());
                println!(
                    "provider: {}\n{}\n{}",
                    provider_name, env_status, session_status
                );
                Ok(())
            }
        }
    }

    async fn execute_tool_command(&self, command: &ToolCommands) -> Result<()> {
        let config = load_config().unwrap_or_default();
        let policy = ToolPolicy::from_config(&config);
        let executor = ToolExecutor::with_policy(policy);
        let result = match command {
            ToolCommands::Read { path } => {
                executor.execute(ToolInput::Read { path: path.clone() })?
            }
            ToolCommands::Write { path, content } => {
                let preview = executor.preview_write(path.clone(), content.clone())?;
                println!("{}", format_tool_result(&preview));
                if let Some(applied) = apply_preview_write(&executor, &preview)? {
                    println!("{}", format_tool_result(&applied));
                }
                return Ok(());
            }
            ToolCommands::Shell { command, args } => executor.execute(ToolInput::Shell {
                command: command.clone(),
                args: args.clone(),
            })?,
            ToolCommands::Grep { pattern, paths } => executor.execute(ToolInput::Grep {
                pattern: pattern.clone(),
                paths: paths.clone(),
            })?,
            ToolCommands::Glob { pattern, root } => executor.execute(ToolInput::Glob {
                pattern: pattern.clone(),
                root: root.clone(),
            })?,
        };

        println!("{}", format_tool_result(&result));

        Ok(())
    }

    async fn execute_review_command(
        &self,
        base: Option<String>,
        preset: Option<String>,
    ) -> Result<()> {
        let options = ReviewOptions { base, preset };
        let Some(prompt) = build_review_prompt(&options)? else {
            println!("no diff to review");
            return Ok(());
        };

        let config = load_config().unwrap_or_default();
        let (client, model_name) = self.resolve_llm_with_config(&config)?;
        let policy = ToolPolicy::from_config(&config);
        let runner = AgentRunner::new(client, model_name, policy);

        if self.output_format == "stream-json" {
            let (mut stream, _tool_result) = runner
                .handle_prompt_stream_with_tool_context(&prompt, "")
                .await?;
            println!("{}", json!({ "type": "start", "mode": "review" }));
            while let Some(chunk) = stream.next().await {
                match chunk {
                    Ok(LlmStreamEvent::Text(text)) => {
                        println!(
                            "{}",
                            json!({ "type": "chunk", "mode": "review", "delta": text })
                        );
                    }
                    Ok(LlmStreamEvent::Usage(usage)) => {
                        println!(
                            "{}",
                            json!({ "type": "usage", "mode": "review", "usage": usage_to_json(&usage) })
                        );
                    }
                    Err(err) => {
                        println!(
                            "{}",
                            json!({ "type": "error", "mode": "review", "message": err.to_string() })
                        );
                        println!("{}", json!({ "type": "end", "mode": "review" }));
                        return Err(err);
                    }
                }
            }
            println!("{}", json!({ "type": "end", "mode": "review" }));
            return Ok(());
        }

        let output = runner.handle_prompt(&prompt).await?;
        self.print_output("review", &output.response.content, None);
        Ok(())
    }

    async fn execute_tui(&self) -> Result<()> {
        let banner = "👺 Tengu - Interactive mode".to_string();
        let config = load_config().unwrap_or_default();
        let (client, model_name) = self.resolve_llm_with_config(&config)?;
        let policy = ToolPolicy::from_config(&config);
        let status_model = model_name.clone();
        let runner = std::sync::Arc::new(AgentRunner::new(client, model_name, policy));
        let handle = tokio::runtime::Handle::current();
        let status_build = option_env!("BUILD_TIMESTAMP")
            .unwrap_or("unknown")
            .to_string();
        let (result_tx, result_rx) = std::sync::mpsc::channel();
        let mut app = App::new(
            runner,
            handle,
            banner,
            status_model,
            status_build,
            result_rx,
            result_tx,
        );
        app.run()?;
        Ok(())
    }

    async fn execute_headless(&self) -> Result<()> {
        let (system_prompt, sources) = self.resolve_system_prompt()?;
        self.log_system_prompt_sources(&sources, system_prompt.as_deref());
        if let Some(prompt) = self.prompt.as_deref() {
            let request = build_headless_request(prompt, system_prompt.as_deref(), &self.image)?;
            if self.output_format == "stream-json" {
                let config = load_config().unwrap_or_default();
                let (client, model_name) = self.resolve_llm_with_config(&config)?;
                if !request.images.is_empty() {
                    let mut stream = client.generate_stream(&model_name, &request).await?;
                    println!("{}", json!({ "type": "start", "mode": "llm" }));
                    while let Some(chunk) = stream.next().await {
                        match chunk {
                            Ok(LlmStreamEvent::Text(text)) => {
                                println!(
                                    "{}",
                                    json!({ "type": "chunk", "mode": "llm", "delta": text })
                                );
                            }
                            Ok(LlmStreamEvent::Usage(usage)) => {
                                println!(
                                    "{}",
                                    json!({ "type": "usage", "mode": "llm", "usage": usage_to_json(&usage) })
                                );
                            }
                            Err(err) => {
                                println!(
                                    "{}",
                                    json!({ "type": "error", "mode": "llm", "message": err.to_string() })
                                );
                                println!("{}", json!({ "type": "end", "mode": "llm" }));
                                return Err(err);
                            }
                        }
                    }
                    println!("{}", json!({ "type": "end", "mode": "llm" }));
                    return Ok(());
                }
                let policy = ToolPolicy::from_config(&config);
                let runner = AgentRunner::new(client, model_name, policy);
                let (mut stream, tool_result) = runner
                    .handle_prompt_stream_with_tool_context(&request.prompt, "")
                    .await?;

                println!("{}", json!({ "type": "start", "mode": "llm" }));
                while let Some(chunk) = stream.next().await {
                    match chunk {
                        Ok(LlmStreamEvent::Text(text)) => {
                            println!(
                                "{}",
                                json!({ "type": "chunk", "mode": "llm", "delta": text })
                            );
                        }
                        Ok(LlmStreamEvent::Usage(usage)) => {
                            println!(
                                "{}",
                                json!({ "type": "usage", "mode": "llm", "usage": usage_to_json(&usage) })
                            );
                        }
                        Err(err) => {
                            println!(
                                "{}",
                                json!({ "type": "error", "mode": "llm", "message": err.to_string() })
                            );
                            println!("{}", json!({ "type": "end", "mode": "llm" }));
                            return Err(err);
                        }
                    }
                }

                if let Some(result) = tool_result.as_ref() {
                    println!(
                        "{}",
                        json!({
                            "type": "tool",
                            "mode": "tool",
                            "content": format_tool_result(result)
                        })
                    );
                    if let Some(applied) = apply_preview_write_with_config(result)? {
                        println!(
                            "{}",
                            json!({
                                "type": "tool",
                                "mode": "tool",
                                "content": format_tool_result(&applied)
                            })
                        );
                    }
                }
                println!("{}", json!({ "type": "end", "mode": "llm" }));
                return Ok(());
            }
        }
        let message = format!("Headless mode with prompt: {:?}", self.prompt);
        self.print_output("headless", &message, self.prompt.as_deref());
        if let Some(prompt) = self.prompt.as_deref() {
            let request = build_headless_request(prompt, system_prompt.as_deref(), &self.image)?;
            let config = load_config().unwrap_or_default();
            let (client, model_name) = self.resolve_llm_with_config(&config)?;
            if !request.images.is_empty() {
                let output = client.generate(&model_name, &request).await?;
                if self.output_format == "json" {
                    if let Some(usage) = output.usage.as_ref() {
                        println!(
                            "{}",
                            json!({ "type": "usage", "usage": usage_to_json(usage) })
                        );
                    }
                }
                self.print_output("llm", &output.content, Some(prompt));
                return Ok(());
            }
            let policy = ToolPolicy::from_config(&config);
            let runner = AgentRunner::new(client, model_name, policy);
            let output = runner.handle_prompt(&request.prompt).await?;
            if self.output_format == "json" {
                if let Some(usage) = output.response.usage.as_ref() {
                    println!(
                        "{}",
                        json!({ "type": "usage", "usage": usage_to_json(usage) })
                    );
                }
            }
            self.print_output("llm", &output.response.content, Some(prompt));
            self.print_tool_result(&output);
        }
        Ok(())
    }

    fn print_output(&self, mode: &str, message: &str, prompt: Option<&str>) {
        match self.output_format.as_str() {
            "json" => {
                let payload = json!({
                    "type": "response",
                    "mode": mode,
                    "prompt": prompt,
                    "message": message
                });
                println!("{}", payload);
            }
            "stream-json" => {
                let start = json!({ "type": "start", "mode": mode });
                println!("{}", start);
                let item = json!({ "type": "message", "prompt": prompt, "content": message });
                println!("{}", item);
                let end = json!({ "type": "end", "mode": mode });
                println!("{}", end);
            }
            _ => {
                println!("{}", message);
            }
        }
    }

    fn resolve_system_prompt(&self) -> Result<(Option<String>, Vec<String>)> {
        let mut sources = Vec::new();
        let mut parts = Vec::new();

        if let Some(path) = &self.system_prompt_file {
            let content = read_required_file(path)?;
            sources.push(format!("system_prompt_file:{}", path.display()));
            parts.push(content);
        } else if let Some(prompt) = &self.system_prompt {
            sources.push("system_prompt_arg".to_string());
            parts.push(prompt.clone());
        } else {
            if let Some(home) = std::env::var_os("HOME") {
                let global_path = PathBuf::from(home).join(".tengu").join("TENGU.md");
                if let Some(content) = read_optional_file(&global_path)? {
                    sources.push(format!("global:{}", global_path.display()));
                    parts.push(content);
                }
            }

            let project_path = PathBuf::from(".").join(".tengu").join("TENGU.md");
            if let Some(content) = read_optional_file(&project_path)? {
                sources.push(format!("project:{}", project_path.display()));
                parts.push(content);
            }

            let workspace_path = PathBuf::from(".")
                .join("workspace")
                .join(".tengu")
                .join("TENGU.md");
            if let Some(content) = read_optional_file(&workspace_path)? {
                sources.push(format!("workspace:{}", workspace_path.display()));
                parts.push(content);
            }
        }

        if let Some(path) = &self.append_system_prompt_file {
            let content = read_required_file(path)?;
            sources.push(format!("append_file:{}", path.display()));
            parts.push(content);
        }

        if let Some(prompt) = &self.append_system_prompt {
            sources.push("append_arg".to_string());
            parts.push(prompt.clone());
        }

        if let Some(agent_name) = &self.agent {
            let store = AgentStore::new();
            let agent = store.load(agent_name)?;
            sources.push(format!("agent:{}", agent.name));
            parts.push(agent.prompt);
        }

        if parts.is_empty() {
            Ok((None, sources))
        } else {
            Ok((Some(parts.join("\n\n")), sources))
        }
    }

    fn log_system_prompt_sources(&self, sources: &[String], prompt: Option<&str>) {
        if !self.verbose {
            return;
        }
        if sources.is_empty() {
            eprintln!("system_prompt_sources: none");
            return;
        }
        eprintln!("system_prompt_sources: {}", sources.join(", "));
        if let Some(prompt) = prompt {
            eprintln!("system_prompt_length: {}", prompt.len());
        }
    }

    fn resolve_llm_with_config(&self, config: &Config) -> Result<(LlmClient, String)> {
        let configured_provider_name = if !config.model.provider.trim().is_empty() {
            config.model.provider.as_str()
        } else {
            config
                .model
                .backend
                .as_deref()
                .filter(|value| !value.trim().is_empty())
                .unwrap_or("anthropic")
        };
        let configured_provider = LlmProvider::from_str(configured_provider_name)?;
        let model_name = self
            .model
            .as_deref()
            .filter(|value| LlmProvider::from_str(value).is_err())
            .map(|value| value.to_string())
            .or_else(|| {
                config
                    .model
                    .name
                    .as_deref()
                    .filter(|value| !value.trim().is_empty())
                    .map(|value| value.to_string())
            })
            .or_else(|| {
                (!config.model.default.trim().is_empty()).then(|| config.model.default.clone())
            })
            .ok_or_else(|| anyhow!("model name is not set in config.toml"))?;
        let provider = match self.model.as_deref() {
            Some(value) => LlmProvider::from_str(value).unwrap_or(configured_provider),
            None => configured_provider,
        };
        let backend = build_backend(&provider, config, self.ollama_base_url.clone());
        Ok((LlmClient::new(backend), model_name))
    }
}

fn load_config() -> Option<Config> {
    let mut candidates = Vec::new();
    if let Some(home) = std::env::var_os("HOME") {
        candidates.push(PathBuf::from(home).join(".tengu").join("config.toml"));
    }
    candidates.push(PathBuf::from(".").join(".tengu").join("config.toml"));

    let mut config = None;
    for path in candidates {
        if path.exists() {
            if let Ok(loaded) = Config::load(&path) {
                config = Some(loaded);
            }
        }
    }
    config
}

fn build_backend(
    provider: &LlmProvider,
    config: &Config,
    cli_base_url: Option<String>,
) -> Box<dyn LlmBackend + Send + Sync> {
    match provider {
        LlmProvider::Local => {
            let base_url = cli_base_url
                .or_else(|| std::env::var("OLLAMA_BASE_URL").ok())
                .or_else(|| config.model.backend_url.clone())
                .unwrap_or_else(|| "http://localhost:11434".to_string());
            Box::new(OllamaBackend::new(base_url))
        }
        LlmProvider::Anthropic => Box::new(AnthropicBackend::new(
            config.model.backend_url.clone(),
            config.model.max_tokens,
        )),
        LlmProvider::OpenAI => Box::new(OpenAiBackend::new(
            config.model.backend_url.clone(),
            config.model.max_tokens,
        )),
        LlmProvider::Google => Box::new(GoogleBackend::new(config.model.backend_url.clone())),
    }
}

impl Cli {
    async fn generate_agent_with_llm(&self, store: &AgentStore) -> Result<()> {
        let config = load_config().unwrap_or_default();
        let (client, model_name) = self.resolve_llm_with_config(&config)?;
        let request = LlmRequest::text(
            "Create a practical coding assistant agent configuration.\n\
             Return JSON only with keys: name, description, prompt.\n\
             Requirements:\n\
             - name must be lowercase kebab-case\n\
             - description must be one short sentence\n\
             - prompt must instruct concise, pragmatic coding assistance\n\
             - do not include markdown fences or extra commentary",
        );
        let response = client.generate(&model_name, &request).await?;
        let agent =
            parse_generated_agent(&response.content).unwrap_or_else(|_| fallback_generated_agent());
        let path = store.save_local(&agent)?;
        println!("agent generated: {}", path.display());
        Ok(())
    }

    fn print_tool_result(&self, output: &AgentOutput) {
        let Some(result) = output.tool_result.as_ref() else {
            return;
        };
        self.print_output("tool", &format_tool_result(result), None);
        match apply_preview_write_with_config(result) {
            Ok(Some(applied)) => {
                self.print_output("tool", &format_tool_result(&applied), None);
            }
            Ok(None) => {}
            Err(err) => {
                eprintln!("failed to apply write: {}", err);
            }
        }
    }
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

fn apply_preview_write(executor: &ToolExecutor, result: &ToolResult) -> Result<Option<ToolResult>> {
    let ToolResult::PreviewWrite { path, content, .. } = result else {
        return Ok(None);
    };
    let applied = executor.execute(ToolInput::Write {
        path: path.clone(),
        content: content.clone(),
    })?;
    Ok(Some(applied))
}

fn apply_preview_write_with_config(result: &ToolResult) -> Result<Option<ToolResult>> {
    let config = load_config().unwrap_or_default();
    let policy = ToolPolicy::from_config(&config);
    let executor = ToolExecutor::with_policy(policy);
    apply_preview_write(&executor, result)
}

#[derive(Debug, Serialize, Deserialize)]
struct AuthSession {
    provider: String,
    env_var: String,
    updated_at: String,
}

fn auth_env_var_for_provider(provider: &str) -> Option<&'static str> {
    match provider.trim().to_ascii_lowercase().as_str() {
        "anthropic" => Some("ANTHROPIC_API_KEY"),
        "openai" => Some("OPENAI_API_KEY"),
        "google" | "gemini" => Some("GOOGLE_API_KEY"),
        _ => None,
    }
}

fn auth_session_path() -> Result<PathBuf> {
    let home = std::env::var("HOME").map_err(|_| anyhow!("HOME not set"))?;
    Ok(auth_session_path_from_home(Path::new(&home)))
}

fn save_auth_session(provider: &str, env_var: &str) -> Result<()> {
    let path = auth_session_path()?;
    save_auth_session_at(&path, provider, env_var)
}

fn auth_session_path_from_home(home: &Path) -> PathBuf {
    home.join(".tengu").join("auth").join("session.json")
}

fn save_auth_session_at(path: &Path, provider: &str, env_var: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let session = AuthSession {
        provider: provider.to_string(),
        env_var: env_var.to_string(),
        updated_at: Utc::now().to_rfc3339(),
    };
    fs::write(path, serde_json::to_string_pretty(&session)?)?;
    Ok(())
}

fn load_auth_session() -> Option<AuthSession> {
    let path = auth_session_path().ok()?;
    load_auth_session_from_path(&path)
}

fn load_auth_session_from_path(path: &Path) -> Option<AuthSession> {
    let data = fs::read_to_string(path).ok()?;
    serde_json::from_str(&data).ok()
}

fn clear_auth_session() -> Result<()> {
    let path = auth_session_path()?;
    clear_auth_session_at(&path)
}

fn clear_auth_session_at(path: &Path) -> Result<()> {
    if path.exists() {
        fs::remove_file(path)?;
    }
    Ok(())
}

fn usage_to_json(usage: &LlmUsage) -> serde_json::Value {
    json!({
        "provider": &usage.provider,
        "input_tokens": usage.input_tokens,
        "output_tokens": usage.output_tokens,
        "total_tokens": usage.total_tokens,
        "cache_creation_input_tokens": usage.cache_creation_input_tokens,
        "cache_read_input_tokens": usage.cache_read_input_tokens,
        "reasoning_tokens": usage.reasoning_tokens,
        "raw": usage.raw.as_ref(),
    })
}

fn parse_generated_agent(raw: &str) -> Result<StoredAgent> {
    let trimmed = raw.trim();
    let candidate = if let Some(stripped) = trimmed.strip_prefix("```") {
        let body = stripped
            .split_once('\n')
            .map(|(_, rest)| rest)
            .unwrap_or(stripped);
        body.rsplit_once("```")
            .map(|(content, _)| content.trim())
            .unwrap_or(body.trim())
    } else {
        trimmed
    };
    let json_slice = if candidate.starts_with('{') {
        candidate
    } else {
        let start = candidate
            .find('{')
            .ok_or_else(|| anyhow!("generated agent JSON start not found"))?;
        let end = candidate
            .rfind('}')
            .ok_or_else(|| anyhow!("generated agent JSON end not found"))?;
        &candidate[start..=end]
    };
    let agent: StoredAgent = serde_json::from_str(json_slice)?;
    if agent.name.trim().is_empty() {
        return Err(anyhow!("generated agent name is empty"));
    }
    if agent.description.trim().is_empty() {
        return Err(anyhow!("generated agent description is empty"));
    }
    if agent.prompt.trim().is_empty() {
        return Err(anyhow!("generated agent prompt is empty"));
    }
    Ok(agent)
}

fn fallback_generated_agent() -> StoredAgent {
    let name = format!("generated-{}", Utc::now().format("%Y%m%d%H%M%S"));
    StoredAgent {
        name: name.clone(),
        description: "LLM-generated fallback coding assistant".to_string(),
        prompt: format!(
            "You are the `{name}` agent. Provide concise, pragmatic coding help, \
             prioritize correctness, explain tradeoffs briefly, and propose concrete next steps."
        ),
    }
}

fn build_headless_request(
    prompt: &str,
    system_prompt: Option<&str>,
    image_paths: &[PathBuf],
) -> Result<LlmRequest> {
    let prompt = match system_prompt {
        Some(system_prompt) if !system_prompt.trim().is_empty() => format!(
            "System instructions:\n{}\n\nUser request:\n{}",
            system_prompt, prompt
        ),
        _ => prompt.to_string(),
    };

    let images = image_paths
        .iter()
        .map(|path| load_llm_image(path))
        .collect::<Result<Vec<_>>>()?;

    Ok(LlmRequest { prompt, images })
}

fn load_llm_image(path: &Path) -> Result<LlmImage> {
    let media_type = image_media_type(path)
        .ok_or_else(|| anyhow!("unsupported image type: {}", path.display()))?;
    let bytes = fs::read(path)?;
    let data_base64 = base64::engine::general_purpose::STANDARD.encode(bytes);
    Ok(LlmImage {
        media_type: media_type.to_string(),
        data_base64,
    })
}

fn image_media_type(path: &Path) -> Option<&'static str> {
    let ext = path.extension()?.to_str()?.to_ascii_lowercase();
    match ext.as_str() {
        "png" => Some("image/png"),
        "jpg" | "jpeg" => Some("image/jpeg"),
        "gif" => Some("image/gif"),
        "webp" => Some("image/webp"),
        _ => None,
    }
}

fn read_required_file(path: &Path) -> Result<String> {
    Ok(fs::read_to_string(path)?)
}

fn read_optional_file(path: &Path) -> Result<Option<String>> {
    if path.exists() {
        Ok(Some(fs::read_to_string(path)?))
    } else {
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_temp_dir(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("tengu-{name}-{nanos}"));
        fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn maps_provider_to_expected_auth_env_var() {
        assert_eq!(
            auth_env_var_for_provider("anthropic"),
            Some("ANTHROPIC_API_KEY")
        );
        assert_eq!(auth_env_var_for_provider("openai"), Some("OPENAI_API_KEY"));
        assert_eq!(auth_env_var_for_provider("gemini"), Some("GOOGLE_API_KEY"));
        assert_eq!(auth_env_var_for_provider("local"), None);
    }

    #[test]
    fn detects_supported_image_media_types() {
        assert_eq!(image_media_type(Path::new("a.png")), Some("image/png"));
        assert_eq!(image_media_type(Path::new("a.jpeg")), Some("image/jpeg"));
        assert_eq!(image_media_type(Path::new("a.gif")), Some("image/gif"));
        assert_eq!(image_media_type(Path::new("a.webp")), Some("image/webp"));
        assert_eq!(image_media_type(Path::new("a.txt")), None);
    }

    #[test]
    fn builds_headless_request_with_system_prompt() {
        let request = build_headless_request("hello", Some("system"), &[]).unwrap();
        assert!(request.prompt.contains("System instructions:"));
        assert!(request.prompt.contains("User request:"));
        assert!(request.images.is_empty());
    }

    #[test]
    fn saves_loads_and_clears_auth_session_file() {
        let root = unique_temp_dir("auth-session");
        let path = auth_session_path_from_home(&root);

        save_auth_session_at(&path, "anthropic", "ANTHROPIC_API_KEY").unwrap();
        let session = load_auth_session_from_path(&path).unwrap();
        assert_eq!(session.provider, "anthropic");
        assert_eq!(session.env_var, "ANTHROPIC_API_KEY");

        clear_auth_session_at(&path).unwrap();
        assert!(load_auth_session_from_path(&path).is_none());
    }

    #[test]
    fn loads_image_as_base64_payload() {
        let root = unique_temp_dir("image-load");
        let path = root.join("sample.png");
        fs::write(&path, [0_u8, 1, 2, 3]).unwrap();

        let image = load_llm_image(&path).unwrap();
        assert_eq!(image.media_type, "image/png");
        assert_eq!(image.data_base64, "AAECAw==");
    }

    #[test]
    fn serializes_usage_json_payload() {
        let value = usage_to_json(&LlmUsage {
            provider: "openai".to_string(),
            input_tokens: Some(12),
            output_tokens: Some(5),
            total_tokens: Some(17),
            cache_creation_input_tokens: None,
            cache_read_input_tokens: Some(3),
            reasoning_tokens: Some(2),
            raw: Some(serde_json::json!({"prompt_tokens": 12})),
        });

        assert_eq!(value["provider"], "openai");
        assert_eq!(value["input_tokens"], 12);
        assert_eq!(value["output_tokens"], 5);
        assert_eq!(value["total_tokens"], 17);
        assert_eq!(value["cache_read_input_tokens"], 3);
        assert_eq!(value["reasoning_tokens"], 2);
        assert_eq!(value["raw"]["prompt_tokens"], 12);
    }

    #[test]
    fn parses_generated_agent_from_json_fence() {
        let raw = "```json\n{\"name\":\"reviewer\",\"description\":\"Reviews diffs.\",\"prompt\":\"Review code carefully.\"}\n```";
        let agent = parse_generated_agent(raw).unwrap();
        assert_eq!(agent.name, "reviewer");
        assert_eq!(agent.description, "Reviews diffs.");
        assert_eq!(agent.prompt, "Review code carefully.");
    }

    #[test]
    fn falls_back_when_generated_agent_payload_is_invalid() {
        assert!(parse_generated_agent("not json").is_err());
        let agent = fallback_generated_agent();
        assert!(agent.name.starts_with("generated-"));
        assert!(!agent.prompt.is_empty());
    }
}
