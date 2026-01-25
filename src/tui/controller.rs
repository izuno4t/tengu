use std::io::{self, Stdout, Write};
use std::sync::{mpsc, Arc};
use std::time::Duration;

use anyhow::Result;
use crossterm::cursor::position;
use crossterm::event::{self, Event, KeyCode, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, size, ScrollUp};
use futures_util::StreamExt;
use tokio::runtime::Handle;
use tokio::task::JoinHandle;
use tokio::sync::oneshot;

use crate::agent::AgentRunner;
use crate::config::Config;
use crate::mcp::McpStore;
use crate::tui::render;
use crate::tui::state::{AppState, ApprovalPending, TuiEvent};
use crate::tools::{Tool, ToolApprovalDecision, ToolApprovalRequest};

pub struct App {
    state: AppState,
    runner: Arc<AgentRunner>,
    handle: Handle,
    current_task: Option<JoinHandle<()>>,
}

impl App {
    pub fn new(
        runner: Arc<AgentRunner>,
        handle: Handle,
        banner: String,
        status_model: String,
        status_build: String,
        result_rx: mpsc::Receiver<anyhow::Result<TuiEvent>>,
        result_tx: mpsc::Sender<anyhow::Result<TuiEvent>>,
    ) -> Self {
        let state = AppState::new(banner, status_model, status_build, result_rx, result_tx);
        let approval_sender = state.result_tx.clone();
        runner.set_approval_handler(Arc::new(move |request: ToolApprovalRequest| {
            let (tx, rx) = oneshot::channel();
            let _ = approval_sender.send(Ok(TuiEvent::ApprovalRequest {
                request,
                respond_to: tx,
            }));
            Box::pin(async move {
                rx.await.unwrap_or(ToolApprovalDecision::DenyOnce)
            })
        }));
        Self {
            state,
            runner,
            handle,
            current_task: None,
        }
    }

    pub fn run(&mut self) -> anyhow::Result<()> {
        let mut stdout = io::stdout();
        writeln!(stdout)?;
        stdout.flush()?;
        enable_raw_mode()?;
        writeln!(stdout)?;
        stdout.flush()?;
        self.state.origin_y = position().map(|(_, y)| y).unwrap_or(0);
        let result = self.run_loop(&mut stdout);

        disable_raw_mode()?;
        execute!(stdout, crossterm::cursor::Show)?;

        result
    }

    fn run_loop(&mut self, stdout: &mut Stdout) -> Result<()> {
        while !self.state.should_quit {
            self.ensure_layout_space(stdout)?;
            render::draw(stdout, &mut self.state)?;

            self.state.tick = self.state.tick.wrapping_add(1);
            self.drain_results();
            self.maybe_start_next();
            if event::poll(Duration::from_millis(100))? {
                if let Event::Key(key) = event::read()? {
                    if key.code == KeyCode::Char('c')
                        && key.modifiers.contains(KeyModifiers::CONTROL)
                    {
                        if self.state.status_state == "running" {
                            if let Some(handle) = self.current_task.take() {
                                handle.abort();
                            }
                            self.cancel_pending_approval();
                            self.state.set_idle();
                            self.state.append_message("interrupted");
                        } else {
                            self.state.should_quit = true;
                        }
                    }
                    if self.state.approval_pending.is_some() {
                        if self.handle_approval_key(&key.code) {
                            continue;
                        }
                    }
                    match key.code {
                        KeyCode::Char(ch) => {
                            self.state.input.push(ch);
                            self.refresh_suggestions();
                        }
                        KeyCode::Up => {
                            self.history_prev();
                        }
                        KeyCode::Down => {
                            self.history_next();
                        }
                        KeyCode::Backspace => {
                            self.state.input.pop();
                            self.refresh_suggestions();
                        }
                        KeyCode::Enter => {
                            self.handle_input();
                        }
                        KeyCode::Esc => {
                            self.state.input.clear();
                            self.refresh_suggestions();
                        }
                        _ => {}
                    }
                }
            }
        }

        Ok(())
    }

    fn ensure_layout_space(&mut self, stdout: &mut Stdout) -> Result<()> {
        let (_term_width, term_height) = size()?;
        let required = self.required_height();
        let needed = self
            .state
            .origin_y
            .saturating_add(required)
            .saturating_sub(term_height);
        if needed > 0 {
            execute!(stdout, ScrollUp(needed))?;
            self.state.origin_y = self.state.origin_y.saturating_sub(needed);
        }
        Ok(())
    }

