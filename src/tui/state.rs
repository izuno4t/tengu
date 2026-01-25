use std::collections::VecDeque;
use std::sync::mpsc;

use crate::tui::InlineRenderState;
use crate::tools::{ToolApprovalDecision, ToolApprovalRequest};
use tokio::sync::oneshot;

pub enum TuiEvent {
    Chunk(String),
    Done,
    ApprovalRequest {
        request: ToolApprovalRequest,
        respond_to: oneshot::Sender<ToolApprovalDecision>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConversationRole {
    User,
    Assistant,
}

#[derive(Debug, Clone)]
pub struct ConversationTurn {
    pub role: ConversationRole,
    pub content: String,
}

#[derive(Debug, Clone)]
pub struct PendingInput {
    pub text: String,
    pub logged: bool,
}

pub struct ApprovalPending {
    pub respond_to: oneshot::Sender<ToolApprovalDecision>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogRole {
    User,
    Assistant,
    System,
}

#[derive(Debug, Clone)]
pub struct LogLine {
    pub role: LogRole,
    pub text: String,
}

pub struct AppState {
    pub should_quit: bool,
    pub log_lines: VecDeque<LogLine>,
    pub input: String,
    pub suggestions: String,
    pub origin_y: u16,
    pub inline: InlineRenderState,
    pub status_model: String,
    pub status_build: String,
    pub status_state: String,
    pub status_detail: String,
    pub tick: u64,
    pub queue: VecDeque<PendingInput>,
    pub result_rx: mpsc::Receiver<anyhow::Result<TuiEvent>>,
    pub result_tx: mpsc::Sender<anyhow::Result<TuiEvent>>,
    pub history: Vec<String>,
    pub history_index: Option<usize>,
    pub draft_input: String,
    pub approval_pending: Option<ApprovalPending>,
    pub conversation: Vec<ConversationTurn>,
    pub current_assistant: String,
}

impl AppState {
    pub fn new(
        banner: String,
        status_model: String,
        status_build: String,
        result_rx: mpsc::Receiver<anyhow::Result<TuiEvent>>,
        result_tx: mpsc::Sender<anyhow::Result<TuiEvent>>,
    ) -> Self {
        let mut log_lines = VecDeque::new();
        for line in banner.lines() {
            log_lines.push_back(LogLine {
                role: LogRole::System,
                text: line.to_string(),
            });
        }
        Self {
            should_quit: false,
            log_lines,
            input: String::new(),
            suggestions: String::new(),
            origin_y: 0,
            inline: InlineRenderState::default(),
            status_model,
            status_build,
            status_state: "idle".to_string(),
            status_detail: "idle".to_string(),
            tick: 0,
            queue: VecDeque::new(),
            result_rx,
            result_tx,
            history: Vec::new(),
            history_index: None,
            draft_input: String::new(),
            approval_pending: None,
            conversation: Vec::new(),
            current_assistant: String::new(),
        }
    }

    pub fn append_message(&mut self, text: &str) {
        self.append_message_with_role(text, LogRole::Assistant);
    }

    pub fn append_user_message(&mut self, text: &str) {
        self.append_message_with_role(text, LogRole::User);
    }

    pub fn append_stream_chunk(&mut self, text: &str) {
        let mut iter = text.split('\n');
        if let Some(first) = iter.next() {
            match self.log_lines.back_mut() {
                Some(last) if last.role == LogRole::Assistant => last.text.push_str(first),
                _ => self.log_lines.push_back(LogLine {
                    role: LogRole::Assistant,
                    text: first.to_string(),
                }),
            }
        }
        for rest in iter {
            self.log_lines.push_back(LogLine {
                role: LogRole::Assistant,
                text: rest.to_string(),
            });
        }
    }

    pub fn append_assistant_chunk(&mut self, text: &str) {
        self.current_assistant.push_str(text);
    }

    pub fn start_assistant_response(&mut self) {
        self.current_assistant.clear();
    }

    pub fn finalize_assistant_response(&mut self) {
        if !self.current_assistant.trim().is_empty() {
            self.conversation.push(ConversationTurn {
                role: ConversationRole::Assistant,
                content: self.current_assistant.trim().to_string(),
            });
        }
        self.current_assistant.clear();
        self.append_blank_line();
    }

    pub fn push_user_conversation(&mut self, text: &str) {
        self.conversation.push(ConversationTurn {
            role: ConversationRole::User,
            content: text.to_string(),
        });
    }

    pub fn build_context(&self, max_turns: usize) -> String {
        let start = self.conversation.len().saturating_sub(max_turns);
        let mut parts = Vec::new();
        for turn in self.conversation.iter().skip(start) {
            let role = match turn.role {
                ConversationRole::User => "ユーザー",
                ConversationRole::Assistant => "アシスタント",
            };
            parts.push(format!("{}: {}", role, turn.content));
        }
        parts.join("\n")
    }

    pub fn input_row_count(&self) -> u16 {
        let count = self.input.split('\n').count();
        count.max(1) as u16
    }

    pub fn visible_log_lines(&self, height: u16) -> Vec<LogLine> {
        if height == 0 {
            return Vec::new();
        }
        let max = height as usize;
        if self.log_lines.len() <= max {
            return self.log_lines.iter().cloned().collect::<Vec<_>>();
        }
        let start = self.log_lines.len().saturating_sub(max);
        self.log_lines
            .iter()
            .skip(start)
            .cloned()
            .collect::<Vec<_>>()
    }

    pub fn set_running(&mut self, detail: &str) {
        self.status_state = "running".to_string();
        self.status_detail = detail.to_string();
    }

    pub fn set_idle(&mut self) {
        self.status_state = "idle".to_string();
        self.status_detail = "idle".to_string();
    }

    fn append_message_with_role(&mut self, text: &str, role: LogRole) {
        for line in text.lines() {
            self.log_lines.push_back(LogLine {
                role,
                text: line.to_string(),
            });
        }
    }

    pub fn append_blank_line(&mut self) {
        self.log_lines.push_back(LogLine {
            role: LogRole::System,
            text: String::new(),
        });
    }
}
