use crate::agent::{AgentOutput, AgentRunner, AgentStore, StoredAgent};
use crate::auth_store;
use crate::checkpoint::{format_checkpoint_diff, format_checkpoint_list, CheckpointStore};
use crate::config::{Config, PermissionsConfig};
use crate::forge::{self, ForgeProvider, ReviewAction};
use crate::llm::{
    AnthropicBackend, GoogleBackend, LlmBackend, LlmClient, LlmImage, LlmProvider, LlmRequest,
    LlmStreamEvent, LlmUsage, OllamaBackend, OpenAiBackend,
};
use crate::mcp::{list_tools_http, list_tools_stdio, McpServerConfig, McpStore};
use crate::memory::{format_memory_context, format_memory_entries, MemoryStore};
use crate::review::{build_review_prompt, ReviewOptions};
use crate::session::{Session, SessionStore};
use crate::tools::{ToolExecutor, ToolInput, ToolPolicy, ToolResult};
use crate::tui::{App, AppInit};
use anyhow::{anyhow, Result};
use base64::Engine;
use chrono::Utc;
use clap::{Parser, Subcommand};
use futures_util::StreamExt;
use serde::Serialize;
use serde_json::json;
use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::time::Instant;

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

    /// チェックポイント管理
    Checkpoint {
        #[command(subcommand)]
        command: CheckpointCommands,
    },

    /// 永続メモリー管理
    Memory {
        #[command(subcommand)]
        command: MemoryCommands,
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

    /// GitHub/GitLab issue operations through gh/glab
    Issue {
        /// Hosting provider
        #[arg(long, value_enum, default_value_t = ForgeProvider::Github)]
        provider: ForgeProvider,

        /// Repository selector (OWNER/REPO or GROUP/PROJECT)
        #[arg(short = 'R', long)]
        repo: Option<String>,

        #[command(subcommand)]
        command: IssueCommands,
    },

    /// GitHub PR / GitLab MR operations through gh/glab
    Pr {
        /// Hosting provider
        #[arg(long, value_enum, default_value_t = ForgeProvider::Github)]
        provider: ForgeProvider,

        /// Repository selector (OWNER/REPO or GROUP/PROJECT)
        #[arg(short = 'R', long)]
        repo: Option<String>,

        #[command(subcommand)]
        command: PrCommands,
    },

    /// GitHub/GitLab label operations through gh/glab
    Label {
        /// Hosting provider
        #[arg(long, value_enum, default_value_t = ForgeProvider::Github)]
        provider: ForgeProvider,

        /// Repository selector (OWNER/REPO or GROUP/PROJECT)
        #[arg(short = 'R', long)]
        repo: Option<String>,

        #[command(subcommand)]
        command: LabelCommands,
    },

    /// ローカル性能基準を計測
    Perf {
        /// 出力形式 (text/json)
        #[arg(long, default_value = "text")]
        format: String,

        /// 基準未達をエラーにする
        #[arg(long)]
        strict: bool,
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
pub enum CheckpointCommands {
    /// Create a checkpoint for paths
    Create {
        /// Files to snapshot
        paths: Vec<PathBuf>,
        /// Checkpoint reason
        #[arg(short, long, default_value = "manual")]
        reason: String,
    },
    /// List checkpoints
    List,
    /// Show diff from checkpoint to current files
    Diff {
        /// Checkpoint ID; omit to use latest
        id: Option<String>,
    },
    /// Restore files from checkpoint
    Restore {
        /// Checkpoint ID; omit to use latest
        id: Option<String>,
    },
}

#[derive(Subcommand, Debug)]
pub enum MemoryCommands {
    /// Add a project memory entry
    Add {
        /// Memory content
        content: Vec<String>,
    },
    /// List project memory entries
    List,
    /// Search project memory entries
    Search {
        /// Search query
        query: Vec<String>,
    },
    /// Remove a project memory entry by ID
    Remove {
        /// Memory ID
        id: String,
    },
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
pub enum IssueCommands {
    /// View an issue
    View {
        /// Issue number/IID
        number: String,
        /// Include comments
        #[arg(long)]
        comments: bool,
    },
    /// Create an issue
    Create {
        /// Issue title
        #[arg(short, long)]
        title: String,
        /// Issue body/description
        #[arg(short, long)]
        body: Option<String>,
        /// Label to add; can be repeated
        #[arg(short, long = "label")]
        labels: Vec<String>,
    },
    /// Add a comment/note to an issue
    Comment {
        /// Issue number/IID
        number: String,
        /// Comment body
        #[arg(short, long)]
        body: String,
    },
    /// Add/remove issue labels
    Labels {
        /// Issue number/IID
        number: String,
        /// Label to add; can be repeated
        #[arg(long = "add")]
        add: Vec<String>,
        /// Label to remove; can be repeated
        #[arg(long = "remove")]
        remove: Vec<String>,
    },
}

#[derive(Subcommand, Debug)]
pub enum PrCommands {
    /// Create a GitHub PR or GitLab MR; remaining args pass through to gh/glab
    Create {
        /// Provider-specific args passed to gh pr create / glab mr create
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Show PR/MR comments
    Comments {
        /// PR number, MR IID, URL, or branch
        target: Option<String>,
    },
    /// Add a PR/MR comment
    Comment {
        /// PR number, MR IID, URL, or branch
        target: Option<String>,
        /// Comment body
        #[arg(short, long)]
        body: String,
    },
    /// Add a PR review or MR approval/note
    Review {
        /// PR number, MR IID, URL, or branch
        target: Option<String>,
        /// Review action
        #[arg(long, value_enum, default_value_t = ReviewAction::Comment)]
        action: ReviewAction,
        /// Review body
        #[arg(short, long)]
        body: Option<String>,
    },
}

#[derive(Subcommand, Debug)]
pub enum LabelCommands {
    /// List labels
    List,
    /// Create a label
    Create {
        /// Label name
        name: String,
        /// Label color
        #[arg(long)]
        color: Option<String>,
        /// Label description
        #[arg(short, long)]
        description: Option<String>,
    },
    /// Edit a label
    Edit {
        /// GitHub label name or GitLab label ID
        name: String,
        /// New label name
        #[arg(long)]
        new_name: Option<String>,
        /// Label color
        #[arg(long)]
        color: Option<String>,
        /// Label description
        #[arg(short, long)]
        description: Option<String>,
    },
    /// Delete a label
    Delete {
        /// Label name
        name: String,
    },
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
    /// URL取得
    WebFetch {
        /// 取得するURL
        url: String,
        /// HTTPメソッド
        #[arg(long, default_value = "GET")]
        method: String,
        /// HTTPヘッダー（Key: Value形式、複数指定可）
        #[arg(long = "header")]
        headers: Vec<String>,
        /// リクエストボディ
        #[arg(long)]
        body: Option<String>,
    },
    /// Web検索
    WebSearch {
        /// 検索クエリ
        query: String,
    },
}

impl Cli {
    pub async fn execute(mut self) -> Result<()> {
        self.apply_startup_overrides()?;
        self.hydrate_stored_auth_token();
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
            Commands::Checkpoint { command } => self.execute_checkpoint_command(command),
            Commands::Memory { command } => self.execute_memory_command(command),
            Commands::Tool { command } => self.execute_tool_command(command).await,
            Commands::Tui => self.execute_tui().await,
            Commands::Review { base, preset } => {
                self.execute_review_command(base.clone(), preset.clone())
                    .await
            }
            Commands::Issue {
                provider,
                repo,
                command,
            } => self.execute_issue_command(*provider, repo.as_deref(), command),
            Commands::Pr {
                provider,
                repo,
                command,
            } => self.execute_pr_command(*provider, repo.as_deref(), command),
            Commands::Label {
                provider,
                repo,
                command,
            } => self.execute_label_command(*provider, repo.as_deref(), command),
            Commands::Perf { format, strict } => self.execute_perf_command(format, *strict),
            Commands::Resume { session_id, last } => {
                self.execute_resume_command(session_id.as_deref(), *last)
                    .await
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

    fn apply_startup_overrides(&mut self) -> Result<()> {
        if let Some(cwd) = self.cwd.as_deref() {
            std::env::set_current_dir(cwd)?;
        }
        self.add_dir = normalize_add_dirs(&self.add_dir)?;
        Ok(())
    }

    fn hydrate_stored_auth_token(&self) {
        if std::env::var(auth_store::passphrase_env_var()).is_err() {
            return;
        }
        let config = self.load_effective_config();
        let provider = if config.model.provider.trim().is_empty() {
            "anthropic"
        } else {
            config.model.provider.as_str()
        };
        let _ = auth_store::hydrate_env_for_provider(provider);
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
                println!("{}", format_session_list(store.list()?));
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

    fn execute_checkpoint_command(&self, command: &CheckpointCommands) -> Result<()> {
        let store = CheckpointStore::new(CheckpointStore::default_root());
        match command {
            CheckpointCommands::Create { paths, reason } => {
                if paths.is_empty() {
                    return Err(anyhow!("checkpoint create requires at least one path"));
                }
                let checkpoint = store.create_for_paths(paths, reason)?;
                println!(
                    "checkpoint created: {} files={}",
                    checkpoint.id,
                    checkpoint.files.len()
                );
                Ok(())
            }
            CheckpointCommands::List => {
                println!("{}", format_checkpoint_list(&store.list()?));
                Ok(())
            }
            CheckpointCommands::Diff { id } => {
                let checkpoint = match id {
                    Some(id) => store.load(id)?,
                    None => store.latest()?.ok_or_else(|| anyhow!("no checkpoints"))?,
                };
                println!("{}", format_checkpoint_diff(&checkpoint));
                Ok(())
            }
            CheckpointCommands::Restore { id } => {
                let restored = match id {
                    Some(id) => Some(store.restore(id)?),
                    None => store.restore_latest()?,
                };
                match restored {
                    Some(checkpoint) => println!(
                        "checkpoint restored: {} files={}",
                        checkpoint.id,
                        checkpoint.files.len()
                    ),
                    None => println!("no checkpoints"),
                }
                Ok(())
            }
        }
    }

    fn execute_memory_command(&self, command: &MemoryCommands) -> Result<()> {
        let store = MemoryStore::new(MemoryStore::default_path());
        match command {
            MemoryCommands::Add { content } => {
                let content = content.join(" ");
                let entry = store.add(&content)?;
                println!("memory added: {}", entry.id);
                Ok(())
            }
            MemoryCommands::List => {
                println!("{}", format_memory_entries(&store.list()?));
                Ok(())
            }
            MemoryCommands::Search { query } => {
                println!(
                    "{}",
                    format_memory_entries(&store.search(&query.join(" "))?)
                );
                Ok(())
            }
            MemoryCommands::Remove { id } => {
                if store.remove(id)? {
                    println!("memory removed: {}", id);
                } else {
                    println!("memory not found: {}", id);
                }
                Ok(())
            }
        }
    }

    fn execute_issue_command(
        &self,
        provider: ForgeProvider,
        repo: Option<&str>,
        command: &IssueCommands,
    ) -> Result<()> {
        let cmd = match command {
            IssueCommands::View { number, comments } => {
                forge::issue_view(provider, number, *comments, repo)
            }
            IssueCommands::Create {
                title,
                body,
                labels,
            } => forge::issue_create(provider, title, body.as_deref(), labels, repo),
            IssueCommands::Comment { number, body } => {
                forge::issue_comment(provider, number, body, repo)
            }
            IssueCommands::Labels {
                number,
                add,
                remove,
            } => forge::issue_labels(provider, number, add, remove, repo),
        };
        println!("{}", cmd.run_in_dir(Path::new(".")));
        Ok(())
    }

    fn execute_pr_command(
        &self,
        provider: ForgeProvider,
        repo: Option<&str>,
        command: &PrCommands,
    ) -> Result<()> {
        let cmd = match command {
            PrCommands::Create { args } => forge::pr_create(provider, args, repo),
            PrCommands::Comments { target } => {
                forge::pr_comments(provider, target.as_deref(), repo)
            }
            PrCommands::Comment { target, body } => {
                forge::pr_comment(provider, target.as_deref(), body, repo)
            }
            PrCommands::Review {
                target,
                action,
                body,
            } => forge::pr_review(provider, target.as_deref(), *action, body.as_deref(), repo),
        };
        println!("{}", cmd.run_in_dir(Path::new(".")));
        Ok(())
    }

    fn execute_label_command(
        &self,
        provider: ForgeProvider,
        repo: Option<&str>,
        command: &LabelCommands,
    ) -> Result<()> {
        let cmd = match command {
            LabelCommands::List => forge::label_list(provider, repo),
            LabelCommands::Create {
                name,
                color,
                description,
            } => forge::label_create(
                provider,
                name,
                color.as_deref(),
                description.as_deref(),
                repo,
            ),
            LabelCommands::Edit {
                name,
                new_name,
                color,
                description,
            } => forge::label_edit(
                provider,
                name,
                new_name.as_deref(),
                color.as_deref(),
                description.as_deref(),
                repo,
            ),
            LabelCommands::Delete { name } => forge::label_delete(provider, name, repo),
        };
        println!("{}", cmd.run_in_dir(Path::new(".")));
        Ok(())
    }

    async fn execute_resume_command(&self, session_id: Option<&str>, last: bool) -> Result<()> {
        let store = SessionStore::new(SessionStore::default_root()?);
        if last {
            if let Some(session) = store.latest()? {
                return self.execute_tui_with_session(Some(session)).await;
            }
            println!("no sessions");
            return Ok(());
        }

        if let Some(session_id) = session_id {
            let session = store.load(session_id)?;
            return self.execute_tui_with_session(Some(session)).await;
        }

        println!("{}", format_resume_selection_prompt(store.list()?));
        Ok(())
    }

    async fn execute_auth_command(&self, command: &AuthCommands) -> Result<()> {
        let config = self.load_effective_config();
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
                let token =
                    std::env::var(env_name).map_err(|_| anyhow!("{} is not set", env_name))?;
                if token.trim().is_empty() {
                    return Err(anyhow!("{} is not set", env_name));
                }
                save_auth_session(provider_name, env_name, &token)?;
                println!(
                    "auth ready: provider={} token_store=encrypted passphrase_env={}",
                    provider_name,
                    auth_store::passphrase_env_var()
                );
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
                            "session=ready provider={} encrypted={} updated_at={}",
                            session.provider, session.encrypted, session.updated_at
                        )
                    })
                    .unwrap_or_else(|| "session=none".to_string());
                let token_status = auth_store::token_store_status(provider_name);
                println!(
                    "provider: {}\n{}\n{}\n{}",
                    provider_name, env_status, session_status, token_status
                );
                Ok(())
            }
        }
    }

    async fn execute_tool_command(&self, command: &ToolCommands) -> Result<()> {
        let config = self.load_effective_config();
        let policy = ToolPolicy::from_config(&config);
        let executor = ToolExecutor::with_policy(policy);
        let result = match command {
            ToolCommands::Read { path } => executor.execute(ToolInput::Read {
                path: path.clone(),
                offset: None,
                limit: None,
            })?,
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
            ToolCommands::WebFetch {
                url,
                method,
                headers,
                body,
            } => executor.execute(ToolInput::WebFetch {
                url: url.clone(),
                method: method.clone(),
                headers: parse_header_args(headers)?,
                body: body.clone(),
            })?,
            ToolCommands::WebSearch { query } => executor.execute(ToolInput::WebSearch {
                query: query.clone(),
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

        let config = self.load_effective_config();
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

    fn execute_perf_command(&self, format: &str, strict: bool) -> Result<()> {
        if !matches!(format, "json" | "text") {
            return Err(anyhow!("unsupported perf format: {}", format));
        }

        let report = run_performance_checks()?;
        match format {
            "json" => println!("{}", serde_json::to_string_pretty(&report)?),
            "text" => println!("{}", format_performance_report(&report)),
            _ => unreachable!("format was validated before performance checks"),
        }
        if strict && !report.passed() {
            return Err(anyhow!("performance baseline failed"));
        }
        Ok(())
    }

    async fn execute_tui(&self) -> Result<()> {
        self.execute_tui_with_session(None).await
    }

    async fn execute_tui_with_session(&self, initial_session: Option<Session>) -> Result<()> {
        let banner = "👺 Tengu - Interactive mode".to_string();
        let config = self.load_effective_config();
        let (client, model_name) = self.resolve_llm_with_config(&config)?;
        let policy = ToolPolicy::from_config(&config);
        let status_model = model_name.clone();
        let runner = std::sync::Arc::new(AgentRunner::new(client, model_name, policy));

        // Set system prompt for TUI mode
        let (system_prompt, _sources) = self.resolve_system_prompt()?;
        let effective_prompt = system_prompt.unwrap_or_else(default_system_prompt);
        runner.set_system_prompt(effective_prompt);

        let handle = tokio::runtime::Handle::current();
        let status_build = option_env!("BUILD_TIMESTAMP")
            .unwrap_or("unknown")
            .to_string();
        let (result_tx, result_rx) = std::sync::mpsc::channel();
        let mut app = App::new(AppInit {
            runner,
            handle,
            banner,
            status_model,
            status_build,
            result_rx,
            result_tx,
            initial_session,
        });
        app.set_initial_added_dirs(self.add_dir.clone());
        app.run()?;
        Ok(())
    }

    async fn execute_headless(&self) -> Result<()> {
        let (system_prompt, sources) = self.resolve_system_prompt()?;
        self.log_system_prompt_sources(&sources, system_prompt.as_deref());
        if let Some(prompt) = self.prompt.as_deref() {
            let request = build_headless_request(prompt, system_prompt.as_deref(), &self.image)?;
            if self.output_format == "stream-json" {
                let config = self.load_effective_config();
                let (client, model_name) = self.resolve_llm_with_config(&config)?;
                let workspace_context = self.additional_workspace_context();
                if !request.images.is_empty() {
                    let request = request_with_prompt_context(&request, &workspace_context);
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
                let effective_prompt = system_prompt.clone().unwrap_or_else(default_system_prompt);
                runner.set_system_prompt(effective_prompt);

                // Set up tool event handler for stream-json output
                let tx_json = std::sync::Arc::new(std::sync::Mutex::new(()));
                let _lock = tx_json; // ensure serialization
                runner.set_tool_event_handler(std::sync::Arc::new(move |event| {
                    Box::pin(async move {
                        match event {
                            crate::agent::ToolEvent::Text(text) => {
                                println!(
                                    "{}",
                                    json!({ "type": "chunk", "mode": "llm", "delta": text })
                                );
                            }
                            crate::agent::ToolEvent::ToolCall { name, input } => {
                                println!(
                                    "{}",
                                    json!({
                                        "type": "tool_call",
                                        "mode": "tool",
                                        "name": name,
                                        "input": input
                                    })
                                );
                            }
                            crate::agent::ToolEvent::ToolResult {
                                name,
                                result,
                                is_error,
                            } => {
                                println!(
                                    "{}",
                                    json!({
                                        "type": "tool_result",
                                        "mode": "tool",
                                        "name": name,
                                        "result": result,
                                        "is_error": is_error
                                    })
                                );
                            }
                            crate::agent::ToolEvent::Usage(usage) => {
                                println!(
                                    "{}",
                                    json!({
                                        "type": "usage",
                                        "mode": "llm",
                                        "usage": {
                                            "provider": &usage.provider,
                                            "input_tokens": usage.input_tokens,
                                            "output_tokens": usage.output_tokens,
                                        }
                                    })
                                );
                            }
                            crate::agent::ToolEvent::Thinking(text) => {
                                println!(
                                    "{}",
                                    json!({ "type": "thinking", "mode": "llm", "delta": text })
                                );
                            }
                        }
                    })
                }));

                let request = request_with_prompt_context(&request, &workspace_context);
                println!("{}", json!({ "type": "start", "mode": "agent" }));
                match runner.run_prompt(&request.prompt).await {
                    Ok(result) => {
                        println!(
                            "{}",
                            json!({
                                "type": "result",
                                "mode": "agent",
                                "text": result.final_text,
                                "turns": result.total_turns
                            })
                        );
                    }
                    Err(err) => {
                        println!(
                            "{}",
                            json!({ "type": "error", "mode": "agent", "message": err.to_string() })
                        );
                    }
                }
                println!("{}", json!({ "type": "end", "mode": "agent" }));
                return Ok(());
            }
        }
        let message = format!("Headless mode with prompt: {:?}", self.prompt);
        self.print_output("headless", &message, self.prompt.as_deref());
        if let Some(prompt) = self.prompt.as_deref() {
            let request = build_headless_request(prompt, system_prompt.as_deref(), &self.image)?;
            let config = self.load_effective_config();
            let (client, model_name) = self.resolve_llm_with_config(&config)?;
            let workspace_context = self.additional_workspace_context();
            if !request.images.is_empty() {
                let request = request_with_prompt_context(&request, &workspace_context);
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
            let effective_prompt = system_prompt.clone().unwrap_or_else(default_system_prompt);
            runner.set_system_prompt(effective_prompt);

            // Show tool events in headless text mode
            let verbose = self.verbose;
            runner.set_tool_event_handler(std::sync::Arc::new(move |event| {
                Box::pin(async move {
                    match event {
                        crate::agent::ToolEvent::Text(_) => {
                            // Text will be printed at the end via final_text
                        }
                        crate::agent::ToolEvent::ToolCall { name, input } => {
                            if verbose {
                                let input_str = serde_json::to_string_pretty(&input)
                                    .unwrap_or_else(|_| format!("{:?}", input));
                                eprintln!("[tool:{}] {}", name, input_str);
                            }
                        }
                        crate::agent::ToolEvent::ToolResult { name, is_error, .. } => {
                            if verbose {
                                let status = if is_error { "ERROR" } else { "OK" };
                                eprintln!("[tool:{}] {}", name, status);
                            }
                        }
                        crate::agent::ToolEvent::Usage(usage) => {
                            if verbose {
                                eprintln!(
                                    "[usage] in={} out={}",
                                    usage.input_tokens.unwrap_or(0),
                                    usage.output_tokens.unwrap_or(0),
                                );
                            }
                        }
                        crate::agent::ToolEvent::Thinking(text) => {
                            if verbose {
                                eprintln!("[thinking] {}", text);
                            }
                        }
                    }
                })
            }));
            let request = request_with_prompt_context(&request, &workspace_context);
            let result = runner.run_prompt(&request.prompt).await?;
            self.print_output("llm", &result.final_text, Some(prompt));
        }
        Ok(())
    }

    fn load_effective_config(&self) -> Config {
        let mut config = load_config().unwrap_or_default();
        self.apply_cli_config_overrides(&mut config);
        config
    }

    fn apply_cli_config_overrides(&self, config: &mut Config) {
        if let Some(allowed_tools) = self.allowed_tools.as_deref() {
            let tools = parse_allowed_tools(allowed_tools);
            let permissions = config.permissions.get_or_insert(PermissionsConfig {
                approval_policy: None,
                allowed_tools: None,
                deny: None,
            });
            permissions.allowed_tools = Some(tools);
        }
    }

    fn additional_workspace_context(&self) -> String {
        if self.add_dir.is_empty() {
            return String::new();
        }
        let dirs = self
            .add_dir
            .iter()
            .map(|path| format!("- {}", path.display()))
            .collect::<Vec<_>>()
            .join("\n");
        format!("Additional workspace directories:\n{}", dirs)
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
            read_system_prompt_candidates(&mut sources, &mut parts)?;
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

fn read_system_prompt_candidates(sources: &mut Vec<String>, parts: &mut Vec<String>) -> Result<()> {
    for (scope, path) in system_prompt_candidate_paths() {
        if let Some(content) = read_optional_file(&path)? {
            sources.push(format!("{}:{}", scope, path.display()));
            parts.push(content);
        }
    }
    append_memory_prompt_context(sources, parts, MemoryStore::default_path())?;
    Ok(())
}

fn append_memory_prompt_context(
    sources: &mut Vec<String>,
    parts: &mut Vec<String>,
    path: PathBuf,
) -> Result<()> {
    let store = MemoryStore::new(path.clone());
    let entries = store.list()?;
    if let Some(context) = format_memory_context(&entries, 20) {
        sources.push(format!("project:{}", path.display()));
        parts.push(context);
    }
    Ok(())
}

fn system_prompt_candidate_paths() -> Vec<(&'static str, PathBuf)> {
    let mut candidates = Vec::new();
    if let Some(home) = std::env::var_os("HOME") {
        let root = PathBuf::from(home).join(".tengu");
        candidates.extend(memory_file_candidates("global", &root));
    }

    candidates.extend(memory_file_candidates(
        "project",
        &PathBuf::from(".").join(".tengu"),
    ));
    candidates.extend(memory_file_candidates(
        "workspace",
        &PathBuf::from(".").join("workspace").join(".tengu"),
    ));
    candidates
}

fn memory_file_candidates(scope: &'static str, root: &Path) -> Vec<(&'static str, PathBuf)> {
    vec![root.join("AGENT.md"), root.join("TENGU.md")]
        .into_iter()
        .map(|path| (scope, path))
        .collect()
}

fn format_resume_selection_prompt(sessions: Vec<Session>) -> String {
    if sessions.is_empty() {
        return "no sessions".to_string();
    }
    format!(
        "{}\n\nResume with `tengu resume <session-id>` or `tengu resume --last`.",
        format_session_list(sessions)
    )
}

fn format_session_list(mut sessions: Vec<Session>) -> String {
    if sessions.is_empty() {
        return "no sessions".to_string();
    }
    sessions.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    sessions
        .iter()
        .map(format_session_entry)
        .collect::<Vec<_>>()
        .join("\n")
}

fn format_session_entry(session: &Session) -> String {
    format!(
        "{} updated={} created={} turns={} logs={} queue={}",
        session.id,
        session.updated_at,
        session.created_at,
        session.conversation.len(),
        session.log_lines.len(),
        session.queue.len()
    )
}

#[derive(Debug, Serialize)]
struct PerformanceReport {
    metrics: Vec<PerformanceMetric>,
}

impl PerformanceReport {
    fn passed(&self) -> bool {
        self.metrics.iter().all(PerformanceMetric::passed)
    }
}

#[derive(Debug, Serialize)]
struct PerformanceMetric {
    name: String,
    value: f64,
    unit: String,
    threshold: f64,
    available: bool,
}

impl PerformanceMetric {
    fn passed(&self) -> bool {
        if !self.available {
            return true;
        }
        self.value <= self.threshold
    }
}

fn run_performance_checks() -> Result<PerformanceReport> {
    let mut metrics = Vec::new();
    metrics.push(measure_ms("startup_path", 500.0, || {
        let _ = load_config();
        let _ = system_prompt_candidate_paths();
        Ok(())
    })?);
    metrics.push(measure_ms("command_dispatch", 100.0, || {
        let mut session = Session::with_id("perf-session".to_string());
        session.updated_at = "2026-01-01T00:00:00Z".to_string();
        let _ = format_session_list(vec![session]);
        Ok(())
    })?);
    metrics.push(measure_file_read_1mb()?);
    metrics.push(memory_metric());
    Ok(PerformanceReport { metrics })
}

fn measure_ms<F>(name: &str, threshold: f64, mut op: F) -> Result<PerformanceMetric>
where
    F: FnMut() -> Result<()>,
{
    let started = Instant::now();
    op()?;
    Ok(PerformanceMetric {
        name: name.to_string(),
        value: started.elapsed().as_secs_f64() * 1000.0,
        unit: "ms".to_string(),
        threshold,
        available: true,
    })
}

fn measure_file_read_1mb() -> Result<PerformanceMetric> {
    let dir = std::env::temp_dir().join(format!(
        "tengu-perf-{}",
        chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
    ));
    fs::create_dir_all(&dir)?;
    let path = dir.join("one-mib.txt");
    let mut file = fs::File::create(&path)?;
    let chunk = "0123456789abcdef\n".repeat(64);
    while file.metadata()?.len() < 1024 * 1024 {
        file.write_all(chunk.as_bytes())?;
    }
    drop(file);

    let metric = measure_ms("file_read_1mb", 50.0, || {
        let _ = fs::read_to_string(&path)?;
        Ok(())
    });
    let _ = fs::remove_file(&path);
    let _ = fs::remove_dir(&dir);
    metric
}

fn memory_metric() -> PerformanceMetric {
    match current_rss_mb() {
        Some(value) => PerformanceMetric {
            name: "rss_memory".to_string(),
            value,
            unit: "MB".to_string(),
            threshold: 200.0,
            available: true,
        },
        None => PerformanceMetric {
            name: "rss_memory".to_string(),
            value: 0.0,
            unit: "MB".to_string(),
            threshold: 200.0,
            available: false,
        },
    }
}

fn current_rss_mb() -> Option<f64> {
    if let Ok(status) = fs::read_to_string("/proc/self/status") {
        for line in status.lines() {
            if let Some(rest) = line.strip_prefix("VmRSS:") {
                let kb = rest.split_whitespace().next()?.parse::<f64>().ok()?;
                return Some(kb / 1024.0);
            }
        }
    }

    let pid = std::process::id().to_string();
    let output = std::process::Command::new("ps")
        .args(["-o", "rss=", "-p", &pid])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let rss_kb = String::from_utf8_lossy(&output.stdout)
        .trim()
        .parse::<f64>()
        .ok()?;
    Some(rss_kb / 1024.0)
}

fn format_performance_report(report: &PerformanceReport) -> String {
    let mut lines = vec!["performance baseline:".to_string()];
    for metric in &report.metrics {
        if metric.available {
            let status = if metric.passed() { "ok" } else { "fail" };
            lines.push(format!(
                "{} {}={:.2}{} threshold={:.2}{}",
                status, metric.name, metric.value, metric.unit, metric.threshold, metric.unit
            ));
        } else {
            lines.push(format!(
                "skip {} unavailable threshold={:.2}{}",
                metric.name, metric.threshold, metric.unit
            ));
        }
    }
    lines.push(format!(
        "overall: {}",
        if report.passed() { "ok" } else { "fail" }
    ));
    lines.join("\n")
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
                .or_else(|| config.model.local.as_ref().and_then(|p| p.base_url.clone()))
                .or_else(|| config.model.backend_url.clone())
                .unwrap_or_else(|| "http://localhost:11434".to_string());
            Box::new(OllamaBackend::new(base_url))
        }
        LlmProvider::Anthropic => Box::new(AnthropicBackend::new(
            config
                .model
                .anthropic
                .as_ref()
                .and_then(|p| p.base_url.clone())
                .or_else(|| config.model.backend_url.clone()),
            config.model.effective_max_tokens(),
        )),
        LlmProvider::OpenAI => Box::new(OpenAiBackend::new(
            config
                .model
                .openai
                .as_ref()
                .and_then(|p| p.base_url.clone())
                .or_else(|| config.model.backend_url.clone()),
            config.model.effective_max_tokens(),
        )),
        LlmProvider::Google => Box::new(GoogleBackend::new(
            config
                .model
                .google
                .as_ref()
                .and_then(|p| p.base_url.clone())
                .or_else(|| config.model.backend_url.clone()),
        )),
    }
}

impl Cli {
    async fn generate_agent_with_llm(&self, store: &AgentStore) -> Result<()> {
        let config = self.load_effective_config();
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

    #[allow(dead_code)]
    fn print_tool_result(&self, output: &AgentOutput) {
        let Some(result) = output.tool_result.as_ref() else {
            return;
        };
        self.print_output("tool", &format_tool_result(result), None);
        match self.apply_preview_write_with_effective_config(result) {
            Ok(Some(applied)) => {
                self.print_output("tool", &format_tool_result(&applied), None);
            }
            Ok(None) => {}
            Err(err) => {
                eprintln!("failed to apply write: {}", err);
            }
        }
    }

    fn apply_preview_write_with_effective_config(
        &self,
        result: &ToolResult,
    ) -> Result<Option<ToolResult>> {
        let config = self.load_effective_config();
        let policy = ToolPolicy::from_config(&config);
        let executor = ToolExecutor::with_policy(policy);
        apply_preview_write(&executor, result)
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

fn auth_env_var_for_provider(provider: &str) -> Option<&'static str> {
    match provider.trim().to_ascii_lowercase().as_str() {
        "anthropic" => Some("ANTHROPIC_API_KEY"),
        "openai" => Some("OPENAI_API_KEY"),
        "google" | "gemini" => Some("GOOGLE_API_KEY"),
        _ => None,
    }
}

fn save_auth_session(provider: &str, env_var: &str, token: &str) -> Result<()> {
    auth_store::save_login(provider, env_var, token)?;
    Ok(())
}

#[cfg(test)]
fn auth_session_path_from_home(home: &Path) -> PathBuf {
    auth_store::session_path_from_home(home)
}

#[cfg(test)]
fn auth_token_store_path_from_home(home: &Path) -> PathBuf {
    auth_store::token_store_path_from_home(home)
}

#[cfg(test)]
fn save_auth_session_at(path: &Path, provider: &str, env_var: &str, token: &str) -> Result<()> {
    let token_store = path
        .parent()
        .map(|parent| parent.join("tokens.json"))
        .unwrap_or_else(|| PathBuf::from("tokens.json"));
    auth_store::save_login_at_with_passphrase(
        path,
        &token_store,
        provider,
        env_var,
        token,
        b"test passphrase",
    )?;
    Ok(())
}

fn load_auth_session() -> Option<auth_store::AuthSession> {
    auth_store::load_session()
}

#[cfg(test)]
fn load_auth_session_from_path(path: &Path) -> Option<auth_store::AuthSession> {
    auth_store::load_session_from_path(path)
}

fn clear_auth_session() -> Result<()> {
    auth_store::clear()
}

#[cfg(test)]
fn clear_auth_session_at(path: &Path) -> Result<()> {
    let token_store = path
        .parent()
        .map(|parent| parent.join("tokens.json"))
        .unwrap_or_else(|| PathBuf::from("tokens.json"));
    auth_store::clear_at(path, &token_store)
}

fn default_system_prompt() -> String {
    let cwd = std::env::current_dir()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| ".".to_string());

    format!(
        r#"You are Tengu (天狗), an expert autonomous coding agent. You solve software engineering tasks by reading, writing, and executing code.

# Environment
- Working directory: {cwd}
- Platform: {platform}
- You can execute any shell command via the Bash tool.

# Available Tools
1. **Read** - Read file contents (supports offset/limit for large files)
2. **Edit** - Replace exact strings in files (old_string must be unique)
3. **Write** - Create new files or completely overwrite existing ones
4. **Bash** - Execute shell commands (sh -c). Use for builds, tests, git, package management.
5. **Grep** - Search file contents using regex patterns
6. **Glob** - Find files matching glob patterns (e.g., "**/*.rs")
7. **ListFiles** - List directory contents with file types and sizes

# Guidelines
- **Read before editing**: Always read a file before modifying it.
- **Edit over Write**: Use Edit for partial changes. Use Write only for new files.
- **Verify changes**: After making changes, verify by reading the file or running tests.
- **Iterative approach**: If a build/test fails, read the error, fix the issue, and retry.
- **Be concise**: Focus on solving the task. Explain only when asked.
- **Path handling**: Use absolute paths when possible. The Edit tool requires old_string to appear exactly once.
- **Error handling**: If a tool returns an error, analyze the error and try a different approach.
- **Multi-step tasks**: Break complex tasks into steps. Use Bash for builds and tests between steps.

# Important Rules
- Never guess file contents. Always read first.
- When editing, provide enough context in old_string to uniquely identify the target.
- For shell commands, prefer single commands. Use && to chain dependent commands.
- If you're unsure about the project structure, use ListFiles and Glob to explore first."#,
        cwd = cwd,
        platform = std::env::consts::OS,
    )
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

fn request_with_prompt_context(request: &LlmRequest, context: &str) -> LlmRequest {
    if context.trim().is_empty() {
        return request.clone();
    }
    LlmRequest {
        prompt: format!(
            "Conversation context:\n{}\n\nUser request:\n{}",
            context, request.prompt
        ),
        images: request.images.clone(),
    }
}

fn parse_allowed_tools(raw: &str) -> Vec<String> {
    raw.split(',')
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

fn parse_header_args(raw_headers: &[String]) -> Result<Vec<(String, String)>> {
    raw_headers
        .iter()
        .map(|header| {
            let (key, value) = header
                .split_once(':')
                .ok_or_else(|| anyhow!("invalid header, expected `Key: Value`: {}", header))?;
            let key = key.trim();
            if key.is_empty() {
                return Err(anyhow!("invalid header, key is empty: {}", header));
            }
            Ok((key.to_string(), value.trim().to_string()))
        })
        .collect()
}

fn normalize_add_dirs(paths: &[PathBuf]) -> Result<Vec<PathBuf>> {
    let cwd = std::env::current_dir()?;
    let mut normalized = Vec::new();
    for path in paths {
        let resolved = if path.is_absolute() {
            path.clone()
        } else {
            cwd.join(path)
        };
        if !normalized.iter().any(|existing| existing == &resolved) {
            normalized.push(resolved);
        }
    }
    Ok(normalized)
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
    fn parses_forge_pr_passthrough_args() {
        let cli = Cli::try_parse_from([
            "tengu",
            "pr",
            "--provider",
            "gitlab",
            "create",
            "--fill",
            "--draft",
        ])
        .unwrap();
        assert!(matches!(
            cli.command,
            Some(Commands::Pr {
                provider: ForgeProvider::Gitlab,
                command: PrCommands::Create { args },
                ..
            }) if args == vec!["--fill".to_string(), "--draft".to_string()]
        ));
    }

    #[test]
    fn parses_forge_issue_and_label_commands() {
        let issue = Cli::try_parse_from([
            "tengu",
            "issue",
            "create",
            "--title",
            "bug",
            "--body",
            "broken",
            "--label",
            "regression",
        ])
        .unwrap();
        assert!(matches!(
            issue.command,
            Some(Commands::Issue {
                provider: ForgeProvider::Github,
                command: IssueCommands::Create { title, body, labels },
                ..
            }) if title == "bug"
                && body == Some("broken".to_string())
                && labels == vec!["regression".to_string()]
        ));

        let label = Cli::try_parse_from(["tengu", "label", "delete", "obsolete"]).unwrap();
        assert!(matches!(
            label.command,
            Some(Commands::Label {
                command: LabelCommands::Delete { name },
                ..
            }) if name == "obsolete"
        ));
    }

    #[test]
    fn parses_checkpoint_commands() {
        let create = Cli::try_parse_from([
            "tengu",
            "checkpoint",
            "create",
            "src/main.rs",
            "--reason",
            "before refactor",
        ])
        .unwrap();
        assert!(matches!(
            create.command,
            Some(Commands::Checkpoint {
                command: CheckpointCommands::Create { paths, reason }
            }) if paths == vec![PathBuf::from("src/main.rs")]
                && reason == "before refactor"
        ));

        let restore = Cli::try_parse_from(["tengu", "checkpoint", "restore", "cp-1"]).unwrap();
        assert!(matches!(
            restore.command,
            Some(Commands::Checkpoint {
                command: CheckpointCommands::Restore { id }
            }) if id == Some("cp-1".to_string())
        ));
    }

    #[test]
    fn parses_memory_commands() {
        let add = Cli::try_parse_from(["tengu", "memory", "add", "Use", "cargo", "test"]).unwrap();
        assert!(matches!(
            add.command,
            Some(Commands::Memory {
                command: MemoryCommands::Add { content }
            }) if content == vec!["Use".to_string(), "cargo".to_string(), "test".to_string()]
        ));

        let search = Cli::try_parse_from(["tengu", "memory", "search", "cargo"]).unwrap();
        assert!(matches!(
            search.command,
            Some(Commands::Memory {
                command: MemoryCommands::Search { query }
            }) if query == vec!["cargo".to_string()]
        ));
    }

    #[test]
    fn builds_headless_request_with_system_prompt() {
        let request = build_headless_request("hello", Some("system"), &[]).unwrap();
        assert!(request.prompt.contains("System instructions:"));
        assert!(request.prompt.contains("User request:"));
        assert!(request.images.is_empty());
    }

    #[test]
    fn resolves_agent_and_tengu_memory_files_in_hierarchy() {
        let home = unique_temp_dir("prompt-home");
        let project = unique_temp_dir("prompt-project");
        let original_home = std::env::var_os("HOME");
        let original_dir = std::env::current_dir().unwrap();

        fs::create_dir_all(home.join(".tengu")).unwrap();
        fs::write(home.join(".tengu").join("AGENT.md"), "GLOBAL AGENT").unwrap();
        fs::write(home.join(".tengu").join("TENGU.md"), "GLOBAL TENGU").unwrap();

        fs::create_dir_all(project.join(".tengu")).unwrap();
        fs::write(project.join(".tengu").join("AGENT.md"), "PROJECT AGENT").unwrap();
        fs::write(project.join(".tengu").join("TENGU.md"), "PROJECT TENGU").unwrap();

        fs::create_dir_all(project.join("workspace").join(".tengu")).unwrap();
        fs::write(
            project.join("workspace").join(".tengu").join("AGENT.md"),
            "WORKSPACE AGENT",
        )
        .unwrap();
        fs::write(
            project.join("workspace").join(".tengu").join("TENGU.md"),
            "WORKSPACE TENGU",
        )
        .unwrap();

        std::env::set_var("HOME", &home);
        std::env::set_current_dir(&project).unwrap();

        let cli = test_cli_with_allowed_tools("");
        let (prompt, sources) = cli.resolve_system_prompt().unwrap();

        std::env::set_current_dir(original_dir).unwrap();
        if let Some(home) = original_home {
            std::env::set_var("HOME", home);
        } else {
            std::env::remove_var("HOME");
        }

        assert_eq!(
            prompt.unwrap(),
            [
                "GLOBAL AGENT",
                "GLOBAL TENGU",
                "PROJECT AGENT",
                "PROJECT TENGU",
                "WORKSPACE AGENT",
                "WORKSPACE TENGU"
            ]
            .join("\n\n")
        );
        assert_eq!(sources.len(), 6);
        assert!(sources[0].contains("global:"));
        assert!(sources[0].contains("AGENT.md"));
        assert!(sources[5].contains("workspace:"));
        assert!(sources[5].contains("TENGU.md"));
    }

    #[test]
    fn includes_project_memory_in_system_prompt_candidates() {
        let project = unique_temp_dir("prompt-memory-project");
        let memory_path = project.join(".tengu").join("memory.json");

        let store = MemoryStore::new(memory_path.clone());
        store
            .add("Use cargo test before marking work complete")
            .unwrap();

        let mut sources = Vec::new();
        let mut parts = Vec::new();
        append_memory_prompt_context(&mut sources, &mut parts, memory_path).unwrap();

        assert!(sources
            .iter()
            .any(|source| source.contains(".tengu/memory.json")));
        assert!(parts.iter().any(|part| part.contains("Project memory:")));
        assert!(parts.iter().any(|part| part.contains("cargo test")));
    }

    #[test]
    fn parses_allowed_tools_csv() {
        assert_eq!(
            parse_allowed_tools("Read, Write, Shell(cargo *)"),
            vec![
                "Read".to_string(),
                "Write".to_string(),
                "Shell(cargo *)".to_string()
            ]
        );
        assert!(parse_allowed_tools(" , ").is_empty());
    }

    #[test]
    fn parses_webfetch_header_args() {
        assert_eq!(
            parse_header_args(&[
                "Accept: text/html".to_string(),
                "X-Test: value:with:colon".to_string()
            ])
            .unwrap(),
            vec![
                ("Accept".to_string(), "text/html".to_string()),
                ("X-Test".to_string(), "value:with:colon".to_string())
            ]
        );
        assert!(parse_header_args(&["missing-colon".to_string()]).is_err());
        assert!(parse_header_args(&[": value".to_string()]).is_err());
    }

    #[test]
    fn applies_allowed_tools_override_to_config() {
        let cli = test_cli_with_allowed_tools("Read,Write");
        let mut config = Config::default();

        cli.apply_cli_config_overrides(&mut config);

        assert_eq!(
            config.permissions.and_then(|p| p.allowed_tools),
            Some(vec!["Read".to_string(), "Write".to_string()])
        );
    }

    #[test]
    fn normalizes_add_dirs_relative_to_current_dir() {
        let root = unique_temp_dir("add-dir-normalize");
        let expected_root = root.canonicalize().unwrap();
        let original = std::env::current_dir().unwrap();
        std::env::set_current_dir(&root).unwrap();

        let dirs = normalize_add_dirs(&[PathBuf::from("src"), PathBuf::from("src")]).unwrap();

        std::env::set_current_dir(original).unwrap();
        assert_eq!(dirs, vec![expected_root.join("src")]);
    }

    #[test]
    fn adds_workspace_context_to_image_request_prompt() {
        let request = LlmRequest {
            prompt: "describe".to_string(),
            images: vec![LlmImage {
                media_type: "image/png".to_string(),
                data_base64: "AA==".to_string(),
            }],
        };

        let with_context =
            request_with_prompt_context(&request, "Additional workspace directories:\n- /tmp/a");

        assert!(with_context.prompt.contains("Conversation context:"));
        assert!(with_context
            .prompt
            .contains("Additional workspace directories:"));
        assert!(with_context.prompt.contains("User request:\ndescribe"));
        assert_eq!(with_context.images.len(), 1);
    }

    #[test]
    fn saves_loads_and_clears_auth_session_file() {
        let root = unique_temp_dir("auth-session");
        let path = auth_session_path_from_home(&root);
        let token_store = auth_token_store_path_from_home(&root);

        save_auth_session_at(&path, "anthropic", "ANTHROPIC_API_KEY", "sk-secret").unwrap();
        let session = load_auth_session_from_path(&path).unwrap();
        assert_eq!(session.provider, "anthropic");
        assert_eq!(session.env_var, "ANTHROPIC_API_KEY");
        assert!(session.encrypted);
        let raw = fs::read_to_string(&token_store).unwrap();
        assert!(!raw.contains("sk-secret"));

        clear_auth_session_at(&path).unwrap();
        assert!(load_auth_session_from_path(&path).is_none());
        assert!(!token_store.exists());
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
    fn formats_session_list_latest_first_with_resume_details() {
        let mut older = Session::with_id("older".to_string());
        older.created_at = "2026-01-01T00:00:00Z".to_string();
        older.updated_at = "2026-01-01T00:00:00Z".to_string();

        let mut newer = Session::with_id("newer".to_string());
        newer.created_at = "2026-01-02T00:00:00Z".to_string();
        newer.updated_at = "2026-01-03T00:00:00Z".to_string();
        newer
            .conversation
            .push(crate::session::SessionConversationTurn {
                role: crate::session::SessionConversationRole::User,
                content: "hello".to_string(),
            });

        let output = format_session_list(vec![older, newer]);
        let lines = output.lines().collect::<Vec<_>>();

        assert!(lines[0].starts_with("newer updated=2026-01-03T00:00:00Z"));
        assert!(lines[0].contains("turns=1"));
        assert!(lines[1].starts_with("older updated=2026-01-01T00:00:00Z"));
    }

    #[test]
    fn formats_resume_selection_prompt_with_next_commands() {
        let mut session = Session::with_id("session-a".to_string());
        session.updated_at = "2026-01-01T00:00:00Z".to_string();

        let output = format_resume_selection_prompt(vec![session]);

        assert!(output.contains("session-a updated=2026-01-01T00:00:00Z"));
        assert!(output.contains("tengu resume <session-id>"));
        assert!(output.contains("tengu resume --last"));
    }

    #[test]
    fn performance_report_tracks_pass_fail() {
        let report = PerformanceReport {
            metrics: vec![
                PerformanceMetric {
                    name: "fast".to_string(),
                    value: 1.0,
                    unit: "ms".to_string(),
                    threshold: 2.0,
                    available: true,
                },
                PerformanceMetric {
                    name: "slow".to_string(),
                    value: 3.0,
                    unit: "ms".to_string(),
                    threshold: 2.0,
                    available: true,
                },
            ],
        };

        assert!(!report.passed());
        let text = format_performance_report(&report);
        assert!(text.contains("ok fast"));
        assert!(text.contains("fail slow"));
        assert!(text.contains("overall: fail"));
    }

    #[test]
    fn performance_report_treats_unavailable_metrics_as_skipped() {
        let report = PerformanceReport {
            metrics: vec![PerformanceMetric {
                name: "rss_memory".to_string(),
                value: 0.0,
                unit: "MB".to_string(),
                threshold: 200.0,
                available: false,
            }],
        };

        assert!(report.passed());
        let text = format_performance_report(&report);
        assert!(text.contains("skip rss_memory unavailable"));
        assert!(text.contains("overall: ok"));
    }

    #[test]
    fn performance_checks_include_required_local_metrics() {
        let report = run_performance_checks().unwrap();
        let names = report
            .metrics
            .iter()
            .map(|metric| metric.name.as_str())
            .collect::<Vec<_>>();

        assert!(names.contains(&"startup_path"));
        assert!(names.contains(&"command_dispatch"));
        assert!(names.contains(&"file_read_1mb"));
        assert!(names.contains(&"rss_memory"));
    }

    #[test]
    fn performance_report_serializes_to_json() {
        let report = PerformanceReport {
            metrics: vec![PerformanceMetric {
                name: "command_dispatch".to_string(),
                value: 1.0,
                unit: "ms".to_string(),
                threshold: 100.0,
                available: true,
            }],
        };

        let json = serde_json::to_value(&report).unwrap();
        assert_eq!(json["metrics"][0]["name"], "command_dispatch");
        assert_eq!(json["metrics"][0]["threshold"], 100.0);
    }

    #[test]
    fn perf_command_rejects_unknown_format_before_running_checks() {
        let cli = test_cli_with_allowed_tools("");
        let err = cli.execute_perf_command("xml", false).unwrap_err();
        assert!(err.to_string().contains("unsupported perf format: xml"));
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

    fn test_cli_with_allowed_tools(allowed_tools: &str) -> Cli {
        Cli {
            prompt: None,
            model: None,
            image: Vec::new(),
            ollama_base_url: None,
            allowed_tools: Some(allowed_tools.to_string()),
            system_prompt: None,
            system_prompt_file: None,
            append_system_prompt: None,
            append_system_prompt_file: None,
            output_format: "text".to_string(),
            agent: None,
            cwd: None,
            add_dir: Vec::new(),
            verbose: false,
            command: None,
        }
    }
}
