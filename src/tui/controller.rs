use std::fs;
use std::io::{self, Stdout, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{mpsc, Arc};
use std::time::Duration;

use anyhow::{anyhow, Result};
use base64::Engine;
use crossterm::cursor::position;
use crossterm::event::{self, Event, KeyCode, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, size, ScrollUp};
use futures_util::StreamExt;
use tokio::runtime::Handle;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;

use crate::agent::{AgentRunner, AgentStore};
use crate::config::Config;
use crate::llm::{LlmImage, LlmRequest, LlmStreamEvent};
use crate::mcp::McpStore;
use crate::review::{build_review_prompt, parse_review_args};
use crate::session::SessionPendingApproval;
use crate::session::{Session, SessionStore};
use crate::tools::{Tool, ToolApprovalDecision, ToolApprovalRequest};
use crate::tui::render;
use crate::tui::state::{AppState, ApprovalPending, PendingMode, TuiEvent};

pub struct App {
    state: AppState,
    runner: Arc<AgentRunner>,
    handle: Handle,
    current_task: Option<JoinHandle<()>>,
    session_store: Option<SessionStore>,
    current_session: Option<Session>,
    pending_local_action: Option<PendingLocalAction>,
    pending_tool_approval: Option<ToolApprovalRequest>,
    restored_tool_approval: Option<RestoredToolApproval>,
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
        let session_store = SessionStore::default_root().ok().map(SessionStore::new);
        let current_session = create_persisted_session(session_store.as_ref());
        let approval_sender = state.result_tx.clone();
        runner.set_approval_handler(Arc::new(move |request: ToolApprovalRequest| {
            let (tx, rx) = oneshot::channel();
            let _ = approval_sender.send(Ok(TuiEvent::ApprovalRequest {
                request,
                respond_to: tx,
            }));
            Box::pin(async move { rx.await.unwrap_or(ToolApprovalDecision::DenyOnce) })
        }));
        Self {
            state,
            runner,
            handle,
            current_task: None,
            session_store,
            current_session,
            pending_local_action: None,
            pending_tool_approval: None,
            restored_tool_approval: None,
        }
    }

    pub fn run(&mut self) -> anyhow::Result<()> {
        let mut stdout = io::stdout();
        writeln!(stdout)?;
        stdout.flush()?;
        enable_raw_mode()?;
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
                    if (self.state.approval_pending.is_some()
                        || self.pending_local_action.is_some())
                        && self.handle_approval_key(&key.code)
                    {
                        continue;
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
        let status_height = 1u16;
        let status_gap = 1u16;
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
            .saturating_add(status_height)
            .saturating_add(status_gap)
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
        if input == "/" || input == "／" || input == "/help" {
            self.state.suggestions = build_slash_help();
            return;
        }
        self.push_history(&input);
        if let Some(outcome) = handle_slash_command(&input) {
            match outcome {
                SlashCommandOutcome::Display(response) => {
                    self.state.append_message(&response);
                    return;
                }
                SlashCommandOutcome::AttachImages(paths) => {
                    let response = self.attach_images(paths);
                    self.state.append_message(&response);
                    self.state.append_blank_line();
                    return;
                }
                SlashCommandOutcome::TogglePlanMode(mode) => {
                    let response = self.set_plan_mode(mode);
                    self.state.append_message(&response);
                    self.state.append_blank_line();
                    return;
                }
                SlashCommandOutcome::QueuePlan(request) => {
                    let response = self.queue_plan_request(request);
                    self.state.append_message(&response);
                    self.state.append_blank_line();
                    return;
                }
                SlashCommandOutcome::TaskWriter(title) => {
                    let response = self.write_tasks_from_plan(title.as_deref());
                    self.state.append_message(&response);
                    self.state.append_blank_line();
                    return;
                }
                SlashCommandOutcome::ApplyPlan => {
                    let response = self.apply_last_plan();
                    self.state.append_message(&response);
                    self.state.append_blank_line();
                    return;
                }
                SlashCommandOutcome::Compact(note) => {
                    let response = self.compact_session(note.as_deref());
                    self.state.append_message(&response);
                    self.state.append_blank_line();
                    return;
                }
                SlashCommandOutcome::OpenMemory => {
                    let response = self.open_editor(Some(project_memory_path()));
                    self.state.append_message(&response);
                    self.state.append_blank_line();
                    return;
                }
                SlashCommandOutcome::InitMemory => {
                    let response = self.init_memory_file();
                    self.state.append_message(&response);
                    self.state.append_blank_line();
                    return;
                }
                SlashCommandOutcome::ConfigCommand(args) => {
                    let response = self.handle_config_command(&args);
                    self.state.append_message(&response);
                    self.state.append_blank_line();
                    return;
                }
                SlashCommandOutcome::AddDir(paths) => {
                    let response = self.add_directories(paths);
                    self.state.append_message(&response);
                    self.state.append_blank_line();
                    return;
                }
                SlashCommandOutcome::ListAgents => {
                    let response = self.list_agents();
                    self.state.append_message(&response);
                    self.state.append_blank_line();
                    return;
                }
                SlashCommandOutcome::Login => {
                    let response = self.login_auth();
                    self.state.append_message(&response);
                    self.state.append_blank_line();
                    return;
                }
                SlashCommandOutcome::Logout => {
                    let response = self.logout_auth();
                    self.state.append_message(&response);
                    self.state.append_blank_line();
                    return;
                }
                SlashCommandOutcome::BugReport(note) => {
                    let response = self.write_bug_report(note.as_deref());
                    self.state.append_message(&response);
                    self.state.append_blank_line();
                    return;
                }
                SlashCommandOutcome::ShowPrComments => {
                    let response = self.show_pr_comments();
                    self.state.append_message(&response);
                    self.state.append_blank_line();
                    return;
                }
                SlashCommandOutcome::TerminalSetup => {
                    let response = self.show_terminal_setup();
                    self.state.append_message(&response);
                    self.state.append_blank_line();
                    return;
                }
                SlashCommandOutcome::ShowBackground => {
                    let response = self.show_background_tasks();
                    self.state.append_message(&response);
                    self.state.append_blank_line();
                    return;
                }
                SlashCommandOutcome::ShowCost => {
                    let response = self.show_cost_status();
                    self.state.append_message(&response);
                    self.state.append_blank_line();
                    return;
                }
                SlashCommandOutcome::SetModel(model) => {
                    let response = self.set_model(model.as_deref());
                    self.state.append_message(&response);
                    self.state.append_blank_line();
                    return;
                }
                SlashCommandOutcome::SetStrategy(strategy) => {
                    let response = self.set_strategy(strategy.as_deref());
                    self.state.append_message(&response);
                    self.state.append_blank_line();
                    return;
                }
                SlashCommandOutcome::ToggleVim => {
                    let response = self.toggle_vim_mode();
                    self.state.append_message(&response);
                    self.state.append_blank_line();
                    return;
                }
                SlashCommandOutcome::Doctor => {
                    let response = self.run_doctor();
                    self.state.append_message(&response);
                    self.state.append_blank_line();
                    return;
                }
                SlashCommandOutcome::OpenEditor(path) => {
                    let response = self.open_editor(path);
                    self.state.append_message(&response);
                    self.state.append_blank_line();
                    return;
                }
                SlashCommandOutcome::Reset {
                    response,
                    new_session,
                } => {
                    if let Some(handle) = self.current_task.take() {
                        handle.abort();
                    }
                    self.cancel_pending_approval();
                    self.state.reset_session_view();
                    if new_session {
                        self.current_session =
                            create_persisted_session(self.session_store.as_ref());
                    }
                    self.state.append_message(&response);
                    self.state.append_blank_line();
                    return;
                }
                SlashCommandOutcome::ForkSession => {
                    let response = self.fork_current_session();
                    self.state.append_message(&response);
                    self.state.append_blank_line();
                    return;
                }
                SlashCommandOutcome::SaveSession => {
                    let response = self.save_current_session();
                    self.state.append_message(&response);
                    self.state.append_blank_line();
                    return;
                }
                SlashCommandOutcome::SaveSessionAs(path) => {
                    let response = self.save_session_to_path(&path);
                    self.state.append_message(&response);
                    self.state.append_blank_line();
                    return;
                }
                SlashCommandOutcome::Resume(target) => {
                    let response = self.handle_resume_command(target);
                    self.state.append_message(&response);
                    self.state.append_blank_line();
                    return;
                }
                SlashCommandOutcome::LoadPath(path) => {
                    let response = self.load_session_from_path(&path);
                    self.state.append_message(&response);
                    self.state.append_blank_line();
                    return;
                }
                SlashCommandOutcome::ConfirmLocal { prompt, action } => {
                    self.pending_local_action = Some(action);
                    self.state.append_message(&prompt);
                    self.state.status_state = "running".to_string();
                    self.state.status_detail = "approval required".to_string();
                    return;
                }
                SlashCommandOutcome::Exit(response) => {
                    self.state.append_message(&response);
                    self.state.should_quit = true;
                    return;
                }
                SlashCommandOutcome::Submit(prompt) => {
                    let images = self.state.take_pending_images();
                    self.state
                        .append_message(&format!("custom command expanded:\n{}", prompt));
                    self.state.append_user_message(&format!("> {}", prompt));
                    self.state.queue.push_back(crate::tui::state::PendingInput {
                        text: prompt,
                        logged: true,
                        images,
                        mode: PendingMode::Execute,
                    });
                    self.touch_current_session();
                    self.maybe_start_next();
                    return;
                }
            }
        }

        if let Some(paths) = parse_dropped_image_paths(&input) {
            let response = self.attach_images(paths);
            self.state.append_message(&response);
            self.state.append_blank_line();
            return;
        }

        if self.state.status_state == "running" {
            let images = self.state.take_pending_images();
            self.state.queue.push_back(crate::tui::state::PendingInput {
                text: input,
                logged: false,
                images,
                mode: if self.state.plan_mode {
                    PendingMode::Plan
                } else {
                    PendingMode::Execute
                },
            });
            return;
        }

        let images = self.state.take_pending_images();
        self.state.append_user_message(&format!("> {}", input));
        self.state.queue.push_back(crate::tui::state::PendingInput {
            text: input,
            logged: true,
            images,
            mode: if self.state.plan_mode {
                PendingMode::Plan
            } else {
                PendingMode::Execute
            },
        });
        self.touch_current_session();
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
        self.touch_current_session();
        if !pending.logged {
            self.state
                .append_user_message(&format!("> {}", pending.text));
        }
        self.state.record_pending_usage(&pending);
        self.state.append_blank_line();
        self.state
            .set_running(if pending.mode == PendingMode::Plan {
                "planning"
            } else {
                "waiting LLM"
            });
        if pending.mode == PendingMode::Execute {
            self.state.log_lines.push_back(crate::tui::state::LogLine {
                role: crate::tui::state::LogRole::Assistant,
                text: String::new(),
            });
            self.state.start_assistant_response();
        }
        let runner = Arc::clone(&self.runner);
        let request = LlmRequest {
            prompt: pending.text.clone(),
            images: pending.images.clone(),
        };
        let context = self.state.build_context(10);
        let pending_text = pending.text.clone();
        let pending_mode = pending.mode;
        self.state.push_user_conversation(&pending_text);
        let result_tx = self.state.result_tx.clone();
        let handle = self.handle.spawn(async move {
            if pending_mode == PendingMode::Plan {
                match runner
                    .generate_plan_text_with_context(&pending_text, &context)
                    .await
                {
                    Ok(plan) => {
                        let _ = result_tx.send(Ok(TuiEvent::PlanResult {
                            request: pending_text,
                            plan,
                        }));
                    }
                    Err(err) => {
                        let _ = result_tx.send(Err(err));
                    }
                }
                return;
            }
            let stream_result = runner
                .handle_request_stream_with_context(request.clone(), &context)
                .await;
            match stream_result {
                Ok((mut stream, _tool_result)) => {
                    while let Some(chunk) = stream.next().await {
                        match chunk {
                            Ok(LlmStreamEvent::Text(text)) => {
                                if result_tx.send(Ok(TuiEvent::Chunk(text))).is_err() {
                                    return;
                                }
                            }
                            Ok(LlmStreamEvent::Usage(usage)) => {
                                if result_tx.send(Ok(TuiEvent::Usage(usage))).is_err() {
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
                Err(_) => match runner.handle_request_with_context(request, &context).await {
                    Ok(output) => {
                        if let Some(usage) = output.response.usage {
                            let _ = result_tx.send(Ok(TuiEvent::Usage(usage)));
                        }
                        let _ = result_tx.send(Ok(TuiEvent::Chunk(output.response.content)));
                        let _ = result_tx.send(Ok(TuiEvent::Done));
                    }
                    Err(err) => {
                        let _ = result_tx.send(Err(err));
                    }
                },
            }
        });
        self.current_task = Some(handle);
    }

    fn drain_results(&mut self) {
        while let Ok(result) = self.state.result_rx.try_recv() {
            match result {
                Ok(event) => match event {
                    TuiEvent::Chunk(text) => {
                        self.state.record_output_chunk(&text);
                        self.state.append_stream_chunk(&text);
                        self.state.append_assistant_chunk(&text);
                    }
                    TuiEvent::Usage(usage) => {
                        self.state.record_provider_usage(&usage);
                    }
                    TuiEvent::PlanResult { request, plan } => {
                        self.state.record_output_chunk(&plan);
                        self.state.store_plan(request, plan.clone());
                        self.state.append_message("plan:");
                        self.state.append_message(&plan);
                        self.state.append_blank_line();
                        self.state.set_idle();
                        self.current_task = None;
                    }
                    TuiEvent::Done => {
                        self.state.finalize_assistant_response();
                        self.touch_current_session();
                        self.state.set_idle();
                        self.current_task = None;
                    }
                    TuiEvent::ApprovalRequest {
                        request,
                        respond_to,
                    } => {
                        self.state.record_approval_request();
                        let prompt = format_approval_prompt(&request);
                        self.state.append_message(&prompt);
                        self.state.status_state = "running".to_string();
                        self.state.status_detail = "approval required".to_string();
                        self.pending_tool_approval = Some(request.clone());
                        self.restored_tool_approval = None;
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
                self.pending_tool_approval = None;
                self.state.status_detail = "waiting LLM".to_string();
                return true;
            }
            if let Some(action) = self.pending_local_action.take() {
                let approved = matches!(
                    decision,
                    ToolApprovalDecision::AllowOnce | ToolApprovalDecision::AllowAll
                );
                let message = if approved {
                    execute_pending_local_action(action)
                } else {
                    "local action cancelled".to_string()
                };
                self.state.append_message(&message);
                self.state.append_blank_line();
                self.state.set_idle();
                return true;
            }
            if let Some(restored) = self.restored_tool_approval.take() {
                let approved = matches!(
                    decision,
                    ToolApprovalDecision::AllowOnce | ToolApprovalDecision::AllowAll
                );
                let message = if approved {
                    format!(
                        "restored approval acknowledged for {}. rerun the previous prompt to continue.",
                        restored.summary
                    )
                } else {
                    "restored approval dismissed".to_string()
                };
                self.state.append_message(&message);
                self.state.append_blank_line();
                self.state.set_idle();
                return true;
            }
            return true;
        }
        true
    }

    fn cancel_pending_approval(&mut self) {
        if let Some(pending) = self.state.approval_pending.take() {
            let _ = pending.respond_to.send(ToolApprovalDecision::DenyOnce);
        }
        self.pending_local_action = None;
        self.pending_tool_approval = None;
        self.restored_tool_approval = None;
    }

    fn touch_current_session(&mut self) {
        let pending_approval = self.export_pending_approval();
        let Some(session) = &mut self.current_session else {
            self.current_session = create_persisted_session(self.session_store.as_ref());
            return;
        };
        session.conversation = self.state.export_conversation();
        session.log_lines = self.state.export_log_lines();
        session.queue = self.state.export_queue();
        session.pending_images = self.state.export_pending_images();
        session.usage_records = self.state.export_usage_records();
        session.pending_approval = pending_approval;
        session.updated_at = chrono::Utc::now().to_rfc3339();
        if let Some(store) = &self.session_store {
            let _ = store.save(session);
        }
    }

    fn save_current_session(&mut self) -> String {
        if self.current_session.is_none() {
            self.current_session = create_persisted_session(self.session_store.as_ref());
        }
        self.touch_current_session();
        match &self.current_session {
            Some(session) => format!("session saved: {}", session.id),
            None => "session store unavailable".to_string(),
        }
    }

    fn save_session_to_path(&mut self, path: &Path) -> String {
        if self.current_session.is_none() {
            self.current_session = create_persisted_session(self.session_store.as_ref());
        }
        self.touch_current_session();
        match &self.current_session {
            Some(session) => match SessionStore::save_to_path(path, session) {
                Ok(()) => format!("session exported: {}", path.display()),
                Err(err) => format!("save failed: {}", err),
            },
            None => "session store unavailable".to_string(),
        }
    }

    fn fork_current_session(&mut self) -> String {
        if self.session_store.is_none() {
            return "session store unavailable".to_string();
        }
        if self.current_session.is_none() {
            self.current_session = create_persisted_session(self.session_store.as_ref());
        }
        self.touch_current_session();
        let Some(current) = &self.current_session else {
            return "session store unavailable".to_string();
        };
        let forked = current.fork();
        let fork_id = forked.id.clone();
        if let Some(store) = &self.session_store {
            if let Err(err) = store.save(&forked) {
                return format!("fork failed: {}", err);
            }
        }
        self.current_session = Some(forked);
        format!("forked session: {}", fork_id)
    }

    fn handle_resume_command(&mut self, target: ResumeTarget) -> String {
        let Some(store) = &self.session_store else {
            return "session store unavailable".to_string();
        };
        match target {
            ResumeTarget::List => match store.list() {
                Ok(entries) if entries.is_empty() => "no sessions".to_string(),
                Ok(entries) => entries
                    .iter()
                    .map(|session| {
                        format!(
                            "{} {} {}",
                            session.id, session.created_at, session.updated_at
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n"),
                Err(err) => format!("resume failed: {}", err),
            },
            ResumeTarget::Last => match store.latest() {
                Ok(Some(session)) => {
                    self.current_session = Some(session.clone());
                    self.state.restore_from_session(
                        &session.conversation,
                        &session.log_lines,
                        &session.queue,
                        &session.pending_images,
                        &session.usage_records,
                    );
                    self.restore_pending_approval(session.pending_approval.clone());
                    format!("resumed session: {}", session.id)
                }
                Ok(None) => "no sessions".to_string(),
                Err(err) => format!("resume failed: {}", err),
            },
            ResumeTarget::ById(id) => match store.load(&id) {
                Ok(session) => {
                    self.current_session = Some(session.clone());
                    self.state.restore_from_session(
                        &session.conversation,
                        &session.log_lines,
                        &session.queue,
                        &session.pending_images,
                        &session.usage_records,
                    );
                    self.restore_pending_approval(session.pending_approval.clone());
                    format!("resumed session: {}", session.id)
                }
                Err(err) => format!("resume failed: {}", err),
            },
        }
    }

    fn load_session_from_path(&mut self, path: &Path) -> String {
        match SessionStore::load_from_path(path) {
            Ok(session) => {
                self.current_session = Some(session.clone());
                self.state.restore_from_session(
                    &session.conversation,
                    &session.log_lines,
                    &session.queue,
                    &session.pending_images,
                    &session.usage_records,
                );
                self.restore_pending_approval(session.pending_approval.clone());
                format!("loaded session: {}", path.display())
            }
            Err(err) => format!("load failed: {}", err),
        }
    }

    fn attach_images(&mut self, paths: Vec<PathBuf>) -> String {
        let mut images = Vec::new();
        for path in &paths {
            match load_tui_image(path) {
                Ok(image) => images.push(image),
                Err(err) => return format!("image load failed: {}", err),
            }
        }
        self.state.set_pending_images(images);
        format!("attached {} image(s) for the next prompt", paths.len())
    }

    fn set_plan_mode(&mut self, mode: Option<bool>) -> String {
        let next = mode.unwrap_or(!self.state.plan_mode);
        self.state.plan_mode = next;
        format!(
            "plan mode: {}",
            if self.state.plan_mode { "on" } else { "off" }
        )
    }

    fn queue_plan_request(&mut self, request: String) -> String {
        self.state
            .append_user_message(&format!("> /plan {}", request));
        self.state.queue.push_back(crate::tui::state::PendingInput {
            text: request,
            logged: true,
            images: Vec::new(),
            mode: PendingMode::Plan,
        });
        self.touch_current_session();
        self.maybe_start_next();
        "planning request queued".to_string()
    }

    fn write_tasks_from_plan(&mut self, title: Option<&str>) -> String {
        if self.state.last_plan_items.is_empty() {
            return "no saved plan to write".to_string();
        }

        let heading = title
            .filter(|value| !value.trim().is_empty())
            .map(|value| value.trim().to_string())
            .or_else(|| self.state.last_plan_request.clone())
            .unwrap_or_else(|| "Generated Tasks".to_string());

        let mut section = String::new();
        section.push_str("\n\n## ");
        section.push_str(&heading);
        section.push_str("\n\n");
        for item in &self.state.last_plan_items {
            section.push_str("- [ ] ");
            section.push_str(item);
            section.push('\n');
        }

        match fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open("TASK.md")
        {
            Ok(mut file) => match file.write_all(section.as_bytes()) {
                Ok(()) => "wrote plan items to TASK.md".to_string(),
                Err(err) => format!("taskwriter failed: {}", err),
            },
            Err(err) => format!("taskwriter failed: {}", err),
        }
    }

    fn apply_last_plan(&mut self) -> String {
        let Some(request) = self.state.last_plan_request.clone() else {
            return "no saved plan to apply".to_string();
        };
        let images = self.state.take_pending_images();
        self.state.append_user_message(&format!("> {}", request));
        self.state.queue.push_back(crate::tui::state::PendingInput {
            text: request,
            logged: true,
            images,
            mode: PendingMode::Execute,
        });
        self.touch_current_session();
        self.maybe_start_next();
        "queued last planned request for execution".to_string()
    }

    fn compact_session(&mut self, note: Option<&str>) -> String {
        let summary = self
            .state
            .conversation
            .iter()
            .rev()
            .take(6)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .map(|turn| {
                let role = match turn.role {
                    crate::tui::state::ConversationRole::User => "user",
                    crate::tui::state::ConversationRole::Assistant => "assistant",
                };
                format!("{role}: {}", turn.content)
            })
            .collect::<Vec<_>>()
            .join("\n");

        self.state.reset_session_view();
        let compacted = if let Some(note) = note.filter(|value| !value.trim().is_empty()) {
            format!("conversation compacted ({})", note.trim())
        } else {
            "conversation compacted".to_string()
        };
        self.state.append_message(&compacted);
        if !summary.is_empty() {
            self.state.append_message("summary:");
            self.state.append_message(&summary);
            self.state
                .conversation
                .push(crate::tui::state::ConversationTurn {
                    role: crate::tui::state::ConversationRole::Assistant,
                    content: format!("Compacted summary:\n{}", summary),
                });
        }
        self.touch_current_session();
        "session compacted".to_string()
    }

    fn init_memory_file(&mut self) -> String {
        let path = project_memory_path();
        if path.exists() {
            return format!("memory file already exists: {}", path.display());
        }
        if let Some(parent) = path.parent() {
            if let Err(err) = fs::create_dir_all(parent) {
                return format!("init failed: {}", err);
            }
        }
        let scaffold = "# Tengu Project Memory\n\n## Build\n- cargo build\n\n## Test\n- cargo test\n\n## Notes\n- Add project-specific guidance here.\n";
        match fs::write(&path, scaffold) {
            Ok(()) => format!("initialized memory file: {}", path.display()),
            Err(err) => format!("init failed: {}", err),
        }
    }

    fn show_config_paths(&self) -> String {
        let mut lines = vec![
            format!("config: {}", global_config_path().display()),
            format!("config: {}", local_config_path().display()),
            format!("memory: {}", project_memory_path().display()),
            format!("provider: {}", self.current_provider_label()),
            format!("model.default: {}", self.current_model_label()),
            format!(
                "plan mode: {}",
                if self.state.plan_mode { "on" } else { "off" }
            ),
        ];
        if let Some(request) = &self.state.last_plan_request {
            lines.push(format!("last plan request: {}", request));
        }
        if !self.state.added_dirs.is_empty() {
            lines.push(format!(
                "added dirs: {}",
                self.state
                    .added_dirs
                    .iter()
                    .map(|path| path.display().to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
        lines.join("\n")
    }

    fn handle_config_command(&mut self, args: &[String]) -> String {
        if args.is_empty() {
            return self.show_config_paths();
        }

        match args[0].as_str() {
            "list" => self.show_config_paths(),
            "get" => {
                let Some(key) = args.get(1) else {
                    return "usage: /config get <key>".to_string();
                };
                self.get_config_value(key)
            }
            "set" => {
                let Some(key) = args.get(1) else {
                    return "usage: /config set <key> <value>".to_string();
                };
                if args.len() < 3 {
                    return "usage: /config set <key> <value>".to_string();
                }
                self.set_config_value(key, &args[2..].join(" "))
            }
            _ => "usage: /config [list|get <key>|set <key> <value>]".to_string(),
        }
    }

    fn get_config_value(&self, key: &str) -> String {
        match key {
            "model.default" => format!("model.default: {}", self.current_model_label()),
            "model.provider" | "provider" => {
                format!("model.provider: {}", self.current_provider_label())
            }
            "plan_mode" | "plan-mode" => format!(
                "plan_mode: {}",
                if self.state.plan_mode { "on" } else { "off" }
            ),
            "permissions.approval_policy" => {
                let config = load_config().unwrap_or_default();
                let value = config
                    .permissions
                    .as_ref()
                    .and_then(|p| p.approval_policy.as_deref())
                    .unwrap_or("unset");
                format!("permissions.approval_policy: {}", value)
            }
            "sandbox.mode" => {
                let config = load_config().unwrap_or_default();
                let value = config
                    .sandbox
                    .as_ref()
                    .and_then(|s| s.mode.as_deref())
                    .unwrap_or("unset");
                format!("sandbox.mode: {}", value)
            }
            _ => format!("unsupported config key: {}", key),
        }
    }

    fn set_config_value(&mut self, key: &str, value: &str) -> String {
        let value = value.trim();
        if value.is_empty() {
            return "usage: /config set <key> <value>".to_string();
        }

        match key {
            "model.default" => {
                let mut config = load_config().unwrap_or_default();
                config.model.default = value.to_string();
                match write_local_config(&config) {
                    Ok(()) => {
                        self.state.status_model = value.to_string();
                        format!("model.default set: {}", value)
                    }
                    Err(err) => format!("config update failed: {}", err),
                }
            }
            "model.provider" | "provider" => {
                let normalized = value.to_ascii_lowercase();
                if !matches!(
                    normalized.as_str(),
                    "anthropic" | "openai" | "google" | "gemini"
                ) {
                    return format!("unsupported provider: {}", value);
                }
                let mut config = load_config().unwrap_or_default();
                config.model.provider = normalized.clone();
                match write_local_config(&config) {
                    Ok(()) => format!("model.provider set: {}", normalized),
                    Err(err) => format!("config update failed: {}", err),
                }
            }
            "plan_mode" | "plan-mode" => match value.to_ascii_lowercase().as_str() {
                "on" | "true" | "1" => {
                    self.state.plan_mode = true;
                    "plan_mode set: on".to_string()
                }
                "off" | "false" | "0" => {
                    self.state.plan_mode = false;
                    "plan_mode set: off".to_string()
                }
                _ => "plan_mode must be one of: on, off".to_string(),
            },
            _ => format!("unsupported config key: {}", key),
        }
    }

    fn current_model_label(&self) -> String {
        let config = load_config().unwrap_or_default();
        if !config.model.default.trim().is_empty() {
            config.model.default
        } else if !self.state.status_model.trim().is_empty() {
            self.state.status_model.clone()
        } else {
            "unset".to_string()
        }
    }

    fn current_provider_label(&self) -> String {
        let config = load_config().unwrap_or_default();
        if !config.model.provider.trim().is_empty() {
            config.model.provider
        } else {
            "unset".to_string()
        }
    }

    fn add_directories(&mut self, paths: Vec<PathBuf>) -> String {
        if paths.is_empty() {
            return "usage: /add-dir <path> [more_paths...]".to_string();
        }
        for path in paths {
            if !self
                .state
                .added_dirs
                .iter()
                .any(|existing| existing == &path)
            {
                self.state.added_dirs.push(path);
            }
        }
        format!("added dirs: {}", self.state.added_dirs.len())
    }

    fn list_agents(&self) -> String {
        let store = AgentStore::new();
        match store.list() {
            Ok(agents) if agents.is_empty() => "no agents".to_string(),
            Ok(agents) => agents
                .iter()
                .map(|agent| format!("{} {}", agent.name, agent.description))
                .collect::<Vec<_>>()
                .join("\n"),
            Err(err) => format!("agents failed: {}", err),
        }
    }

    fn login_auth(&self) -> String {
        let config = load_config().unwrap_or_default();
        let provider = if !config.model.provider.trim().is_empty() {
            config.model.provider
        } else {
            "anthropic".to_string()
        };
        let Some(env_name) = auth_env_var_for_provider(&provider) else {
            return format!("login unsupported for provider: {}", provider);
        };
        if std::env::var(env_name).is_err() {
            return format!("{} is not set", env_name);
        }
        match save_tui_auth_session(&provider, env_name) {
            Ok(()) => format!("auth ready: provider={} via {}", provider, env_name),
            Err(err) => format!("login failed: {}", err),
        }
    }

    fn logout_auth(&self) -> String {
        match clear_tui_auth_session() {
            Ok(()) => "auth session cleared".to_string(),
            Err(err) => format!("logout failed: {}", err),
        }
    }

    fn write_bug_report(&self, note: Option<&str>) -> String {
        let path = PathBuf::from(".").join(".tengu").join(format!(
            "bug-report-{}.md",
            chrono::Utc::now().format("%Y%m%d%H%M%S")
        ));
        if let Some(parent) = path.parent() {
            if let Err(err) = fs::create_dir_all(parent) {
                return format!("bug report failed: {}", err);
            }
        }

        let mut report = String::from("# Tengu Bug Report\n\n");
        if let Some(note) = note.filter(|value| !value.trim().is_empty()) {
            report.push_str("## Note\n");
            report.push_str(note.trim());
            report.push_str("\n\n");
        }
        report.push_str("## Status\n");
        report.push_str(&format!(
            "mode: {}\n",
            if self.state.plan_mode {
                "plan"
            } else {
                "default"
            }
        ));
        report.push_str(&format!("queued: {}\n", self.state.queue.len()));
        report.push_str(&format!("log lines: {}\n\n", self.state.log_lines.len()));
        if let Some(plan) = &self.state.last_plan_text {
            report.push_str("## Last Plan\n");
            report.push_str(plan);
            report.push_str("\n\n");
        }
        let recent = self
            .state
            .conversation
            .iter()
            .rev()
            .take(8)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .map(|turn| {
                let role = match turn.role {
                    crate::tui::state::ConversationRole::User => "user",
                    crate::tui::state::ConversationRole::Assistant => "assistant",
                };
                format!("- {}: {}", role, turn.content)
            })
            .collect::<Vec<_>>()
            .join("\n");
        if !recent.is_empty() {
            report.push_str("## Recent Conversation\n");
            report.push_str(&recent);
            report.push('\n');
        }

        match fs::write(&path, report) {
            Ok(()) => format!("bug report written: {}", path.display()),
            Err(err) => format!("bug report failed: {}", err),
        }
    }

    fn show_pr_comments(&self) -> String {
        let output = Command::new("gh")
            .arg("pr")
            .arg("view")
            .arg("--comments")
            .output();
        format_command_result("gh pr view --comments", output)
    }

    fn show_terminal_setup(&self) -> String {
        [
            "Recommended terminal setup:",
            "- truecolor / 24-bit color enabled",
            "- enough scrollback for streaming output",
            "- set $EDITOR or $VISUAL for /editor",
            "- install `gh` if you want /pr and /pr_comments",
        ]
        .join("\n")
    }

    fn show_background_tasks(&self) -> String {
        let mut lines = vec![format!("queued tasks: {}", self.state.queue.len())];
        if self.current_task.is_some() {
            lines.push("active task: running".to_string());
        } else {
            lines.push("active task: idle".to_string());
        }
        if let Some(next) = self.state.queue.front() {
            lines.push(format!("next: {}", next.text));
        }
        lines.join("\n")
    }

    fn show_cost_status(&self) -> String {
        if self.state.provider_usage.is_empty() {
            return [
                "no provider-reported usage recorded yet.".to_string(),
                "usage appears only when the selected provider returns usage metadata.".to_string(),
                "streaming endpoints may omit usage depending on provider behavior.".to_string(),
            ]
            .join("\n");
        }

        let mut lines = vec!["provider usage:".to_string()];
        for record in &self.state.provider_usage {
            lines.push(format!("[{}]", record.provider));
            lines.push(format!("requests: {}", record.requests));
            lines.push(format!("input_tokens: {}", record.input_tokens));
            lines.push(format!("output_tokens: {}", record.output_tokens));
            lines.push(format!("total_tokens: {}", record.total_tokens));
            if record.cache_creation_input_tokens > 0 {
                lines.push(format!(
                    "cache_creation_input_tokens: {}",
                    record.cache_creation_input_tokens
                ));
            }
            if record.cache_read_input_tokens > 0 {
                lines.push(format!(
                    "cache_read_input_tokens: {}",
                    record.cache_read_input_tokens
                ));
            }
            if record.reasoning_tokens > 0 {
                lines.push(format!("reasoning_tokens: {}", record.reasoning_tokens));
            }
            if let Some(raw) = &record.last_raw {
                lines.push(format!("last_raw_usage: {}", raw));
            }
            lines.push(String::new());
        }
        while lines.last().is_some_and(|line| line.is_empty()) {
            lines.pop();
        }
        lines.join("\n")
    }

    fn set_model(&mut self, model: Option<&str>) -> String {
        let Some(model) = model.map(str::trim).filter(|value| !value.is_empty()) else {
            return format!("model: {}", self.state.status_model);
        };

        let mut config = load_config().unwrap_or_default();
        config.model.default = model.to_string();
        let path = local_config_path();
        if let Some(parent) = path.parent() {
            if let Err(err) = fs::create_dir_all(parent) {
                return format!("model update failed: {}", err);
            }
        }
        match toml::to_string_pretty(&config) {
            Ok(content) => match fs::write(&path, content) {
                Ok(()) => {
                    self.state.status_model = model.to_string();
                    format!("default model set: {}", model)
                }
                Err(err) => format!("model update failed: {}", err),
            },
            Err(err) => format!("model update failed: {}", err),
        }
    }

    fn set_strategy(&mut self, strategy: Option<&str>) -> String {
        let selected = strategy.unwrap_or("auto").trim().to_ascii_lowercase();
        match selected.as_str() {
            "plan" => {
                self.state.plan_mode = true;
                "strategy set: plan".to_string()
            }
            "default" | "auto" | "execute" => {
                self.state.plan_mode = false;
                "strategy set: default".to_string()
            }
            other => format!("unsupported strategy: {}", other),
        }
    }

    fn toggle_vim_mode(&mut self) -> String {
        self.state.vim_mode = !self.state.vim_mode;
        format!(
            "vim mode: {}",
            if self.state.vim_mode { "on" } else { "off" }
        )
    }

    fn run_doctor(&self) -> String {
        let checks = [
            ("git", command_exists("git")),
            ("gh", command_exists("gh")),
            ("cargo", command_exists("cargo")),
        ];
        let mut lines = checks
            .iter()
            .map(|(name, ok)| format!("{name}: {}", if *ok { "ok" } else { "missing" }))
            .collect::<Vec<_>>();
        let config_exists = global_config_path().exists() || local_config_path().exists();
        lines.push(format!(
            "config: {}",
            if config_exists { "present" } else { "missing" }
        ));
        lines.push(format!(
            "memory: {}",
            if project_memory_path().exists() {
                "present"
            } else {
                "missing"
            }
        ));
        lines.join("\n")
    }

    fn export_pending_approval(&self) -> Option<SessionPendingApproval> {
        if let Some(action) = &self.pending_local_action {
            let (prompt, kind, args) = match action {
                PendingLocalAction::GitCommit { message } => (
                    format!("Run `git commit -m {:?}`?\n[y] Yes  [n] No", message),
                    "local-commit".to_string(),
                    vec![message.clone()],
                ),
                PendingLocalAction::GhPrCreate { args } => {
                    let prompt = if args.is_empty() {
                        "Run `gh pr create`?\n[y] Yes  [n] No".to_string()
                    } else {
                        format!("Run `gh pr create {}`?\n[y] Yes  [n] No", args.join(" "))
                    };
                    (prompt, "local-pr".to_string(), args.clone())
                }
            };
            return Some(SessionPendingApproval {
                prompt,
                kind,
                tool: None,
                paths: Vec::new(),
                args,
                message: None,
            });
        }
        if let Some(request) = &self.pending_tool_approval {
            let summary = format_tool_summary(request);
            return Some(SessionPendingApproval {
                prompt: format_approval_prompt(request),
                kind: "tool".to_string(),
                tool: Some(summary),
                paths: request
                    .paths
                    .iter()
                    .map(|path| path.display().to_string())
                    .collect(),
                args: Vec::new(),
                message: Some(
                    "The original tool execution cannot be resumed automatically. Rerun the previous prompt after acknowledging.".to_string(),
                ),
            });
        }
        if let Some(restored) = &self.restored_tool_approval {
            return Some(SessionPendingApproval {
                prompt: restored.prompt.clone(),
                kind: "tool".to_string(),
                tool: Some(restored.summary.clone()),
                paths: Vec::new(),
                args: Vec::new(),
                message: Some(restored.message.clone()),
            });
        }
        None
    }

    fn restore_pending_approval(&mut self, pending: Option<SessionPendingApproval>) {
        self.pending_local_action = None;
        self.pending_tool_approval = None;
        self.restored_tool_approval = None;

        let Some(pending) = pending else {
            return;
        };

        match pending.kind.as_str() {
            "local-commit" => {
                let Some(message) = pending.args.first() else {
                    return;
                };
                self.pending_local_action = Some(PendingLocalAction::GitCommit {
                    message: message.clone(),
                });
                self.state.append_message(&pending.prompt);
            }
            "local-pr" => {
                self.pending_local_action = Some(PendingLocalAction::GhPrCreate {
                    args: pending.args.clone(),
                });
                self.state.append_message(&pending.prompt);
            }
            "tool" => {
                self.restored_tool_approval = Some(RestoredToolApproval {
                    prompt: pending.prompt.clone(),
                    summary: pending.tool.unwrap_or_else(|| "tool action".to_string()),
                    message: pending
                        .message
                        .unwrap_or_else(|| "Rerun the previous prompt to continue.".to_string()),
                });
                self.state.append_message(&pending.prompt);
            }
            _ => return,
        }

        self.state.status_state = "running".to_string();
        self.state.status_detail = "approval required".to_string();
        self.state.append_blank_line();
    }

    fn open_editor(&mut self, path: Option<PathBuf>) -> String {
        let editor = std::env::var("VISUAL")
            .ok()
            .or_else(|| std::env::var("EDITOR").ok())
            .unwrap_or_else(|| "vi".to_string());

        let (target_path, capture_prompt) = match path {
            Some(path) => (path, false),
            None => {
                let unique = format!(
                    "tengu-editor-{}.md",
                    chrono::Utc::now().format("%Y%m%d%H%M%S")
                );
                let temp_path = std::env::temp_dir().join(unique);
                if fs::write(&temp_path, "").is_err() {
                    return "failed to prepare temp editor file".to_string();
                }
                (temp_path, true)
            }
        };

        let _ = disable_raw_mode();
        let status = Command::new(&editor).arg(&target_path).status();
        let _ = enable_raw_mode();

        match status {
            Ok(status) if status.success() => {
                if capture_prompt {
                    match fs::read_to_string(&target_path) {
                        Ok(content) => {
                            let prompt = content.trim().to_string();
                            let _ = fs::remove_file(&target_path);
                            if prompt.is_empty() {
                                "editor closed without input".to_string()
                            } else {
                                let images = self.state.take_pending_images();
                                self.state.append_user_message(&format!("> {}", prompt));
                                self.state.queue.push_back(crate::tui::state::PendingInput {
                                    text: prompt,
                                    logged: true,
                                    images,
                                    mode: PendingMode::Execute,
                                });
                                self.touch_current_session();
                                self.maybe_start_next();
                                "submitted editor input".to_string()
                            }
                        }
                        Err(err) => format!("failed to read editor file: {}", err),
                    }
                } else {
                    format!("edited: {}", target_path.display())
                }
            }
            Ok(_) => format!(
                "editor exited with non-zero status: {}",
                target_path.display()
            ),
            Err(err) => format!("failed to launch editor: {}", err),
        }
    }
}

enum SlashCommandOutcome {
    Display(String),
    Submit(String),
    AttachImages(Vec<PathBuf>),
    TogglePlanMode(Option<bool>),
    QueuePlan(String),
    TaskWriter(Option<String>),
    ApplyPlan,
    Compact(Option<String>),
    OpenMemory,
    InitMemory,
    ConfigCommand(Vec<String>),
    ShowBackground,
    ShowCost,
    SetStrategy(Option<String>),
    SetModel(Option<String>),
    AddDir(Vec<PathBuf>),
    ListAgents,
    Login,
    Logout,
    BugReport(Option<String>),
    ShowPrComments,
    TerminalSetup,
    ToggleVim,
    Doctor,
    OpenEditor(Option<PathBuf>),
    Reset {
        response: String,
        new_session: bool,
    },
    ForkSession,
    SaveSession,
    SaveSessionAs(PathBuf),
    Resume(ResumeTarget),
    LoadPath(PathBuf),
    ConfirmLocal {
        prompt: String,
        action: PendingLocalAction,
    },
    Exit(String),
}

enum ResumeTarget {
    List,
    Last,
    ById(String),
}

enum PendingLocalAction {
    GitCommit { message: String },
    GhPrCreate { args: Vec<String> },
}

struct RestoredToolApproval {
    prompt: String,
    summary: String,
    message: String,
}

fn handle_slash_command(input: &str) -> Option<SlashCommandOutcome> {
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
    let args: Vec<&str> = parts.collect();

    match command {
        "/help" => Some(SlashCommandOutcome::Display(build_slash_help())),
        "/plan" => {
            if args.is_empty() {
                Some(SlashCommandOutcome::TogglePlanMode(None))
            } else if matches!(args[0], "on" | "off" | "toggle") {
                Some(SlashCommandOutcome::TogglePlanMode(match args[0] {
                    "on" => Some(true),
                    "off" => Some(false),
                    _ => None,
                }))
            } else {
                Some(SlashCommandOutcome::QueuePlan(args.join(" ")))
            }
        }
        "/taskwriter" => Some(SlashCommandOutcome::TaskWriter(
            (!args.is_empty()).then(|| args.join(" ")),
        )),
        "/apply-plan" => Some(SlashCommandOutcome::ApplyPlan),
        "/model" => Some(SlashCommandOutcome::SetModel(
            (!args.is_empty()).then(|| args.join(" ")),
        )),
        "/add-dir" => {
            if args.is_empty() {
                Some(SlashCommandOutcome::Display(
                    "usage: /add-dir <path> [more_paths...]".to_string(),
                ))
            } else {
                Some(SlashCommandOutcome::AddDir(
                    args.iter().map(PathBuf::from).collect(),
                ))
            }
        }
        "/agents" => Some(SlashCommandOutcome::ListAgents),
        "/login" => Some(SlashCommandOutcome::Login),
        "/logout" => Some(SlashCommandOutcome::Logout),
        "/bug" => Some(SlashCommandOutcome::BugReport(
            (!args.is_empty()).then(|| args.join(" ")),
        )),
        "/pr_comments" => Some(SlashCommandOutcome::ShowPrComments),
        "/terminal-setup" => Some(SlashCommandOutcome::TerminalSetup),
        "/vim" => Some(SlashCommandOutcome::ToggleVim),
        "/bg" => Some(SlashCommandOutcome::ShowBackground),
        "/cost" => Some(SlashCommandOutcome::ShowCost),
        "/strategy" => Some(SlashCommandOutcome::SetStrategy(
            (!args.is_empty()).then(|| args.join(" ")),
        )),
        "/compact" => Some(SlashCommandOutcome::Compact(
            (!args.is_empty()).then(|| args.join(" ")),
        )),
        "/memory" => Some(SlashCommandOutcome::OpenMemory),
        "/init" => Some(SlashCommandOutcome::InitMemory),
        "/config" => Some(SlashCommandOutcome::ConfigCommand(
            args.iter().map(|arg| (*arg).to_string()).collect(),
        )),
        "/doctor" => Some(SlashCommandOutcome::Doctor),
        "/mcp" => list_mcp_servers().ok().map(SlashCommandOutcome::Display),
        "/tools" => Some(SlashCommandOutcome::Display(list_builtin_tools())),
        "/status" => show_status().ok().map(SlashCommandOutcome::Display),
        "/approvals" | "/permissions" => {
            if args.is_empty() {
                show_approvals().ok().map(SlashCommandOutcome::Display)
            } else if matches!(args[0], "plan" | "default" | "accept-edits") {
                Some(SlashCommandOutcome::TogglePlanMode(Some(matches!(
                    args[0], "plan"
                ))))
            } else {
                Some(SlashCommandOutcome::Display(
                    "usage: /permissions [plan|default|accept-edits]".to_string(),
                ))
            }
        }
        "/image" => {
            if args.is_empty() {
                Some(SlashCommandOutcome::Display(
                    "usage: /image <path> [more_paths...]".to_string(),
                ))
            } else {
                Some(SlashCommandOutcome::AttachImages(
                    args.iter().map(PathBuf::from).collect(),
                ))
            }
        }
        "/review" => match parse_review_args(&args) {
            Ok(options) => match build_review_prompt(&options) {
                Ok(Some(prompt)) => Some(SlashCommandOutcome::Submit(prompt)),
                Ok(None) => Some(SlashCommandOutcome::Display(
                    "no diff to review".to_string(),
                )),
                Err(err) => Some(SlashCommandOutcome::Display(format!(
                    "review failed: {}",
                    err
                ))),
            },
            Err(err) => Some(SlashCommandOutcome::Display(format!(
                "review args error: {}",
                err
            ))),
        },
        "/new" => Some(SlashCommandOutcome::Reset {
            response: "started a new local session".to_string(),
            new_session: true,
        }),
        "/clear" => Some(SlashCommandOutcome::Reset {
            response: "cleared local conversation".to_string(),
            new_session: false,
        }),
        "/diff" => match show_git_diff(&args) {
            Ok(diff) => Some(SlashCommandOutcome::Display(diff)),
            Err(err) => Some(SlashCommandOutcome::Display(format!(
                "diff failed: {}",
                err
            ))),
        },
        "/resume" => Some(SlashCommandOutcome::Resume(parse_resume_target(&args))),
        "/save" => match args.as_slice() {
            [] => Some(SlashCommandOutcome::SaveSession),
            [path] => Some(SlashCommandOutcome::SaveSessionAs(PathBuf::from(path))),
            _ => Some(SlashCommandOutcome::Display(
                "usage: /save [path]".to_string(),
            )),
        },
        "/load" => match args.as_slice() {
            [path] => Some(SlashCommandOutcome::LoadPath(PathBuf::from(path))),
            _ => Some(SlashCommandOutcome::Display(
                "usage: /load <path>".to_string(),
            )),
        },
        "/fork" => {
            if args.is_empty() {
                Some(SlashCommandOutcome::ForkSession)
            } else {
                Some(SlashCommandOutcome::Display("usage: /fork".to_string()))
            }
        }
        "/commit" => match parse_commit_action(&args) {
            Ok(outcome) => Some(outcome),
            Err(message) => Some(SlashCommandOutcome::Display(message)),
        },
        "/pr" => Some(build_pr_action(&args)),
        "/editor" => match args.as_slice() {
            [] => Some(SlashCommandOutcome::OpenEditor(None)),
            [path] => Some(SlashCommandOutcome::OpenEditor(Some(PathBuf::from(path)))),
            _ => Some(SlashCommandOutcome::Display(
                "usage: /editor [path]".to_string(),
            )),
        },
        "/exit" | "/quit" => Some(SlashCommandOutcome::Exit("exit requested".to_string())),
        _ => {
            if let Some(expanded) = resolve_custom_command(command, &args) {
                return Some(SlashCommandOutcome::Submit(expanded));
            }
            let filtered = build_slash_help_filtered(command);
            if filtered.is_empty() {
                Some(SlashCommandOutcome::Display(format!(
                    "unknown command: {}",
                    command
                )))
            } else {
                Some(SlashCommandOutcome::Display(filtered))
            }
        }
    }
}

fn resolve_custom_command(input: &str, args: &[&str]) -> Option<String> {
    let (scope, name) = parse_custom_command_name(input)?;
    let template = load_custom_command_template(scope, &name).ok()??;
    Some(expand_custom_command_template(&template, args))
}

fn parse_custom_command_name(input: &str) -> Option<(&'static str, String)> {
    if let Some(name) = input.strip_prefix("/project:") {
        let trimmed = name.trim();
        if trimmed.is_empty() {
            return None;
        }
        return Some(("project", trimmed.to_string()));
    }
    if let Some(name) = input.strip_prefix('/') {
        let trimmed = name.trim();
        if trimmed.is_empty() || trimmed.contains('/') {
            return None;
        }
        return Some(("global", trimmed.to_string()));
    }
    None
}

fn load_custom_command_template(scope: &str, name: &str) -> anyhow::Result<Option<String>> {
    let mut candidates = Vec::new();
    match scope {
        "project" => {
            candidates.push(
                PathBuf::from(".")
                    .join(".tengu")
                    .join("commands")
                    .join(format!("{name}.md")),
            );
        }
        "global" => {
            if let Some(home) = std::env::var_os("HOME") {
                candidates.push(
                    PathBuf::from(home)
                        .join(".tengu")
                        .join("commands")
                        .join(format!("{name}.md")),
                );
            }
            candidates.push(
                PathBuf::from(".")
                    .join(".tengu")
                    .join("commands")
                    .join(format!("{name}.md")),
            );
        }
        _ => {}
    }

    for path in candidates {
        if path.exists() {
            let content = fs::read_to_string(path)?;
            return Ok(Some(strip_frontmatter(&content)));
        }
    }

    Ok(None)
}

fn strip_frontmatter(content: &str) -> String {
    let mut lines = content.lines();
    if lines.next() != Some("---") {
        return content.trim().to_string();
    }

    for line in &mut lines {
        if line.trim() == "---" {
            let body = lines.collect::<Vec<_>>().join("\n");
            return body.trim().to_string();
        }
    }

    content.trim().to_string()
}

fn expand_custom_command_template(template: &str, args: &[&str]) -> String {
    let joined_args = args.join(" ");
    let mut expanded = template
        .replace("{{args}}", &joined_args)
        .replace("$ARGS", &joined_args)
        .replace("$ARGUMENTS", &joined_args);

    for (index, arg) in args.iter().enumerate() {
        expanded = expanded.replace(&format!("${}", index + 1), arg);
    }

    if !joined_args.is_empty()
        && !template.contains("{{args}}")
        && !template.contains("$ARGS")
        && !template.contains("$ARGUMENTS")
        && !args
            .iter()
            .enumerate()
            .any(|(index, _)| template.contains(&format!("${}", index + 1)))
    {
        expanded.push_str("\n\nArguments:\n");
        expanded.push_str(&joined_args);
    }

    expanded
}

fn format_approval_prompt(request: &ToolApprovalRequest) -> String {
    let tool_name = tool_name_label(request.tool);
    let target = if request.paths.is_empty() {
        "target".to_string()
    } else if request.paths.len() == 1 {
        request.paths[0].display().to_string()
    } else {
        format!(
            "{} (+{} more)",
            request.paths[0].display(),
            request.paths.len() - 1
        )
    };
    format!(
        "Allow {} to {}?\n[y] Yes  [n] No  [a] Always allow  [d] Don't ask again",
        tool_name, target
    )
}

fn tool_name_label(tool: Tool) -> &'static str {
    match tool {
        Tool::Read => "Read",
        Tool::Write => "Write",
        Tool::Shell => "Shell",
        Tool::Grep => "Grep",
        Tool::Glob => "Glob",
    }
}

fn format_tool_summary(request: &ToolApprovalRequest) -> String {
    if request.paths.is_empty() {
        tool_name_label(request.tool).to_string()
    } else {
        format!(
            "{} {}",
            tool_name_label(request.tool),
            request.paths[0].display()
        )
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
            cmd: "/plan [on|off|request]",
            desc_en: "Toggle plan mode or generate a plan",
        },
        SlashCommandHelp {
            cmd: "/add-dir <path>",
            desc_en: "Add extra workspace directories",
        },
        SlashCommandHelp {
            cmd: "/agents",
            desc_en: "List custom agents",
        },
        SlashCommandHelp {
            cmd: "/taskwriter [title]",
            desc_en: "Write the last plan into TASK.md",
        },
        SlashCommandHelp {
            cmd: "/apply-plan",
            desc_en: "Run the last planned request",
        },
        SlashCommandHelp {
            cmd: "/model [name]",
            desc_en: "Show or set the default model",
        },
        SlashCommandHelp {
            cmd: "/strategy [mode]",
            desc_en: "Switch execution strategy",
        },
        SlashCommandHelp {
            cmd: "/bg",
            desc_en: "Show running and queued tasks",
        },
        SlashCommandHelp {
            cmd: "/cost",
            desc_en: "Show estimated session usage",
        },
        SlashCommandHelp {
            cmd: "/compact [note]",
            desc_en: "Compact the current conversation",
        },
        SlashCommandHelp {
            cmd: "/memory",
            desc_en: "Open project memory file",
        },
        SlashCommandHelp {
            cmd: "/init",
            desc_en: "Create project memory scaffold",
        },
        SlashCommandHelp {
            cmd: "/login",
            desc_en: "Persist auth state from env vars",
        },
        SlashCommandHelp {
            cmd: "/logout",
            desc_en: "Clear persisted auth state",
        },
        SlashCommandHelp {
            cmd: "/pr_comments",
            desc_en: "Show PR comments via gh",
        },
        SlashCommandHelp {
            cmd: "/terminal-setup",
            desc_en: "Show terminal setup guidance",
        },
        SlashCommandHelp {
            cmd: "/vim",
            desc_en: "Toggle lightweight vim mode flag",
        },
        SlashCommandHelp {
            cmd: "/bug [note]",
            desc_en: "Write a local bug report",
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
            cmd: "/save",
            desc_en: "Save current session metadata",
        },
        SlashCommandHelp {
            cmd: "/load <path>",
            desc_en: "Load session from path",
        },
        SlashCommandHelp {
            cmd: "/image <path>",
            desc_en: "Attach image(s) to the next prompt",
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
            cmd: "/permissions [mode]",
            desc_en: "Switch between default and plan mode",
        },
        SlashCommandHelp {
            cmd: "/status",
            desc_en: "Show current status",
        },
        SlashCommandHelp {
            cmd: "/config",
            desc_en: "Show config and memory paths",
        },
        SlashCommandHelp {
            cmd: "/doctor",
            desc_en: "Run local health checks",
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
            cmd: "/review [opts]",
            desc_en: "Review current git diff with LLM",
        },
        SlashCommandHelp {
            cmd: "/editor [path]",
            desc_en: "Open external editor",
        },
        SlashCommandHelp {
            cmd: "/project:<name>",
            desc_en: "Run a project custom command",
        },
        SlashCommandHelp {
            cmd: "/<name>",
            desc_en: "Run a global or local custom command",
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

fn show_git_diff(args: &[&str]) -> anyhow::Result<String> {
    let mut command = Command::new("git");
    command.arg("diff").arg("--no-ext-diff");

    match args {
        [] => {}
        ["--stat"] => {
            command.arg("--stat");
        }
        _ => {
            return Ok("usage: /diff [--stat]".to_string());
        }
    }

    let output = command.output()?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Ok(format!("git diff failed: {}", stderr.trim()));
    }

    let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if text.is_empty() {
        Ok("no diff".to_string())
    } else {
        Ok(text)
    }
}

fn parse_resume_target(args: &[&str]) -> ResumeTarget {
    match args {
        [] => ResumeTarget::List,
        ["--last"] => ResumeTarget::Last,
        [id] => ResumeTarget::ById((*id).to_string()),
        _ => ResumeTarget::List,
    }
}

fn parse_commit_action(args: &[&str]) -> std::result::Result<SlashCommandOutcome, String> {
    if args.is_empty() {
        return Err("usage: /commit <message>".to_string());
    }
    let message = args.join(" ");
    let prompt = format!("Run `git commit -m {:?}`?\n[y] Yes  [n] No", message);
    Ok(SlashCommandOutcome::ConfirmLocal {
        prompt,
        action: PendingLocalAction::GitCommit { message },
    })
}

fn build_pr_action(args: &[&str]) -> SlashCommandOutcome {
    let prompt = if args.is_empty() {
        "Run `gh pr create`?\n[y] Yes  [n] No".to_string()
    } else {
        format!("Run `gh pr create {}`?\n[y] Yes  [n] No", args.join(" "))
    };
    SlashCommandOutcome::ConfirmLocal {
        prompt,
        action: PendingLocalAction::GhPrCreate {
            args: args.iter().map(|arg| (*arg).to_string()).collect(),
        },
    }
}

fn execute_pending_local_action(action: PendingLocalAction) -> String {
    match action {
        PendingLocalAction::GitCommit { message } => {
            let output = Command::new("git")
                .arg("commit")
                .arg("-m")
                .arg(&message)
                .output();
            format_command_result("git commit", output)
        }
        PendingLocalAction::GhPrCreate { args } => {
            let output = Command::new("gh")
                .arg("pr")
                .arg("create")
                .args(&args)
                .output();
            format_command_result("gh pr create", output)
        }
    }
}

fn format_command_result(label: &str, output: std::io::Result<std::process::Output>) -> String {
    match output {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            if output.status.success() {
                if stdout.is_empty() {
                    format!("{label} succeeded")
                } else {
                    stdout
                }
            } else if stderr.is_empty() {
                format!("{label} failed")
            } else {
                format!("{label} failed: {}", stderr)
            }
        }
        Err(err) => format!("{label} failed: {}", err),
    }
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

fn create_persisted_session(store: Option<&SessionStore>) -> Option<Session> {
    let session = Session::new();
    if let Some(store) = store {
        let _ = store.save(&session);
    }
    Some(session)
}

fn load_tui_image(path: &Path) -> Result<LlmImage> {
    let media_type = tui_image_media_type(path)
        .ok_or_else(|| anyhow!("unsupported image type: {}", path.display()))?;
    let bytes = fs::read(path)?;
    Ok(LlmImage {
        media_type: media_type.to_string(),
        data_base64: base64::engine::general_purpose::STANDARD.encode(bytes),
    })
}

fn tui_image_media_type(path: &Path) -> Option<&'static str> {
    let ext = path.extension()?.to_str()?.to_ascii_lowercase();
    match ext.as_str() {
        "png" => Some("image/png"),
        "jpg" | "jpeg" => Some("image/jpeg"),
        "gif" => Some("image/gif"),
        "webp" => Some("image/webp"),
        _ => None,
    }
}

fn parse_dropped_image_paths(input: &str) -> Option<Vec<PathBuf>> {
    let trimmed = input.trim();
    if trimmed.is_empty() || trimmed.starts_with('/') {
        return None;
    }

    let normalized = trimmed.replace('\n', " ");
    let candidates = normalized
        .split_whitespace()
        .map(|token| token.trim_matches('"').trim_matches('\''))
        .filter(|token| !token.is_empty())
        .map(|token| {
            token
                .strip_prefix("file://")
                .map(percent_decode_basic)
                .unwrap_or_else(|| token.to_string())
        })
        .collect::<Vec<_>>();

    if candidates.is_empty() {
        return None;
    }

    let paths = candidates.iter().map(PathBuf::from).collect::<Vec<_>>();

    if paths
        .iter()
        .all(|path| tui_image_media_type(path).is_some())
    {
        Some(paths)
    } else {
        None
    }
}

fn percent_decode_basic(input: &str) -> String {
    let mut out = String::new();
    let bytes = input.as_bytes();
    let mut idx = 0usize;
    while idx < bytes.len() {
        if bytes[idx] == b'%' && idx + 2 < bytes.len() {
            let hex = &input[idx + 1..idx + 3];
            if let Ok(value) = u8::from_str_radix(hex, 16) {
                out.push(value as char);
                idx += 3;
                continue;
            }
        }
        out.push(bytes[idx] as char);
        idx += 1;
    }
    out
}

fn project_memory_path() -> PathBuf {
    PathBuf::from(".").join(".tengu").join("TENGU.md")
}

fn global_config_path() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".tengu")
        .join("config.toml")
}

fn local_config_path() -> PathBuf {
    PathBuf::from(".").join(".tengu").join("config.toml")
}

fn write_local_config(config: &Config) -> Result<()> {
    let path = local_config_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let content = toml::to_string_pretty(config)?;
    fs::write(path, content)?;
    Ok(())
}

fn command_exists(command: &str) -> bool {
    Command::new(command).arg("--version").output().is_ok()
}

fn auth_env_var_for_provider(provider: &str) -> Option<&'static str> {
    match provider.trim().to_ascii_lowercase().as_str() {
        "anthropic" => Some("ANTHROPIC_API_KEY"),
        "openai" => Some("OPENAI_API_KEY"),
        "google" | "gemini" => Some("GOOGLE_API_KEY"),
        _ => None,
    }
}

fn tui_auth_session_path() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".tengu")
        .join("auth")
        .join("session.json")
}

fn save_tui_auth_session(provider: &str, env_var: &str) -> Result<()> {
    let path = tui_auth_session_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let payload = serde_json::json!({
        "provider": provider,
        "env_var": env_var,
        "updated_at": chrono::Utc::now().to_rfc3339(),
    });
    fs::write(path, serde_json::to_string_pretty(&payload)?)?;
    Ok(())
}

fn clear_tui_auth_session() -> Result<()> {
    let path = tui_auth_session_path();
    if path.exists() {
        fs::remove_file(path)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_frontmatter_from_custom_command() {
        let content = "---\nname: test\ndescription: demo\n---\nRun checks";
        assert_eq!(strip_frontmatter(content), "Run checks");
    }

    #[test]
    fn expands_custom_command_args_placeholder() {
        let template = "Review files: {{args}}";
        let expanded = expand_custom_command_template(template, &["src/main.rs", "src/lib.rs"]);
        assert_eq!(expanded, "Review files: src/main.rs src/lib.rs");
    }

    #[test]
    fn expands_custom_command_positional_placeholders() {
        let template = "Compare $1 with $2 using $ARGUMENTS";
        let expanded = expand_custom_command_template(template, &["src/main.rs", "src/lib.rs"]);
        assert_eq!(
            expanded,
            "Compare src/main.rs with src/lib.rs using src/main.rs src/lib.rs"
        );
    }

    #[test]
    fn parses_project_custom_command_name() {
        let parsed = parse_custom_command_name("/project:security-review");
        assert_eq!(parsed, Some(("project", "security-review".to_string())));
    }

    #[test]
    fn handles_clear_command_as_reset() {
        let outcome = handle_slash_command("/clear");
        assert!(matches!(
            outcome,
            Some(SlashCommandOutcome::Reset { response, new_session: false })
                if response == "cleared local conversation"
        ));
    }

    #[test]
    fn shows_diff_usage_for_unknown_args() {
        let outcome = handle_slash_command("/diff --bad");
        assert!(matches!(
            outcome,
            Some(SlashCommandOutcome::Display(message)) if message == "usage: /diff [--stat]"
        ));
    }

    #[test]
    fn parses_resume_last_command() {
        let outcome = handle_slash_command("/resume --last");
        assert!(matches!(
            outcome,
            Some(SlashCommandOutcome::Resume(ResumeTarget::Last))
        ));
    }

    #[test]
    fn parses_save_command() {
        let outcome = handle_slash_command("/save");
        assert!(matches!(outcome, Some(SlashCommandOutcome::SaveSession)));
    }

    #[test]
    fn parses_save_path_command() {
        let outcome = handle_slash_command("/save /tmp/session.json");
        assert!(matches!(
            outcome,
            Some(SlashCommandOutcome::SaveSessionAs(path)) if path == Path::new("/tmp/session.json")
        ));
    }

    #[test]
    fn parses_load_path_command() {
        let outcome = handle_slash_command("/load ./session.json");
        assert!(matches!(
            outcome,
            Some(SlashCommandOutcome::LoadPath(path)) if path == Path::new("./session.json")
        ));
    }

    #[test]
    fn parses_commit_as_confirmed_local_action() {
        let outcome = handle_slash_command("/commit initial import");
        assert!(matches!(
            outcome,
            Some(SlashCommandOutcome::ConfirmLocal {
                action: PendingLocalAction::GitCommit { .. },
                ..
            })
        ));
    }

    #[test]
    fn parses_pr_as_confirmed_local_action() {
        let outcome = handle_slash_command("/pr --draft");
        assert!(matches!(
            outcome,
            Some(SlashCommandOutcome::ConfirmLocal {
                action: PendingLocalAction::GhPrCreate { .. },
                ..
            })
        ));
    }

    #[test]
    fn parses_editor_without_path() {
        let outcome = handle_slash_command("/editor");
        assert!(matches!(
            outcome,
            Some(SlashCommandOutcome::OpenEditor(None))
        ));
    }

    #[test]
    fn parses_image_command() {
        let outcome = handle_slash_command("/image a.png b.png");
        assert!(matches!(
            outcome,
            Some(SlashCommandOutcome::AttachImages(paths))
                if paths == vec![PathBuf::from("a.png"), PathBuf::from("b.png")]
        ));
    }

    #[test]
    fn detects_dropped_image_paths() {
        let parsed = parse_dropped_image_paths("\"/tmp/a.png\" file:///tmp/b.jpg");
        assert_eq!(
            parsed,
            Some(vec![
                PathBuf::from("/tmp/a.png"),
                PathBuf::from("/tmp/b.jpg")
            ])
        );
    }

    #[test]
    fn ignores_non_image_drop_input() {
        assert!(parse_dropped_image_paths("please review this").is_none());
        assert!(parse_dropped_image_paths("/image a.png").is_none());
    }

    #[test]
    fn parses_plan_toggle_and_request() {
        assert!(matches!(
            handle_slash_command("/plan on"),
            Some(SlashCommandOutcome::TogglePlanMode(Some(true)))
        ));
        assert!(matches!(
            handle_slash_command("/plan fix auth flow"),
            Some(SlashCommandOutcome::QueuePlan(request)) if request == "fix auth flow"
        ));
    }

    #[test]
    fn parses_taskwriter_and_apply_plan() {
        assert!(matches!(
            handle_slash_command("/taskwriter Phase 2"),
            Some(SlashCommandOutcome::TaskWriter(Some(title))) if title == "Phase 2"
        ));
        assert!(matches!(
            handle_slash_command("/apply-plan"),
            Some(SlashCommandOutcome::ApplyPlan)
        ));
    }

    #[test]
    fn parses_strategy_and_bg_commands() {
        assert!(matches!(
            handle_slash_command("/strategy plan"),
            Some(SlashCommandOutcome::SetStrategy(Some(strategy))) if strategy == "plan"
        ));
        assert!(matches!(
            handle_slash_command("/bg"),
            Some(SlashCommandOutcome::ShowBackground)
        ));
        assert!(matches!(
            handle_slash_command("/cost"),
            Some(SlashCommandOutcome::ShowCost)
        ));
    }

    #[test]
    fn parses_agents_and_auth_commands() {
        assert!(matches!(
            handle_slash_command("/agents"),
            Some(SlashCommandOutcome::ListAgents)
        ));
        assert!(matches!(
            handle_slash_command("/login"),
            Some(SlashCommandOutcome::Login)
        ));
        assert!(matches!(
            handle_slash_command("/logout"),
            Some(SlashCommandOutcome::Logout)
        ));
        assert!(matches!(
            handle_slash_command("/pr_comments"),
            Some(SlashCommandOutcome::ShowPrComments)
        ));
        assert!(matches!(
            handle_slash_command("/terminal-setup"),
            Some(SlashCommandOutcome::TerminalSetup)
        ));
        assert!(matches!(
            handle_slash_command("/vim"),
            Some(SlashCommandOutcome::ToggleVim)
        ));
        assert!(matches!(
            handle_slash_command("/model claude-sonnet-4"),
            Some(SlashCommandOutcome::SetModel(Some(model))) if model == "claude-sonnet-4"
        ));
    }

    #[test]
    fn parses_config_subcommands() {
        assert!(matches!(
            handle_slash_command("/config"),
            Some(SlashCommandOutcome::ConfigCommand(args)) if args.is_empty()
        ));
        assert!(matches!(
            handle_slash_command("/config get model.default"),
            Some(SlashCommandOutcome::ConfigCommand(args))
                if args == vec!["get".to_string(), "model.default".to_string()]
        ));
        assert!(matches!(
            handle_slash_command("/config set plan_mode on"),
            Some(SlashCommandOutcome::ConfigCommand(args))
                if args == vec!["set".to_string(), "plan_mode".to_string(), "on".to_string()]
        ));
    }
}