    fn required_height(&mut self) -> u16 {
        let input_height = self.state.input_row_count().saturating_add(1);
        let divider_height = 1u16;
        let spacer_height = 1u16;
        let app_status_height = 1u16;
        let help_height = if self.state.suggestions.is_empty() {
            0
        } else {
            self.state.suggestions.lines().count() as u16
        };
        let mut desired_log = (self.state.log_lines.len() as u16).max(3);
        if desired_log >= 5 {
            self.state.inline.min_log_rows = 5;
        }
        desired_log = desired_log.max(self.state.inline.min_log_rows);
        desired_log
            .saturating_add(spacer_height)
            .saturating_add(divider_height)
            .saturating_add(input_height)
            .saturating_add(help_height)
            .saturating_add(app_status_height)
    }

    fn refresh_suggestions(&mut self) {
        if self.state.input.trim().starts_with('/') {
            self.state.suggestions = build_slash_help_filtered(self.state.input.trim());
        } else {
            self.state.suggestions.clear();
        }
    }

    fn handle_input(&mut self) {
        let input = self.state.input.trim().to_string();
        self.state.input.clear();
        self.refresh_suggestions();
        if input.is_empty() {
            return;
        }
        self.push_history(&input);
        if input == "/" || input == "／" {
            self.state.append_message(&build_slash_help());
            return;
        }
        if let Some(response) = handle_slash_command(&input) {
            if input == "/exit" || input == "/quit" {
                self.state.should_quit = true;
                return;
            }
            self.state.append_message(&response);
            return;
        }

        if self.state.status_state == "running" {
            self.state.queue.push_back(crate::tui::state::PendingInput {
                text: input,
                logged: false,
            });
            return;
        }

        self.state.append_user_message(&format!("> {}", input));
        self.state.queue.push_back(crate::tui::state::PendingInput {
            text: input,
            logged: true,
        });
        self.maybe_start_next();
    }

    fn push_history(&mut self, input: &str) {
        if self
            .state
            .history
            .last()
            .map(|last| last == input)
            .unwrap_or(false)
        {
            self.state.history_index = None;
            self.state.draft_input.clear();
            return;
        }
        self.state.history.push(input.to_string());
        self.state.history_index = None;
        self.state.draft_input.clear();
    }

    fn history_prev(&mut self) {
        if self.state.history.is_empty() {
            return;
        }
        let last_index = self.state.history.len().saturating_sub(1);
        let next_index = match self.state.history_index {
            None => {
                self.state.draft_input = self.state.input.clone();
                last_index
            }
            Some(idx) => idx.saturating_sub(1),
        };
        self.state.history_index = Some(next_index);
        self.state.input = self.state.history[next_index].clone();
        self.refresh_suggestions();
    }

    fn history_next(&mut self) {
        let Some(idx) = self.state.history_index else {
            return;
        };
        let next = idx.saturating_add(1);
        if next >= self.state.history.len() {
            self.state.history_index = None;
            self.state.input = self.state.draft_input.clone();
            self.state.draft_input.clear();
        } else {
            self.state.history_index = Some(next);
            self.state.input = self.state.history[next].clone();
        }
        self.refresh_suggestions();
    }

    fn maybe_start_next(&mut self) {
        if self.state.status_state == "running" {
            return;
        }
        let Some(pending) = self.state.queue.pop_front() else {
            return;
        };
        if !pending.logged {
            self.state.append_user_message(&format!("> {}", pending.text));
        }
        self.state.append_blank_line();
        self.state.set_running("waiting LLM");
        self.state.log_lines.push_back(crate::tui::state::LogLine {
            role: crate::tui::state::LogRole::Assistant,
            text: String::new(),
        });
        self.state.start_assistant_response();
        let runner = Arc::clone(&self.runner);
        let input_clone = pending.text.clone();
        let context = self.state.build_context(10);
        self.state.push_user_conversation(&pending.text);
        let result_tx = self.state.result_tx.clone();
        let handle = self.handle.spawn(async move {
            let stream_result = runner
                .handle_prompt_stream_with_context(&input_clone, &context)
                .await;
            match stream_result {
                Ok(mut stream) => {
                    while let Some(chunk) = stream.next().await {
                        match chunk {
                            Ok(text) => {
                                if result_tx.send(Ok(TuiEvent::Chunk(text))).is_err() {
                                    return;
                                }
                            }
                            Err(err) => {
                                let _ = result_tx.send(Err(err));
                                return;
                            }
                        }
                    }
                    let _ = result_tx.send(Ok(TuiEvent::Done));
                }
                Err(_) => {
                    match runner.handle_prompt_with_context(&input_clone, &context).await {
                        Ok(output) => {
                            let _ = result_tx.send(Ok(TuiEvent::Chunk(output.response.content)));
                            let _ = result_tx.send(Ok(TuiEvent::Done));
                        }
                        Err(err) => {
                            let _ = result_tx.send(Err(err));
                        }
                    }
                }
            }
        });
        self.current_task = Some(handle);
    }

