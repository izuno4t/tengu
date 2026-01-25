use std::io::{self, Stdout, Write};
use std::sync::{mpsc, Arc};
use std::time::Duration;

use anyhow::Result;
use crossterm::cursor::position;
use crossterm::event::{self, Event, KeyCode, KeyModifiers};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, size, ScrollUp};
use crossterm::execute;
use ratatui::prelude::*;
use tokio::runtime::Handle;
use tokio::task::JoinHandle;

use crate::agent::{AgentOutput, AgentRunner};
use crate::config::Config;
use crate::mcp::McpStore;
use crate::tui::render;
use crate::tui::state::AppState;

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
        result_rx: mpsc::Receiver<anyhow::Result<AgentOutput>>,
        result_tx: mpsc::Sender<anyhow::Result<AgentOutput>>,
    ) -> Self {
        let state = AppState::new(banner, status_model, status_build, result_rx, result_tx);
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
        self.state.origin_y = position().map(|(_, y)| y).unwrap_or(0);
        let mut terminal = Terminal::new(CrosstermBackend::new(stdout))?;

        let result = self.run_loop(&mut terminal);

        disable_raw_mode()?;
        terminal.show_cursor()?;

        result
    }

    fn run_loop(&mut self, terminal: &mut Terminal<CrosstermBackend<Stdout>>) -> Result<()> {
        while !self.state.should_quit {
            self.ensure_layout_space(terminal.backend_mut())?;
            terminal.draw(|frame| {
                render::draw(frame, &mut self.state);
            })?;

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
                            self.state.set_idle();
                            self.state.append_message("interrupted");
                        } else {
                            self.state.should_quit = true;
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

    fn ensure_layout_space(&mut self, backend: &mut CrosstermBackend<Stdout>) -> Result<()> {
        let (_term_width, term_height) = size()?;
        let required = self.required_height();
        let needed = self
            .state
            .origin_y
            .saturating_add(required)
            .saturating_sub(term_height);
        if needed > 0 {
            execute!(backend, ScrollUp(needed))?;
            self.state.origin_y = self.state.origin_y.saturating_sub(needed);
        }
        Ok(())
    }

    fn required_height(&self) -> u16 {
        let input_height = 2u16;
        let divider_height = 1u16;
        let spacer_height = 1u16;
        let status_height = 1u16;
        let app_status_height = 1u16;
        let help_height = if self.state.suggestions.is_empty() {
            0
        } else {
            self.state.suggestions.lines().count() as u16
        };
        let desired_log = (self.state.log_lines.len() as u16).max(3);
        desired_log
            .saturating_add(spacer_height)
            .saturating_add(status_height)
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

        self.state.append_message(&format!("> {}", input));
        self.state.queue.push_back(input);
        self.maybe_start_next();
    }

    fn push_history(&mut self, input: &str) {
        if self.state.history.last().map(|last| last == input).unwrap_or(false) {
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
        let Some(input) = self.state.queue.pop_front() else {
            return;
        };
        self.state.set_running("waiting LLM");
        let runner = Arc::clone(&self.runner);
        let input_clone = input.clone();
        let result_tx = self.state.result_tx.clone();
        let handle = self.handle.spawn(async move {
            let result = runner.handle_prompt(&input_clone).await;
            let _ = result_tx.send(result);
        });
        self.current_task = Some(handle);
    }

    fn drain_results(&mut self) {
        while let Ok(result) = self.state.result_rx.try_recv() {
            match result {
                Ok(output) => {
                    self.state
                        .append_message(&collapse_result(&output.response.content));
                }
                Err(err) => {
                    self.state.append_message(&format!("error: {}", err));
                }
            }
            self.state.set_idle();
            self.current_task = None;
        }
    }
}

fn collapse_result(content: &str) -> String {
    let marker = "\n\n";
    if let Some((plan, _rest)) = content.split_once(marker) {
        format!("{plan}\n\n結果:（折りたたみ）")
    } else {
        format!("{content}\n\n結果:（折りたたみ）")
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
        "/new"
        | "/clear"
        | "/resume"
        | "/fork"
        | "/save"
        | "/load"
        | "/diff"
        | "/commit"
        | "/pr"
        | "/editor" => Some(format!("{} is not implemented in TUI yet.", command)),
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
        candidates.push(std::path::PathBuf::from(home).join(".tengu").join("config.toml"));
    }
    candidates.push(std::path::PathBuf::from(".").join(".tengu").join("config.toml"));

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
