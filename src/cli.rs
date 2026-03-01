use crate::agent::{AgentOutput, AgentRunner};
use crate::config::Config;
use crate::llm::{
    AnthropicBackend, GoogleBackend, LlmBackend, LlmClient, LlmProvider, OllamaBackend,
    OpenAiBackend,
};
use crate::mcp::{list_tools_http, list_tools_stdio, McpServerConfig, McpStore};
use crate::session::{Session, SessionStore};
use crate::tools::{ToolExecutor, ToolInput, ToolPolicy, ToolResult};
use crate::tui::App;
use anyhow::{anyhow, Result};
use clap::{Parser, Subcommand};
use futures_util::StreamExt;
use serde_json::json;
use std::fs;
use std::path::PathBuf;

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
        match command {
            AgentCommands::List => {
                println!("List agents");
                Ok(())
            }
            AgentCommands::Create { name } => {
                println!("Create agent: {}", name);
                Ok(())
            }
            AgentCommands::Remove { name } => {
                println!("Remove agent: {}", name);
                Ok(())
            }
            AgentCommands::Generate => {
                println!("Generate agent with AI assistance");
                Ok(())
            }
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
        match command {
            AuthCommands::Login => {
                println!("Login");
                Ok(())
            }
            AuthCommands::Logout => {
                println!("Logout");
                Ok(())
            }
            AuthCommands::Status => {
                println!("Auth status");
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
            if self.output_format == "stream-json" {
                let config = load_config().unwrap_or_default();
                let (client, model_name) = self.resolve_llm_with_config(&config)?;
                let policy = ToolPolicy::from_config(&config);
                let runner = AgentRunner::new(client, model_name, policy);
                let (mut stream, tool_result) =
                    runner.handle_prompt_stream_with_tool_context(prompt, "").await?;

                println!("{}", json!({ "type": "start", "mode": "llm" }));
                while let Some(chunk) = stream.next().await {
                    match chunk {
                        Ok(text) => {
                            println!(
                                "{}",
                                json!({ "type": "chunk", "mode": "llm", "delta": text })
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
            let config = load_config().unwrap_or_default();
            let (client, model_name) = self.resolve_llm_with_config(&config)?;
            let policy = ToolPolicy::from_config(&config);
            let runner = AgentRunner::new(client, model_name, policy);
            let output = runner.handle_prompt(prompt).await?;
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
        let backend = build_backend(&provider, &config, self.ollama_base_url.clone());
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

fn read_required_file(path: &PathBuf) -> Result<String> {
    Ok(fs::read_to_string(path)?)
}

fn read_optional_file(path: &PathBuf) -> Result<Option<String>> {
    if path.exists() {
        Ok(Some(fs::read_to_string(path)?))
    } else {
        Ok(None)
    }
}
