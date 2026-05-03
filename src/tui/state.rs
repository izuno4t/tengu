use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::mpsc;

use crate::llm::{LlmImage, LlmUsage};
use crate::session::{
    SessionConversationRole, SessionConversationTurn, SessionImage, SessionLogLine, SessionLogRole,
    SessionPendingInput, SessionUsageRecord,
};
use crate::tools::{ToolApprovalDecision, ToolApprovalRequest};
use crate::tui::InlineRenderState;
use tokio::sync::oneshot;

pub enum TuiEvent {
    Chunk(String),
    Usage(LlmUsage),
    Done,
    PlanResult {
        request: String,
        plan: String,
    },
    ApprovalRequest {
        request: ToolApprovalRequest,
        respond_to: oneshot::Sender<ToolApprovalDecision>,
    },
    AgentToolCall {
        name: String,
        input: String,
    },
    AgentToolResult {
        name: String,
        result: String,
        is_error: bool,
    },
}

#[derive(Debug, Clone, Default)]
pub struct UsageStats {
    pub prompt_count: u64,
    pub plan_count: u64,
    pub image_count: u64,
    pub approval_count: u64,
    pub input_chars: u64,
    pub output_chars: u64,
}

#[derive(Debug, Clone)]
pub struct ProviderUsageRecord {
    pub provider: String,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub total_tokens: u64,
    pub cache_creation_input_tokens: u64,
    pub cache_read_input_tokens: u64,
    pub reasoning_tokens: u64,
    pub requests: u64,
    pub last_raw: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PendingMode {
    Execute,
    Plan,
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
    pub images: Vec<LlmImage>,
    pub mode: PendingMode,
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
    pub banner_lines: Vec<String>,
    pub input: String,
    pub input_cursor: usize,
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
    pub pending_images: Vec<LlmImage>,
    pub plan_mode: bool,
    pub last_plan_request: Option<String>,
    pub last_plan_text: Option<String>,
    pub last_plan_items: Vec<String>,
    pub added_dirs: Vec<PathBuf>,
    pub recent_files: Vec<String>,
    pub vim_mode: bool,
    pub usage: UsageStats,
    pub provider_usage: Vec<ProviderUsageRecord>,
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
        let banner_lines = banner
            .lines()
            .map(|line| line.to_string())
            .collect::<Vec<_>>();
        for line in &banner_lines {
            log_lines.push_back(LogLine {
                role: LogRole::System,
                text: line.clone(),
            });
        }
        log_lines.push_back(LogLine {
            role: LogRole::System,
            text: String::new(),
        });
        Self {
            should_quit: false,
            log_lines,
            banner_lines,
            input: String::new(),
            input_cursor: 0,
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
            pending_images: Vec::new(),
            plan_mode: false,
            last_plan_request: None,
            last_plan_text: None,
            last_plan_items: Vec::new(),
            added_dirs: Vec::new(),
            recent_files: Vec::new(),
            vim_mode: false,
            usage: UsageStats::default(),
            provider_usage: Vec::new(),
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
        if !self.added_dirs.is_empty() {
            parts.push(format!(
                "追加ワークスペースディレクトリ:\n{}",
                self.added_dirs
                    .iter()
                    .map(|path| format!("- {}", path.display()))
                    .collect::<Vec<_>>()
                    .join("\n")
            ));
        }
        for turn in self.conversation.iter().skip(start) {
            let role = match turn.role {
                ConversationRole::User => "ユーザー",
                ConversationRole::Assistant => "アシスタント",
            };
            parts.push(format!("{}: {}", role, turn.content));
        }
        parts.join("\n")
    }

    pub fn input_visual_row_count(&self, width: usize) -> u16 {
        let content_width = input_content_width(width);
        self.input
            .split('\n')
            .map(|line| wrapped_plain_line_count(line, content_width))
            .sum::<usize>()
            .max(1) as u16
    }

    pub fn set_input(&mut self, input: String) {
        self.input = input;
        self.input_cursor = self.input.chars().count();
    }

    pub fn clear_input(&mut self) {
        self.input.clear();
        self.input_cursor = 0;
    }

    pub fn cancel_history_navigation(&mut self) {
        self.history_index = None;
        self.draft_input.clear();
    }

    pub fn insert_input_char(&mut self, ch: char) {
        let byte_index = input_byte_index(&self.input, self.input_cursor);
        self.input.insert(byte_index, ch);
        self.input_cursor = self.input_cursor.saturating_add(1);
    }

    pub fn insert_input_text(&mut self, text: &str) {
        for ch in text.chars() {
            self.insert_input_char(ch);
        }
    }

    pub fn input_has_multiple_lines(&self) -> bool {
        self.input.contains('\n')
    }

    pub fn backspace_input_char(&mut self) {
        if self.input_cursor == 0 {
            return;
        }
        let start = input_byte_index(&self.input, self.input_cursor.saturating_sub(1));
        let end = input_byte_index(&self.input, self.input_cursor);
        self.input.replace_range(start..end, "");
        self.input_cursor = self.input_cursor.saturating_sub(1);
    }

    pub fn delete_input_char(&mut self) {
        let char_len = self.input.chars().count();
        if self.input_cursor >= char_len {
            return;
        }
        let start = input_byte_index(&self.input, self.input_cursor);
        let end = input_byte_index(&self.input, self.input_cursor.saturating_add(1));
        self.input.replace_range(start..end, "");
    }

    pub fn move_input_cursor_left(&mut self) {
        self.input_cursor = self.input_cursor.saturating_sub(1);
    }

    pub fn move_input_cursor_right(&mut self) {
        self.input_cursor = self
            .input_cursor
            .saturating_add(1)
            .min(self.input.chars().count());
    }

    pub fn move_input_cursor_start(&mut self) {
        self.input_cursor = 0;
    }

    pub fn move_input_cursor_end(&mut self) {
        self.input_cursor = self.input.chars().count();
    }

    pub fn move_input_cursor_line_start(&mut self) {
        self.input_cursor = input_line_start(&self.input, self.input_cursor);
    }

    pub fn move_input_cursor_line_end(&mut self) {
        let line_start = input_line_start(&self.input, self.input_cursor);
        self.input_cursor = input_line_end(&self.input, line_start);
    }

    pub fn move_input_cursor_previous_line(&mut self) -> bool {
        let Some((line_start, column)) =
            input_line_start_and_column(&self.input, self.input_cursor)
        else {
            return false;
        };
        if line_start == 0 {
            return false;
        }
        let previous_end = line_start.saturating_sub(1);
        let previous_start = input_line_start(&self.input, previous_end);
        let previous_len = previous_end.saturating_sub(previous_start);
        self.input_cursor = previous_start.saturating_add(column.min(previous_len));
        true
    }

    pub fn move_input_cursor_next_line(&mut self) -> bool {
        let Some((line_start, column)) =
            input_line_start_and_column(&self.input, self.input_cursor)
        else {
            return false;
        };
        let line_end = input_line_end(&self.input, line_start);
        let char_len = self.input.chars().count();
        if line_end >= char_len {
            return false;
        }
        let next_start = line_end.saturating_add(1);
        let next_end = input_line_end(&self.input, next_start);
        let next_len = next_end.saturating_sub(next_start);
        self.input_cursor = next_start.saturating_add(column.min(next_len));
        true
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

    pub fn record_pending_usage(&mut self, pending: &PendingInput) {
        self.usage.input_chars = self
            .usage
            .input_chars
            .saturating_add(pending.text.chars().count() as u64);
        self.usage.image_count = self
            .usage
            .image_count
            .saturating_add(pending.images.len() as u64);
        match pending.mode {
            PendingMode::Execute => {
                self.usage.prompt_count = self.usage.prompt_count.saturating_add(1);
            }
            PendingMode::Plan => {
                self.usage.plan_count = self.usage.plan_count.saturating_add(1);
            }
        }
    }

    pub fn record_output_chunk(&mut self, text: &str) {
        self.usage.output_chars = self
            .usage
            .output_chars
            .saturating_add(text.chars().count() as u64);
    }

    pub fn record_approval_request(&mut self) {
        self.usage.approval_count = self.usage.approval_count.saturating_add(1);
    }

    pub fn record_provider_usage(&mut self, usage: &LlmUsage) {
        let input_tokens = usage.input_tokens.unwrap_or(0);
        let output_tokens = usage.output_tokens.unwrap_or(0);
        let total_tokens = usage
            .total_tokens
            .unwrap_or_else(|| input_tokens.saturating_add(output_tokens));
        let cache_creation_input_tokens = usage.cache_creation_input_tokens.unwrap_or(0);
        let cache_read_input_tokens = usage.cache_read_input_tokens.unwrap_or(0);
        let reasoning_tokens = usage.reasoning_tokens.unwrap_or(0);
        let last_raw = usage
            .raw
            .as_ref()
            .and_then(|value| serde_json::to_string(value).ok());

        if let Some(record) = self
            .provider_usage
            .iter_mut()
            .find(|record| record.provider == usage.provider)
        {
            record.input_tokens = record.input_tokens.saturating_add(input_tokens);
            record.output_tokens = record.output_tokens.saturating_add(output_tokens);
            record.total_tokens = record.total_tokens.saturating_add(total_tokens);
            record.cache_creation_input_tokens = record
                .cache_creation_input_tokens
                .saturating_add(cache_creation_input_tokens);
            record.cache_read_input_tokens = record
                .cache_read_input_tokens
                .saturating_add(cache_read_input_tokens);
            record.reasoning_tokens = record.reasoning_tokens.saturating_add(reasoning_tokens);
            record.requests = record.requests.saturating_add(1);
            if last_raw.is_some() {
                record.last_raw = last_raw;
            }
            return;
        }

        self.provider_usage.push(ProviderUsageRecord {
            provider: usage.provider.clone(),
            input_tokens,
            output_tokens,
            total_tokens,
            cache_creation_input_tokens,
            cache_read_input_tokens,
            reasoning_tokens,
            requests: 1,
            last_raw,
        });
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

    pub fn set_pending_images(&mut self, images: Vec<LlmImage>) {
        self.pending_images = images;
    }

    pub fn set_added_dirs(&mut self, paths: Vec<PathBuf>) {
        self.added_dirs = paths;
    }

    pub fn set_recent_files(&mut self, files: Vec<String>) {
        self.recent_files = files;
        self.truncate_recent_files();
    }

    pub fn record_recent_files(&mut self, files: Vec<String>) {
        for file in files.into_iter().rev() {
            self.recent_files.retain(|existing| existing != &file);
            self.recent_files.insert(0, file);
        }
        self.truncate_recent_files();
    }

    fn truncate_recent_files(&mut self) {
        const MAX_RECENT_FILES: usize = 50;
        self.recent_files.truncate(MAX_RECENT_FILES);
    }

    pub fn take_pending_images(&mut self) -> Vec<LlmImage> {
        std::mem::take(&mut self.pending_images)
    }

    pub fn export_conversation(&self) -> Vec<SessionConversationTurn> {
        self.conversation
            .iter()
            .map(|turn| SessionConversationTurn {
                role: match turn.role {
                    ConversationRole::User => SessionConversationRole::User,
                    ConversationRole::Assistant => SessionConversationRole::Assistant,
                },
                content: turn.content.clone(),
            })
            .collect()
    }

    pub fn export_log_lines(&self) -> Vec<SessionLogLine> {
        self.log_lines
            .iter()
            .map(|line| SessionLogLine {
                role: match line.role {
                    LogRole::User => SessionLogRole::User,
                    LogRole::Assistant => SessionLogRole::Assistant,
                    LogRole::System => SessionLogRole::System,
                },
                text: line.text.clone(),
            })
            .collect()
    }

    pub fn export_queue(&self) -> Vec<SessionPendingInput> {
        self.queue
            .iter()
            .map(|item| SessionPendingInput {
                text: item.text.clone(),
                logged: item.logged,
                images: item
                    .images
                    .iter()
                    .map(|image| SessionImage {
                        media_type: image.media_type.clone(),
                        data_base64: image.data_base64.clone(),
                    })
                    .collect(),
            })
            .collect()
    }

    pub fn export_pending_images(&self) -> Vec<SessionImage> {
        self.pending_images
            .iter()
            .map(|image| SessionImage {
                media_type: image.media_type.clone(),
                data_base64: image.data_base64.clone(),
            })
            .collect()
    }

    pub fn export_usage_records(&self) -> Vec<SessionUsageRecord> {
        self.provider_usage
            .iter()
            .map(|record| SessionUsageRecord {
                provider: record.provider.clone(),
                input_tokens: record.input_tokens,
                output_tokens: record.output_tokens,
                total_tokens: record.total_tokens,
                cache_creation_input_tokens: record.cache_creation_input_tokens,
                cache_read_input_tokens: record.cache_read_input_tokens,
                reasoning_tokens: record.reasoning_tokens,
                requests: record.requests,
                last_raw: record.last_raw.clone(),
            })
            .collect()
    }

    pub fn export_recent_files(&self) -> Vec<String> {
        self.recent_files.clone()
    }

    pub fn restore_from_session(
        &mut self,
        conversation: &[SessionConversationTurn],
        log_lines: &[SessionLogLine],
        queue: &[SessionPendingInput],
        pending_images: &[SessionImage],
        usage_records: &[SessionUsageRecord],
        recent_files: &[String],
    ) {
        self.reset_session_view();
        if !log_lines.is_empty() {
            self.log_lines = log_lines
                .iter()
                .map(|line| LogLine {
                    role: match line.role {
                        SessionLogRole::User => LogRole::User,
                        SessionLogRole::Assistant => LogRole::Assistant,
                        SessionLogRole::System => LogRole::System,
                    },
                    text: line.text.clone(),
                })
                .collect();
        }
        self.conversation = conversation
            .iter()
            .map(|turn| ConversationTurn {
                role: match turn.role {
                    SessionConversationRole::User => ConversationRole::User,
                    SessionConversationRole::Assistant => ConversationRole::Assistant,
                },
                content: turn.content.clone(),
            })
            .collect();
        self.queue = queue
            .iter()
            .map(|item| PendingInput {
                text: item.text.clone(),
                logged: item.logged,
                images: item
                    .images
                    .iter()
                    .map(|image| LlmImage {
                        media_type: image.media_type.clone(),
                        data_base64: image.data_base64.clone(),
                    })
                    .collect(),
                mode: PendingMode::Execute,
            })
            .collect();
        self.pending_images = pending_images
            .iter()
            .map(|image| LlmImage {
                media_type: image.media_type.clone(),
                data_base64: image.data_base64.clone(),
            })
            .collect();
        self.provider_usage = usage_records
            .iter()
            .map(|record| ProviderUsageRecord {
                provider: record.provider.clone(),
                input_tokens: record.input_tokens,
                output_tokens: record.output_tokens,
                total_tokens: record.total_tokens,
                cache_creation_input_tokens: record.cache_creation_input_tokens,
                cache_read_input_tokens: record.cache_read_input_tokens,
                reasoning_tokens: record.reasoning_tokens,
                requests: record.requests,
                last_raw: record.last_raw.clone(),
            })
            .collect();
        self.set_recent_files(recent_files.to_vec());
    }

    pub fn store_plan(&mut self, request: String, plan: String) {
        self.last_plan_items = extract_plan_items(&plan);
        self.last_plan_request = Some(request);
        self.last_plan_text = Some(plan);
    }

    pub fn reset_session_view(&mut self) {
        self.log_lines.clear();
        for line in &self.banner_lines {
            self.log_lines.push_back(LogLine {
                role: LogRole::System,
                text: line.clone(),
            });
        }
        self.log_lines.push_back(LogLine {
            role: LogRole::System,
            text: String::new(),
        });
        self.queue.clear();
        self.conversation.clear();
        self.current_assistant.clear();
        self.input.clear();
        self.input_cursor = 0;
        self.suggestions.clear();
        self.draft_input.clear();
        self.history_index = None;
        self.approval_pending = None;
        self.pending_images.clear();
        self.added_dirs.clear();
        self.recent_files.clear();
        self.vim_mode = false;
        self.provider_usage.clear();
        self.set_idle();
    }
}

fn extract_plan_items(plan: &str) -> Vec<String> {
    let mut items = Vec::new();
    for line in plan.lines() {
        let trimmed = line.trim();
        let item = trimmed
            .strip_prefix("- ")
            .or_else(|| trimmed.strip_prefix("* "))
            .or_else(|| trimmed.strip_prefix("[ ] "))
            .or_else(|| {
                let mut chars = trimmed.chars();
                let mut digits = String::new();
                while let Some(ch) = chars.next() {
                    if ch.is_ascii_digit() {
                        digits.push(ch);
                        continue;
                    }
                    if ch == '.' && !digits.is_empty() {
                        let rest = chars.as_str().trim_start();
                        return Some(rest);
                    }
                    break;
                }
                None
            });
        if let Some(item) = item {
            if !item.trim().is_empty() {
                items.push(item.trim().to_string());
            }
        }
    }
    if items.is_empty() && !plan.trim().is_empty() {
        items.push(plan.trim().to_string());
    }
    items
}

fn input_byte_index(input: &str, char_index: usize) -> usize {
    input
        .char_indices()
        .nth(char_index)
        .map(|(idx, _)| idx)
        .unwrap_or(input.len())
}

fn input_line_start_and_column(input: &str, cursor: usize) -> Option<(usize, usize)> {
    if cursor > input.chars().count() {
        return None;
    }
    let start = input_line_start(input, cursor);
    Some((start, cursor.saturating_sub(start)))
}

fn input_line_start(input: &str, cursor: usize) -> usize {
    let mut start = 0usize;
    for (idx, ch) in input.chars().enumerate().take(cursor) {
        if ch == '\n' {
            start = idx.saturating_add(1);
        }
    }
    start
}

fn input_line_end(input: &str, start: usize) -> usize {
    input
        .chars()
        .enumerate()
        .skip(start)
        .find_map(|(idx, ch)| if ch == '\n' { Some(idx) } else { None })
        .unwrap_or_else(|| input.chars().count())
}

fn input_content_width(width: usize) -> usize {
    width.saturating_sub(3).max(1)
}

fn wrapped_plain_line_count(line: &str, width: usize) -> usize {
    if line.is_empty() {
        return 1;
    }
    let mut rows = 1usize;
    let mut current_width = 0usize;
    for ch in line.chars() {
        let ch_width = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0);
        if current_width > 0 && current_width + ch_width > width {
            rows = rows.saturating_add(1);
            current_width = 0;
        }
        current_width = current_width.saturating_add(ch_width).min(width);
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    fn test_state() -> AppState {
        let (tx, rx) = mpsc::channel();
        AppState::new(
            "banner".to_string(),
            "model".to_string(),
            "build".to_string(),
            rx,
            tx,
        )
    }

    #[test]
    fn input_editing_inserts_and_deletes_at_cursor() {
        let mut state = test_state();
        state.set_input("ac".to_string());
        state.move_input_cursor_left();
        state.insert_input_char('b');
        assert_eq!(state.input, "abc");
        assert_eq!(state.input_cursor, 2);

        state.backspace_input_char();
        assert_eq!(state.input, "ac");
        assert_eq!(state.input_cursor, 1);

        state.delete_input_char();
        assert_eq!(state.input, "a");
        assert_eq!(state.input_cursor, 1);
    }

    #[test]
    fn input_editing_handles_multibyte_characters() {
        let mut state = test_state();
        state.set_input("あう".to_string());
        state.move_input_cursor_left();
        state.insert_input_char('い');
        assert_eq!(state.input, "あいう");
        assert_eq!(state.input_cursor, 2);

        state.backspace_input_char();
        assert_eq!(state.input, "あう");
        state.delete_input_char();
        assert_eq!(state.input, "あ");
    }

    #[test]
    fn input_editing_inserts_pasted_text_at_cursor() {
        let mut state = test_state();
        state.set_input("ac".to_string());
        state.move_input_cursor_left();

        state.insert_input_text("b\nあ");

        assert_eq!(state.input, "ab\nあc");
        assert_eq!(state.input_cursor, 4);
    }

    #[test]
    fn input_visual_row_count_wraps_long_lines() {
        let mut state = test_state();
        state.set_input("abcdef".to_string());
        assert_eq!(state.input_visual_row_count(8), 2);

        state.set_input("abc\ndef".to_string());
        assert_eq!(state.input_visual_row_count(20), 2);

        state.set_input("あa".to_string());
        assert_eq!(state.input_visual_row_count(4), 2);
    }

    #[test]
    fn cancel_history_navigation_clears_selection_and_draft() {
        let mut state = test_state();
        state.history_index = Some(1);
        state.draft_input = "draft".to_string();

        state.cancel_history_navigation();

        assert_eq!(state.history_index, None);
        assert!(state.draft_input.is_empty());
    }

    #[test]
    fn input_cursor_moves_between_physical_lines() {
        let mut state = test_state();
        state.set_input("abc\ndefg\nhi".to_string());
        state.input_cursor = 6;

        assert!(state.move_input_cursor_previous_line());
        assert_eq!(state.input_cursor, 2);

        assert!(state.move_input_cursor_next_line());
        assert_eq!(state.input_cursor, 6);

        assert!(state.move_input_cursor_next_line());
        assert_eq!(state.input_cursor, 11);
    }

    #[test]
    fn input_cursor_line_moves_report_edges() {
        let mut state = test_state();
        state.set_input("abc\ndef".to_string());
        state.input_cursor = 1;
        assert!(!state.move_input_cursor_previous_line());
        assert_eq!(state.input_cursor, 1);

        state.input_cursor = 5;
        assert!(!state.move_input_cursor_next_line());
        assert_eq!(state.input_cursor, 5);
    }

    #[test]
    fn input_cursor_moves_to_current_line_boundaries() {
        let mut state = test_state();
        state.set_input("abc\ndefg\nhi".to_string());
        state.input_cursor = 6;

        state.move_input_cursor_line_start();
        assert_eq!(state.input_cursor, 4);

        state.move_input_cursor_line_end();
        assert_eq!(state.input_cursor, 8);

        state.move_input_cursor_start();
        assert_eq!(state.input_cursor, 0);

        state.move_input_cursor_end();
        assert_eq!(state.input_cursor, 11);
    }
}