    fn drain_results(&mut self) {
        while let Ok(result) = self.state.result_rx.try_recv() {
            match result {
                Ok(event) => match event {
                    TuiEvent::Chunk(text) => {
                        self.state.append_stream_chunk(&text);
                        self.state.append_assistant_chunk(&text);
                    }
                    TuiEvent::Done => {
                        self.state.finalize_assistant_response();
                        self.state.set_idle();
                        self.current_task = None;
                    }
                    TuiEvent::ApprovalRequest { request, respond_to } => {
                        let prompt = format_approval_prompt(&request);
                        self.state.append_message(&prompt);
                        self.state.status_state = "running".to_string();
                        self.state.status_detail = "approval required".to_string();
                        self.state.approval_pending = Some(ApprovalPending { respond_to });
                    }
                },
                Err(err) => {
                    self.state.append_message(&format!("error: {}", err));
                    self.state.set_idle();
                    self.current_task = None;
                }
            }
        }
    }

    fn handle_approval_key(&mut self, key: &KeyCode) -> bool {
        let decision = match key {
            KeyCode::Char('y') => Some(ToolApprovalDecision::AllowOnce),
            KeyCode::Char('n') => Some(ToolApprovalDecision::DenyOnce),
            KeyCode::Char('a') => Some(ToolApprovalDecision::AllowAll),
            KeyCode::Char('d') => Some(ToolApprovalDecision::DenyAll),
            _ => None,
        };
        if let Some(decision) = decision {
            if let Some(pending) = self.state.approval_pending.take() {
                let _ = pending.respond_to.send(decision);
            }
            self.state.status_detail = "waiting LLM".to_string();
            return true;
        }
        true
    }

    fn cancel_pending_approval(&mut self) {
        if let Some(pending) = self.state.approval_pending.take() {
            let _ = pending.respond_to.send(ToolApprovalDecision::DenyOnce);
        }
    }
}

fn handle_slash_command(input: &str) -> Option<String> {
    let trimmed = input.trim_start();
    let normalized = if let Some(rest) = trimmed.strip_prefix('／') {
        let mut normalized = String::from("/");
        normalized.push_str(rest);
        normalized
    } else {
        trimmed.to_string()
    };
    if !normalized.starts_with('/') {
        return None;
    }
    let mut parts = normalized.split_whitespace();
    let command = parts.next().unwrap_or("");
    let _args: Vec<&str> = parts.collect();

    match command {
        "/help" => Some(build_slash_help()),
        "/mcp" => list_mcp_servers().ok(),
        "/tools" => Some(list_builtin_tools()),
        "/status" => show_status().ok(),
        "/model" => show_model().ok(),
        "/approvals" => show_approvals().ok(),
        "/new" | "/clear" | "/resume" | "/fork" | "/save" | "/load" | "/diff" | "/commit"
        | "/pr" | "/editor" => Some(format!("{} is not implemented in TUI yet.", command)),
        _ => {
            let filtered = build_slash_help_filtered(command);
            if filtered.is_empty() {
                Some(format!("unknown command: {}", command))
            } else {
                Some(filtered)
            }
        }
    }
}

fn format_approval_prompt(request: &ToolApprovalRequest) -> String {
    let tool_name = match request.tool {
        Tool::Read => "Read",
        Tool::Write => "Write",
        Tool::Shell => "Shell",
        Tool::Grep => "Grep",
        Tool::Glob => "Glob",
    };
    let target = if request.paths.is_empty() {
        "target".to_string()
    } else if request.paths.len() == 1 {
        request.paths[0].display().to_string()
    } else {
        format!("{} (+{} more)", request.paths[0].display(), request.paths.len() - 1)
    };
    format!(
        "Allow {} to {}?\n[y] Yes  [n] No  [a] Always allow  [d] Don't ask again",
        tool_name, target
    )
}

#[derive(Clone, Copy)]
struct SlashCommandHelp {
    cmd: &'static str,
    desc_en: &'static str,
}

