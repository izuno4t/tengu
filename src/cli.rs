use anyhow::{anyhow, Result};
use clap::{Parser, Subcommand};
use serde_json::json;
use std::fs;
use std::io::{self, BufRead, IsTerminal, Write};
use std::path::PathBuf;
use crate::agent::{AgentOutput, AgentRunner};
use crate::config::Config;
use crate::llm::{
    AnthropicBackend, GoogleBackend, LlmBackend, LlmClient, LlmProvider, OllamaBackend, OpenAiBackend,
};
use crate::session::{Session, SessionStore};
use crate::tools::{ToolExecutor, ToolInput, ToolResult};
use crate::tui::App;

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
            self.execute_interactive().await
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
        match command {
            McpCommands::Add { name, command } => {
                println!("Add MCP server: {} with command: {:?}", name, command);
                Ok(())
            }
            McpCommands::List => {
                println!("List MCP servers");
                Ok(())
            }
            McpCommands::Remove { name } => {
                println!("Remove MCP server: {}", name);
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
        let executor = ToolExecutor::new();
        let result = match command {
            ToolCommands::Read { path } => executor.execute(ToolInput::Read { path: path.clone() })?,
            ToolCommands::Write { path, content } => {
                let preview = executor.preview_write(path.clone(), content.clone())?;
                println!("{}", format_tool_result(&preview));
                if let Some(applied) = apply_preview_write(&preview)? {
                    println!("{}", format_tool_result(&applied));
                }
                return Ok(());
            }
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
        let mut app = App::new();
        app.run()?;
        Ok(())
    }

    async fn execute_headless(&self) -> Result<()> {
        let (system_prompt, sources) = self.resolve_system_prompt()?;
        self.log_system_prompt_sources(&sources, system_prompt.as_deref());
        let message = format!("Headless mode with prompt: {:?}", self.prompt);
        self.print_output("headless", &message, self.prompt.as_deref());
        if let Some(prompt) = self.prompt.as_deref() {
            let (client, model_name) = self.resolve_llm()?;
            let runner = AgentRunner::new(client, model_name);
            let output = runner.handle_prompt(prompt).await?;
            self.print_output("llm", &output.response.content, Some(prompt));
            self.print_tool_result(&output);
        }
        Ok(())
    }

    async fn execute_interactive(&self) -> Result<()> {
        let (system_prompt, sources) = self.resolve_system_prompt()?;
        self.log_system_prompt_sources(&sources, system_prompt.as_deref());
        self.print_output("interactive", "👺 Tengu - Interactive mode", None);
        self.print_output("interactive", "Type 'exit' to quit", None);
        let (client, model_name) = self.resolve_llm()?;
        self.run_repl(client, model_name).await?;
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

    async fn run_repl(&self, client: LlmClient, model_name: String) -> Result<()> {
        let mut line = String::new();

        if io::stdin().is_terminal() {
            let stdin = io::stdin();
            let mut handle = stdin.lock();
            return run_repl_loop(&mut handle, &mut line, client, model_name).await;
        }

        #[cfg(unix)]
        {
            if let Ok(tty) = fs::File::open("/dev/tty") {
                let mut reader = io::BufReader::new(tty);
                return run_repl_loop(&mut reader, &mut line, client, model_name).await;
            }
        }

        eprintln!("interactive mode requires a TTY; stdin is not a terminal");
        Ok(())
    }

    fn resolve_llm(&self) -> Result<(LlmClient, String)> {
        let config = load_config().unwrap_or_default();
        let provider_name = self
            .model
            .as_deref()
            .or_else(|| config.model.backend.as_deref())
            .unwrap_or("ollama");
        let provider = LlmProvider::from_str(provider_name)?;
        let backend = build_backend(&provider, &config, self.ollama_base_url.clone());
        let model_name = config
            .model
            .name
            .as_deref()
            .ok_or_else(|| anyhow!("model name is not set in config.toml"))?
            .to_string();
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
        LlmProvider::Anthropic => Box::new(AnthropicBackend),
        LlmProvider::OpenAI => Box::new(OpenAiBackend),
        LlmProvider::Google => Box::new(GoogleBackend),
    }
}

async fn run_repl_loop<R: BufRead>(
    reader: &mut R,
    line: &mut String,
    client: LlmClient,
    model_name: String,
) -> Result<()> {
    let runner = AgentRunner::new(client, model_name);
    loop {
        print!("> ");
        io::stdout().flush()?;
        line.clear();

        let bytes = reader.read_line(line)?;
        if bytes == 0 {
            break;
        }

        let input = line.trim();
        if input.is_empty() {
            continue;
        }
        if input.eq_ignore_ascii_case("exit") || input.eq_ignore_ascii_case("quit") {
            break;
        }

        let output = runner.handle_prompt(input).await?;
        println!("{}", output.response.content);
        if let Some(result) = output.tool_result.as_ref() {
            println!("{}", format_tool_result(result));
            if let Some(applied) = apply_preview_write(result)? {
                println!("{}", format_tool_result(&applied));
            }
        }
    }

    Ok(())
}

impl Cli {
    fn print_tool_result(&self, output: &AgentOutput) {
        let Some(result) = output.tool_result.as_ref() else {
            return;
        };
        self.print_output("tool", &format_tool_result(result), None);
        match apply_preview_write(result) {
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

fn apply_preview_write(result: &ToolResult) -> Result<Option<ToolResult>> {
    let ToolResult::PreviewWrite { path, content, .. } = result else {
        return Ok(None);
    };
    let executor = ToolExecutor::new();
    let applied = executor.execute(ToolInput::Write {
        path: path.clone(),
        content: content.clone(),
    })?;
    Ok(Some(applied))
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