fn slash_help_items() -> Vec<SlashCommandHelp> {
    vec![
        SlashCommandHelp {
            cmd: "/new",
            desc_en: "Add a new working session",
        },
        SlashCommandHelp {
            cmd: "/clear",
            desc_en: "Clear conversation history",
        },
        SlashCommandHelp {
            cmd: "/resume",
            desc_en: "List or resume sessions",
        },
        SlashCommandHelp {
            cmd: "/resume --last",
            desc_en: "Resume latest session",
        },
        SlashCommandHelp {
            cmd: "/fork",
            desc_en: "Fork latest session",
        },
        SlashCommandHelp {
            cmd: "/save <path>",
            desc_en: "Save latest session",
        },
        SlashCommandHelp {
            cmd: "/load <path>",
            desc_en: "Load session from path",
        },
        SlashCommandHelp {
            cmd: "/model",
            desc_en: "Show model name",
        },
        SlashCommandHelp {
            cmd: "/approvals",
            desc_en: "Show approval policy",
        },
        SlashCommandHelp {
            cmd: "/status",
            desc_en: "Show current status",
        },
        SlashCommandHelp {
            cmd: "/tools",
            desc_en: "List built-in tools",
        },
        SlashCommandHelp {
            cmd: "/mcp",
            desc_en: "List MCP servers",
        },
        SlashCommandHelp {
            cmd: "/diff",
            desc_en: "Show git diff",
        },
        SlashCommandHelp {
            cmd: "/commit <msg>",
            desc_en: "Create git commit",
        },
        SlashCommandHelp {
            cmd: "/pr [args]",
            desc_en: "gh pr create (pass-through args)",
        },
        SlashCommandHelp {
            cmd: "/editor [path]",
            desc_en: "Open external editor",
        },
        SlashCommandHelp {
            cmd: "/help",
            desc_en: "Show help",
        },
        SlashCommandHelp {
            cmd: "/exit, /quit",
            desc_en: "Show exit hint",
        },
    ]
}

fn build_slash_help() -> String {
    build_slash_help_from_items(&slash_help_items())
}

pub fn build_slash_help_filtered(prefix: &str) -> String {
    let items: Vec<SlashCommandHelp> = slash_help_items()
        .into_iter()
        .filter(|item| item.cmd.starts_with(prefix))
        .collect();
    if items.is_empty() {
        String::new()
    } else {
        build_slash_help_from_items(&items)
    }
}

fn build_slash_help_from_items(items: &[SlashCommandHelp]) -> String {
    let mut lines = Vec::new();
    for item in items {
        lines.push(format!("{:<14} {}", item.cmd, item.desc_en));
    }
    lines.join("\n")
}

fn list_mcp_servers() -> anyhow::Result<String> {
    let path = McpStore::default_path();
    let config = McpStore::load(&path)?;
    if config.mcp_servers.is_empty() {
        return Ok("no mcp servers".to_string());
    }
    let mut lines = Vec::new();
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
        lines.push(format!("{} {}", name, summary.trim()));
    }
    Ok(lines.join("\n"))
}

fn list_builtin_tools() -> String {
    [
        "Read",
        "Write",
        "Shell",
        "Grep",
        "Glob",
        "MCP(@server/tool)",
    ]
    .join("\n")
}

fn show_status() -> anyhow::Result<String> {
    let config = load_config().unwrap_or_default();
    let model = config.model.name.unwrap_or_else(|| "unknown".to_string());
    let provider = config.model.provider;
    let approvals = config
        .permissions
        .as_ref()
        .and_then(|p| p.approval_policy.clone())
        .unwrap_or_else(|| "unset".to_string());
    let sandbox = config
        .sandbox
        .as_ref()
        .and_then(|s| s.mode.clone())
        .unwrap_or_else(|| "none".to_string());
    Ok(format!(
        "model: {}\nprovider: {}\napprovals: {}\nsandbox: {}",
        model, provider, approvals, sandbox
    ))
}

fn show_model() -> anyhow::Result<String> {
    let config = load_config().unwrap_or_default();
    let model = config.model.name.unwrap_or_else(|| "unknown".to_string());
    Ok(format!("model: {}", model))
}

fn show_approvals() -> anyhow::Result<String> {
    let config = load_config().unwrap_or_default();
    let approvals = config
        .permissions
        .as_ref()
        .and_then(|p| p.approval_policy.clone())
        .unwrap_or_else(|| "unset".to_string());
    Ok(format!("approvals: {}", approvals))
}

fn load_config() -> Option<Config> {
    let mut candidates = Vec::new();
    if let Some(home) = std::env::var_os("HOME") {
        candidates.push(
            std::path::PathBuf::from(home)
                .join(".tengu")
                .join("config.toml"),
        );
    }
    candidates.push(
        std::path::PathBuf::from(".")
            .join(".tengu")
            .join("config.toml"),
    );

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
