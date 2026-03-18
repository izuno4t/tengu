// Tools module
// ビルトインツール: Read, Edit, Write, Bash, Grep, Glob

use anyhow::{anyhow, Result};
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};
use crate::config::{Config, PermissionsConfig, SandboxConfig};
use crate::llm::ToolDefinition;

const BASH_TIMEOUT_SECS: u64 = 120;
const BASH_MAX_OUTPUT_BYTES: usize = 30 * 1024; // 30 KB
const WRITE_MAX_BYTES: usize = 5 * 1024 * 1024; // 5 MB
const READ_MAX_LINE_CHARS: usize = 2000;

/// Patterns that indicate dangerous shell commands.
const DANGEROUS_PATTERNS: &[&str] = &[
    "rm -rf /",
    "rm -rf /*",
    "rm -rf ~",
    "mkfs.",
    "dd if=",
    ":(){:|:&};:",
    "> /dev/sda",
    "chmod -R 777 /",
    "shutdown",
    "reboot",
    "halt",
    "init 0",
    "init 6",
];

/// Environment variable prefixes/names to filter from child processes.
const SENSITIVE_ENV_PREFIXES: &[&str] = &[
    "ANTHROPIC_API_KEY",
    "OPENAI_API_KEY",
    "GROQ_API_KEY",
    "GOOGLE_API_KEY",
    "AWS_SECRET_ACCESS_KEY",
    "GITHUB_TOKEN",
    "GH_TOKEN",
    "DATABASE_URL",
    "HF_TOKEN",
    "HUGGING_FACE_HUB_TOKEN",
];

/// Image file extensions.
const IMAGE_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "gif", "bmp", "webp", "svg", "ico"];

/// Check if a command matches any dangerous pattern.
fn is_dangerous_command(command: &str) -> bool {
    let lower = command.to_ascii_lowercase();
    DANGEROUS_PATTERNS
        .iter()
        .any(|p| lower.contains(&p.to_ascii_lowercase()))
}

/// Build a filtered environment for child processes.
fn build_clean_env() -> Vec<(String, String)> {
    std::env::vars()
        .filter(|(key, _)| {
            !SENSITIVE_ENV_PREFIXES
                .iter()
                .any(|prefix| key == *prefix || key.starts_with(&format!("{}_", prefix)))
        })
        .collect()
}

/// Check if file content appears to be binary.
fn is_binary_content(data: &[u8]) -> bool {
    let check_len = data.len().min(8192);
    data[..check_len].contains(&0)
}

/// Check if a file path has an image extension.
fn is_image_path(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| IMAGE_EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
        .unwrap_or(false)
}

/// Estimate token count for a string (CJK-aware).
pub fn estimate_tokens(text: &str) -> usize {
    if text.is_empty() {
        return 0;
    }
    let cjk_count = text.chars().filter(|c| is_cjk_char(*c)).count();
    let non_cjk_count = text.chars().count() - cjk_count;
    cjk_count + (non_cjk_count + 3) / 4 // round up division
}

fn is_cjk_char(ch: char) -> bool {
    let cp = ch as u32;
    // CJK Unified Ideographs
    (0x4E00..=0x9FFF).contains(&cp)
    // CJK Extension A
    || (0x3400..=0x4DBF).contains(&cp)
    // Hiragana
    || (0x3040..=0x309F).contains(&cp)
    // Katakana
    || (0x30A0..=0x30FF).contains(&cp)
    // Katakana Phonetic Extensions
    || (0x31F0..=0x31FF).contains(&cp)
    // CJK Symbols and Punctuation
    || (0x3000..=0x303F).contains(&cp)
    // Fullwidth Forms
    || (0xFF00..=0xFFEF).contains(&cp)
    // Korean Hangul Syllables
    || (0xAC00..=0xD7AF).contains(&cp)
    // Korean Hangul Jamo
    || (0x1100..=0x11FF).contains(&cp)
}

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tool {
    Read,
    Edit,
    Write,
    Bash,
    Shell, // alias for Bash (backward compat)
    Grep,
    Glob,
}

#[derive(Debug, Clone)]
pub enum ToolInput {
    Read {
        path: PathBuf,
        offset: Option<usize>,
        limit: Option<usize>,
    },
    Edit {
        path: PathBuf,
        old_string: String,
        new_string: String,
    },
    Write {
        path: PathBuf,
        content: String,
    },
    #[allow(dead_code)]
    Shell {
        command: String,
        args: Vec<String>,
    },
    Bash {
        command: String,
        timeout: Option<u64>,
    },
    Grep {
        pattern: String,
        paths: Vec<PathBuf>,
    },
    Glob {
        pattern: String,
        root: Option<PathBuf>,
    },
    ListFiles {
        path: PathBuf,
    },
}

#[derive(Debug)]
pub enum ToolResult {
    Text(String),
    Lines(Vec<String>),
    Paths(Vec<PathBuf>),
    #[allow(dead_code)]
    Status(i32),
    PreviewWrite {
        path: PathBuf,
        diff: String,
        content: String,
    },
}

impl ToolResult {
    pub fn to_string_lossy(&self) -> String {
        match self {
            ToolResult::Text(text) => text.clone(),
            ToolResult::Lines(lines) => lines.join("\n"),
            ToolResult::Paths(paths) => paths
                .iter()
                .map(|p| p.display().to_string())
                .collect::<Vec<_>>()
                .join("\n"),
            ToolResult::Status(code) => format!("exit code: {}", code),
            ToolResult::PreviewWrite { diff, .. } => diff.clone(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ToolPolicy {
    permissions: Option<PermissionsConfig>,
    sandbox: Option<SandboxConfig>,
    workspace_root: PathBuf,
    approval_override: Arc<Mutex<ApprovalOverride>>,
}

impl Default for ToolPolicy {
    fn default() -> Self {
        let workspace_root = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        Self {
            permissions: None,
            sandbox: None,
            workspace_root,
            approval_override: Arc::new(Mutex::new(ApprovalOverride::None)),
        }
    }
}

impl ToolPolicy {
    pub fn from_config(config: &Config) -> Self {
        let workspace_root = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        Self {
            permissions: config.permissions.clone(),
            sandbox: config.sandbox.clone(),
            workspace_root,
            approval_override: Arc::new(Mutex::new(ApprovalOverride::None)),
        }
    }

    #[allow(dead_code)]
    pub fn set_approval_override(&self, override_state: ApprovalOverride) {
        if let Ok(mut guard) = self.approval_override.lock() {
            *guard = override_state;
        }
    }

    fn check(&self, input: &ToolInput) -> Result<()> {
        self.check_permissions(input)?;
        self.check_sandbox(input)?;
        Ok(())
    }

    fn check_permissions(&self, input: &ToolInput) -> Result<()> {
        let Some(permissions) = &self.permissions else {
            return Ok(());
        };

        if let Some(policy) = permissions.approval_policy.as_deref() {
            let policy = policy.trim().to_ascii_lowercase();
            match policy.as_str() {
                "always" => {
                    if let Ok(mut guard) = self.approval_override.lock() {
                        match &*guard {
                            ApprovalOverride::AllowAll => {}
                            ApprovalOverride::DenyAll => {
                                return Err(anyhow!(
                                    "permission denied by approval override for tool: {}",
                                    tool_name(input)
                                ));
                            }
                            ApprovalOverride::AllowOnce(tool) => {
                                if *tool == tool_kind(input) {
                                    *guard = ApprovalOverride::None;
                                } else {
                                    return Err(ToolApprovalRequired::new(input).into());
                                }
                            }
                            ApprovalOverride::None => {
                                return Err(ToolApprovalRequired::new(input).into());
                            }
                        }
                    } else {
                        return Err(ToolApprovalRequired::new(input).into());
                    }
                }
                "read-only" => {
                    if matches!(
                        input,
                        ToolInput::Write { .. }
                            | ToolInput::Edit { .. }
                            | ToolInput::Shell { .. }
                            | ToolInput::Bash { .. }
                    ) {
                        return Err(anyhow!(
                            "permission denied by approval_policy=read-only for tool: {}",
                            tool_name(input)
                        ));
                    }
                }
                _ => {}
            }
        }

        if let Some(deny) = &permissions.deny {
            for rule in deny {
                if rule_matches_tool(rule, input, Some(&self.workspace_root)) {
                    return Err(anyhow!("permission denied by rule: {}", rule));
                }
            }
        }

        if let Some(allowed) = &permissions.allowed_tools {
            if !allowed
                .iter()
                .any(|rule| rule_matches_tool(rule, input, Some(&self.workspace_root)))
            {
                return Err(anyhow!("tool not allowed: {}", tool_name(input)));
            }
        }

        Ok(())
    }

    fn check_sandbox(&self, input: &ToolInput) -> Result<()> {
        let Some(sandbox) = &self.sandbox else {
            return Ok(());
        };

        let mode = sandbox
            .mode
            .as_deref()
            .unwrap_or("none")
            .trim()
            .to_ascii_lowercase();

        if matches!(mode.as_str(), "read-only")
            && matches!(
                input,
                ToolInput::Write { .. }
                    | ToolInput::Edit { .. }
                    | ToolInput::Shell { .. }
                    | ToolInput::Bash { .. }
            )
        {
            return Err(anyhow!(
                "sandbox denies write in read-only mode: {}",
                tool_name(input)
            ));
        }

        if matches!(mode.as_str(), "workspace-write") {
            if matches!(input, ToolInput::Shell { .. } | ToolInput::Bash { .. }) {
                return Err(anyhow!("sandbox denies shell in workspace-write mode"));
            }
            if matches!(input, ToolInput::Write { .. } | ToolInput::Edit { .. }) {
                let paths = tool_paths(input);
                for path in paths {
                    self.enforce_path_limits(&path, sandbox, true)?;
                }
                return Ok(());
            }
        }

        for path in tool_paths(input) {
            self.enforce_path_limits(&path, sandbox, false)?;
        }

        Ok(())
    }

    fn enforce_path_limits(
        &self,
        path: &Path,
        sandbox: &SandboxConfig,
        require_within_workspace: bool,
    ) -> Result<()> {
        let resolved = resolve_path(&self.workspace_root, path);
        let resolved_str = resolved.to_string_lossy();
        let rel_str = resolved
            .strip_prefix(&self.workspace_root)
            .ok()
            .map(|p| PathBuf::from(".").join(p).to_string_lossy().to_string());

        if let Some(blocked) = &sandbox.blocked_paths {
            if path_matches_any(&resolved_str, rel_str.as_deref(), blocked) {
                return Err(anyhow!("sandbox blocked path: {}", resolved_str));
            }
        }

        if let Some(allowed) = &sandbox.allowed_paths {
            if !path_matches_any(&resolved_str, rel_str.as_deref(), allowed) {
                return Err(anyhow!("sandbox path not allowed: {}", resolved_str));
            }
        } else if require_within_workspace && !resolved.starts_with(&self.workspace_root) {
            return Err(anyhow!(
                "sandbox denies write outside workspace: {}",
                resolved_str
            ));
        }

        Ok(())
    }
}

pub struct ToolExecutor {
    policy: ToolPolicy,
}

impl ToolExecutor {
    pub fn new() -> Self {
        Self {
            policy: ToolPolicy::default(),
        }
    }

    pub fn with_policy(policy: ToolPolicy) -> Self {
        Self { policy }
    }

    pub fn preview_write(&self, path: PathBuf, content: String) -> Result<ToolResult> {
        self.policy.check(&ToolInput::Write {
            path: path.clone(),
            content: content.clone(),
        })?;
        let before = if path.exists() {
            fs::read_to_string(&path)?
        } else {
            String::new()
        };
        let diff = build_diff(&path, &before, &content);
        Ok(ToolResult::PreviewWrite {
            path,
            diff,
            content,
        })
    }

    pub fn execute(&self, input: ToolInput) -> Result<ToolResult> {
        self.policy.check(&input)?;
        match input {
            ToolInput::Read { path, offset, limit } => {
                // Check for image files
                if is_image_path(&path) {
                    let meta = fs::metadata(&path)
                        .map_err(|e| anyhow!("failed to read {}: {}", path.display(), e))?;
                    if meta.len() == 0 {
                        return Err(anyhow!("image file is empty: {}", path.display()));
                    }
                    return Ok(ToolResult::Text(format!(
                        "[Image file: {} ({} bytes)]",
                        path.display(),
                        meta.len()
                    )));
                }
                // Check for binary files
                let raw = fs::read(&path)
                    .map_err(|e| anyhow!("failed to read {}: {}", path.display(), e))?;
                if is_binary_content(&raw) {
                    return Ok(ToolResult::Text(format!(
                        "[Binary file: {} ({} bytes)]",
                        path.display(),
                        raw.len()
                    )));
                }
                let content = String::from_utf8_lossy(&raw).to_string();
                let lines: Vec<&str> = content.lines().collect();
                let total = lines.len();
                let start = offset.unwrap_or(0).min(total);
                let end = limit
                    .map(|l| (start + l).min(total))
                    .unwrap_or(total);
                let numbered: Vec<String> = lines[start..end]
                    .iter()
                    .enumerate()
                    .map(|(i, line)| {
                        let truncated = if line.len() > READ_MAX_LINE_CHARS {
                            format!("{}... (truncated)", &line[..READ_MAX_LINE_CHARS])
                        } else {
                            line.to_string()
                        };
                        format!("{:>6}\t{}", start + i + 1, truncated)
                    })
                    .collect();
                let mut result = numbered.join("\n");
                if offset.is_some() || limit.is_some() {
                    result = format!(
                        "(showing lines {}-{} of {})\n{}",
                        start + 1,
                        end,
                        total,
                        result
                    );
                }
                Ok(ToolResult::Text(result))
            }
            ToolInput::Edit {
                path,
                old_string,
                new_string,
            } => {
                let content = fs::read_to_string(&path)
                    .map_err(|e| anyhow!("failed to read {}: {}", path.display(), e))?;
                let count = content.matches(&old_string).count();
                if count == 0 {
                    return Err(anyhow!(
                        "old_string not found in {}",
                        path.display()
                    ));
                }
                if count > 1 {
                    return Err(anyhow!(
                        "old_string found {} times in {} (must be unique)",
                        count,
                        path.display()
                    ));
                }
                let new_content = content.replacen(&old_string, &new_string, 1);
                fs::write(&path, &new_content)?;
                Ok(ToolResult::Text(format!(
                    "Successfully edited {}",
                    path.display()
                )))
            }
            ToolInput::Write { path, content } => {
                if content.len() > WRITE_MAX_BYTES {
                    return Err(anyhow!(
                        "content too large ({} bytes, max {} bytes)",
                        content.len(),
                        WRITE_MAX_BYTES
                    ));
                }
                if let Some(parent) = path.parent() {
                    if !parent.exists() {
                        fs::create_dir_all(parent)?;
                    }
                }
                let line_count = content.lines().count();
                fs::write(&path, &content)?;
                Ok(ToolResult::Text(format!(
                    "Successfully wrote {} ({} bytes, {} lines)",
                    path.display(),
                    content.len(),
                    line_count
                )))
            }
            ToolInput::Shell { command, args } => {
                // Legacy: delegate to Bash
                let full_cmd = if args.is_empty() {
                    command
                } else {
                    format!("{} {}", command, args.join(" "))
                };
                self.execute(ToolInput::Bash {
                    command: full_cmd,
                    timeout: None,
                })
            }
            ToolInput::Bash { command, timeout } => {
                if command.trim().is_empty() {
                    return Err(anyhow!("no command provided"));
                }
                if is_dangerous_command(&command) {
                    return Err(anyhow!(
                        "dangerous command blocked: {}",
                        command
                    ));
                }
                let timeout_secs = timeout.unwrap_or(BASH_TIMEOUT_SECS);
                let clean_env = build_clean_env();
                let mut child = Command::new("sh")
                    .arg("-c")
                    .arg(&command)
                    .current_dir(&self.policy.workspace_root)
                    .env_clear()
                    .envs(clean_env)
                    .stdout(std::process::Stdio::piped())
                    .stderr(std::process::Stdio::piped())
                    .spawn()
                    .map_err(|e| anyhow!("failed to spawn command: {}", e))?;

                // Apply timeout
                let deadline =
                    std::time::Instant::now() + std::time::Duration::from_secs(timeout_secs);
                loop {
                    match child.try_wait() {
                        Ok(Some(_status)) => break,
                        Ok(None) => {
                            if std::time::Instant::now() >= deadline {
                                let _ = child.kill();
                                return Ok(ToolResult::Text(format!(
                                    "command timed out after {}s",
                                    timeout_secs
                                )));
                            }
                            std::thread::sleep(std::time::Duration::from_millis(50));
                        }
                        Err(e) => return Err(anyhow!("error waiting for command: {}", e)),
                    }
                }

                let output = child
                    .wait_with_output()
                    .map_err(|e| anyhow!("command failed: {}", e))?;
                let stdout = String::from_utf8_lossy(&output.stdout);
                let stderr = String::from_utf8_lossy(&output.stderr);
                let exit_code = output.status.code().unwrap_or(-1);
                let mut result = String::new();
                if !stdout.is_empty() {
                    result.push_str(&stdout);
                }
                if !stderr.is_empty() {
                    if !result.is_empty() {
                        result.push('\n');
                    }
                    result.push_str("stderr:\n");
                    result.push_str(&stderr);
                }
                if result.is_empty() {
                    result = format!("(exit code: {})", exit_code);
                } else if exit_code != 0 {
                    result.push_str(&format!("\n(exit code: {})", exit_code));
                }
                // Truncate output if too large
                if result.len() > BASH_MAX_OUTPUT_BYTES {
                    let truncated = &result[..BASH_MAX_OUTPUT_BYTES];
                    result = format!(
                        "{}\n... (output truncated, {} bytes total)",
                        truncated,
                        result.len()
                    );
                }
                Ok(ToolResult::Text(result))
            }
            ToolInput::Grep { pattern, paths } => {
                let re = regex::Regex::new(&pattern)
                    .map_err(|e| anyhow!("invalid regex '{}': {}", pattern, e))?;
                let mut matches = Vec::new();
                let search_paths = if paths.is_empty() {
                    vec![PathBuf::from(".")]
                } else {
                    paths
                };
                for path in search_paths {
                    collect_grep_matches_regex(&re, &path, &mut matches)?;
                }
                Ok(ToolResult::Lines(matches))
            }
            ToolInput::Glob { pattern, root } => {
                let root = root.unwrap_or_else(|| PathBuf::from("."));
                let mut matches = Vec::new();
                collect_glob_matches(&root, &pattern, &mut matches)?;
                Ok(ToolResult::Paths(matches))
            }
            ToolInput::ListFiles { path } => {
                if !path.is_dir() {
                    return Err(anyhow!("{} is not a directory", path.display()));
                }
                let mut entries = Vec::new();
                for entry in fs::read_dir(&path)? {
                    let entry = entry?;
                    let meta = entry.metadata()?;
                    let name = entry.file_name().to_string_lossy().to_string();
                    let kind = if meta.is_dir() { "dir" } else { "file" };
                    let size = if meta.is_file() { meta.len() } else { 0 };
                    entries.push(format!("{}\t{}\t{}", kind, size, name));
                }
                entries.sort();
                Ok(ToolResult::Text(entries.join("\n")))
            }
        }
    }

    /// Execute a tool from JSON input (used by the agentic loop).
    /// Returns (result_text, is_error).
    pub fn execute_from_json(&self, tool_name: &str, input: &Value) -> (String, bool) {
        let result = match tool_name {
            "Read" => {
                let path = input
                    .get("path")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                let offset = input
                    .get("offset")
                    .and_then(Value::as_u64)
                    .map(|v| v as usize);
                let limit = input
                    .get("limit")
                    .and_then(Value::as_u64)
                    .map(|v| v as usize);
                self.execute(ToolInput::Read {
                    path: PathBuf::from(path),
                    offset,
                    limit,
                })
            }
            "Edit" => {
                let path = input
                    .get("path")
                    .or_else(|| input.get("file_path"))
                    .and_then(Value::as_str)
                    .unwrap_or("");
                let old_string = input
                    .get("old_string")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                let new_string = input
                    .get("new_string")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                self.execute(ToolInput::Edit {
                    path: PathBuf::from(path),
                    old_string: old_string.to_string(),
                    new_string: new_string.to_string(),
                })
            }
            "Write" => {
                let path = input
                    .get("path")
                    .or_else(|| input.get("file_path"))
                    .and_then(Value::as_str)
                    .unwrap_or("");
                let content = input
                    .get("content")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                self.execute(ToolInput::Write {
                    path: PathBuf::from(path),
                    content: content.to_string(),
                })
            }
            "Bash" | "Shell" => {
                let command = input
                    .get("command")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                let timeout = input
                    .get("timeout")
                    .and_then(Value::as_u64);
                self.execute(ToolInput::Bash {
                    command: command.to_string(),
                    timeout,
                })
            }
            "Grep" => {
                let pattern = input
                    .get("pattern")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                let paths: Vec<PathBuf> = input
                    .get("paths")
                    .or_else(|| input.get("path"))
                    .map(|v| match v {
                        Value::Array(arr) => arr
                            .iter()
                            .filter_map(Value::as_str)
                            .map(PathBuf::from)
                            .collect(),
                        Value::String(s) => vec![PathBuf::from(s)],
                        _ => vec![],
                    })
                    .unwrap_or_default();
                self.execute(ToolInput::Grep {
                    pattern: pattern.to_string(),
                    paths,
                })
            }
            "Glob" => {
                let pattern = input
                    .get("pattern")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                let root = input
                    .get("root")
                    .or_else(|| input.get("path"))
                    .and_then(Value::as_str)
                    .map(PathBuf::from);
                self.execute(ToolInput::Glob {
                    pattern: pattern.to_string(),
                    root,
                })
            }
            "ListFiles" => {
                let path = input
                    .get("path")
                    .and_then(Value::as_str)
                    .unwrap_or(".");
                self.execute(ToolInput::ListFiles {
                    path: PathBuf::from(path),
                })
            }
            _ => Err(anyhow!("unknown tool: {}", tool_name)),
        };

        match result {
            Ok(r) => (r.to_string_lossy(), false),
            Err(e) => (format!("Error: {}", e), true),
        }
    }
}

impl Default for ToolExecutor {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Tool definitions for LLM
// ---------------------------------------------------------------------------

pub fn builtin_tool_definitions() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            name: "Read".to_string(),
            description: "Read the contents of a file. Returns numbered lines. Use offset and limit for large files.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "The absolute or relative file path to read"
                    },
                    "offset": {
                        "type": "integer",
                        "description": "Line offset to start reading from (0-based). Optional."
                    },
                    "limit": {
                        "type": "integer",
                        "description": "Maximum number of lines to read. Optional."
                    }
                },
                "required": ["path"]
            }),
        },
        ToolDefinition {
            name: "Edit".to_string(),
            description: "Edit a file by replacing an exact string with a new string. The old_string must appear exactly once in the file.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "The file path to edit"
                    },
                    "old_string": {
                        "type": "string",
                        "description": "The exact string to find and replace (must be unique in the file)"
                    },
                    "new_string": {
                        "type": "string",
                        "description": "The replacement string"
                    }
                },
                "required": ["path", "old_string", "new_string"]
            }),
        },
        ToolDefinition {
            name: "Write".to_string(),
            description: "Write content to a file. Creates parent directories if needed. Overwrites existing content.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "The file path to write"
                    },
                    "content": {
                        "type": "string",
                        "description": "The content to write"
                    }
                },
                "required": ["path", "content"]
            }),
        },
        ToolDefinition {
            name: "Bash".to_string(),
            description: "Execute a shell command. Returns stdout, stderr, and exit code.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "command": {
                        "type": "string",
                        "description": "The shell command to execute"
                    },
                    "timeout": {
                        "type": "integer",
                        "description": "Timeout in seconds (default: 120)"
                    }
                },
                "required": ["command"]
            }),
        },
        ToolDefinition {
            name: "Grep".to_string(),
            description: "Search file contents using a regex pattern. Returns matching lines with file paths and line numbers.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "pattern": {
                        "type": "string",
                        "description": "Regex pattern to search for"
                    },
                    "path": {
                        "type": "string",
                        "description": "File or directory to search in (default: current directory)"
                    }
                },
                "required": ["pattern"]
            }),
        },
        ToolDefinition {
            name: "Glob".to_string(),
            description: "Find files matching a glob pattern. Returns matching file paths.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "pattern": {
                        "type": "string",
                        "description": "Glob pattern (e.g. '*.rs', 'src/**/*.ts')"
                    },
                    "root": {
                        "type": "string",
                        "description": "Root directory to search from (default: current directory)"
                    }
                },
                "required": ["pattern"]
            }),
        },
        ToolDefinition {
            name: "ListFiles".to_string(),
            description: "List files and directories in a directory. Returns type, size, and name for each entry.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Directory path to list (default: current directory)"
                    }
                },
                "required": ["path"]
            }),
        },
    ]
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn tool_name(input: &ToolInput) -> &'static str {
    match input {
        ToolInput::Read { .. } => "Read",
        ToolInput::Edit { .. } => "Edit",
        ToolInput::Write { .. } => "Write",
        ToolInput::Shell { .. } => "Shell",
        ToolInput::Bash { .. } => "Bash",
        ToolInput::Grep { .. } => "Grep",
        ToolInput::Glob { .. } => "Glob",
        ToolInput::ListFiles { .. } => "ListFiles",
    }
}

fn tool_kind(input: &ToolInput) -> Tool {
    match input {
        ToolInput::Read { .. } => Tool::Read,
        ToolInput::Edit { .. } => Tool::Edit,
        ToolInput::Write { .. } => Tool::Write,
        ToolInput::Shell { .. } | ToolInput::Bash { .. } => Tool::Bash,
        ToolInput::Grep { .. } => Tool::Grep,
        ToolInput::Glob { .. } | ToolInput::ListFiles { .. } => Tool::Glob,
    }
}

fn tool_paths(input: &ToolInput) -> Vec<PathBuf> {
    match input {
        ToolInput::Read { path, .. } => vec![path.clone()],
        ToolInput::Edit { path, .. } => vec![path.clone()],
        ToolInput::Write { path, .. } => vec![path.clone()],
        ToolInput::Grep { paths, .. } => paths.clone(),
        ToolInput::Glob { root, .. } => root.clone().map(|p| vec![p]).unwrap_or_default(),
        ToolInput::ListFiles { path } => vec![path.clone()],
        ToolInput::Shell { .. } | ToolInput::Bash { .. } => Vec::new(),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolApprovalDecision {
    AllowOnce,
    DenyOnce,
    AllowAll,
    DenyAll,
}

#[derive(Debug, Clone)]
pub struct ToolApprovalRequest {
    pub tool: Tool,
    pub paths: Vec<PathBuf>,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct ToolApprovalRequired {
    pub tool: Tool,
    pub paths: Vec<PathBuf>,
}

impl ToolApprovalRequired {
    fn new(input: &ToolInput) -> Self {
        Self {
            tool: tool_kind(input),
            paths: tool_paths(input),
        }
    }
}

impl std::fmt::Display for ToolApprovalRequired {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "permission approval required for tool: {:?}", self.tool)
    }
}

impl std::error::Error for ToolApprovalRequired {}

#[derive(Debug, Clone, Copy)]
#[allow(dead_code)]
pub enum ApprovalOverride {
    None,
    AllowOnce(Tool),
    AllowAll,
    DenyAll,
}

fn resolve_path(root: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path)
    }
}

fn path_matches_any(path: &str, rel_path: Option<&str>, rules: &[String]) -> bool {
    for rule in rules {
        let rule = rule.trim();
        if wildcard_match(rule, path) {
            return true;
        }
        if let Some(rel) = rel_path {
            if wildcard_match(rule, rel) {
                return true;
            }
        }
    }
    false
}

fn rule_matches_tool(rule: &str, input: &ToolInput, root: Option<&Path>) -> bool {
    let rule = rule.trim();
    if rule.is_empty() {
        return false;
    }
    let (name, pattern) = if let Some(start) = rule.find('(') {
        if rule.ends_with(')') {
            let name = rule[..start].trim();
            let inner = rule[start + 1..rule.len() - 1].trim();
            (name, Some(inner))
        } else {
            (rule, None)
        }
    } else {
        (rule, None)
    };

    let tool = tool_name(input);
    let name_lower = name.to_ascii_lowercase();
    let tool_lower = tool.to_ascii_lowercase();
    let matches_name = name_lower == tool_lower
        || (name_lower == "bash" && tool_lower == "shell")
        || (name_lower == "shell" && tool_lower == "bash");
    if !matches_name {
        return false;
    }

    let Some(pattern) = pattern else {
        return true;
    };

    let targets = tool_match_targets(input, root);
    targets.iter().any(|target| wildcard_match(pattern, target))
}

fn tool_match_targets(input: &ToolInput, root: Option<&Path>) -> Vec<String> {
    match input {
        ToolInput::Read { path, .. }
        | ToolInput::Edit { path, .. }
        | ToolInput::Write { path, .. }
        | ToolInput::ListFiles { path } => {
            let abs = root
                .map(|r| resolve_path(r, path))
                .unwrap_or_else(|| path.clone());
            vec![
                path.to_string_lossy().to_string(),
                abs.to_string_lossy().to_string(),
            ]
        }
        ToolInput::Shell { command, args } => {
            let mut cmd = command.clone();
            if !args.is_empty() {
                cmd.push(' ');
                cmd.push_str(&args.join(" "));
            }
            vec![cmd]
        }
        ToolInput::Bash { command, .. } => {
            vec![command.clone()]
        }
        ToolInput::Grep { pattern, paths } => {
            let mut out = Vec::new();
            out.push(pattern.clone());
            for path in paths {
                out.push(path.to_string_lossy().to_string());
                if let Some(root) = root {
                    out.push(resolve_path(root, path).to_string_lossy().to_string());
                }
            }
            out
        }
        ToolInput::Glob { pattern, root } => {
            let mut out = vec![pattern.clone()];
            if let Some(root) = root {
                out.push(root.to_string_lossy().to_string());
            }
            out
        }
    }
}

fn collect_grep_matches_regex(
    re: &regex::Regex,
    path: &Path,
    out: &mut Vec<String>,
) -> Result<()> {
    collect_grep_matches_regex_inner(re, path, out, true)
}

fn collect_grep_matches_regex_inner(
    re: &regex::Regex,
    path: &Path,
    out: &mut Vec<String>,
    is_root: bool,
) -> Result<()> {
    if path.is_dir() {
        // Skip hidden dirs and common non-text dirs, but not the initial root
        if !is_root {
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                if name.starts_with('.') || name == "node_modules" || name == "target" {
                    return Ok(());
                }
            }
        }
        for entry in fs::read_dir(path)? {
            let entry = entry?;
            collect_grep_matches_regex_inner(re, &entry.path(), out, false)?;
        }
        return Ok(());
    }

    if !path.is_file() {
        return Ok(());
    }

    let content = match fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return Ok(()), // Skip binary/unreadable files
    };

    for (idx, line) in content.lines().enumerate() {
        if re.is_match(line) {
            out.push(format!(
                "{}:{}:{}",
                path.display(),
                idx + 1,
                line.trim_end()
            ));
        }
    }
    Ok(())
}

fn collect_glob_matches(root: &Path, pattern: &str, out: &mut Vec<PathBuf>) -> Result<()> {
    collect_glob_matches_inner(root, pattern, out, true)
}

fn collect_glob_matches_inner(
    root: &Path,
    pattern: &str,
    out: &mut Vec<PathBuf>,
    is_root: bool,
) -> Result<()> {
    if root.is_dir() {
        // Skip hidden/excluded directories, but never skip the initial root
        if !is_root {
            if let Some(name) = root.file_name().and_then(|n| n.to_str()) {
                if name.starts_with('.') || name == "node_modules" || name == "target" {
                    return Ok(());
                }
            }
        }
        for entry in fs::read_dir(root)? {
            let entry = entry?;
            collect_glob_matches_inner(&entry.path(), pattern, out, false)?;
        }
        return Ok(());
    }

    if root.is_file() && matches_glob(pattern, root) {
        out.push(root.to_path_buf());
    }
    Ok(())
}

fn matches_glob(pattern: &str, path: &Path) -> bool {
    let target = path.to_string_lossy();
    // Check just the filename for simple patterns
    if !pattern.contains('/') {
        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            if wildcard_match(pattern, name) {
                return true;
            }
        }
    }
    wildcard_match(pattern, &target)
}

pub fn build_diff(path: &Path, before: &str, after: &str) -> String {
    let mut out = String::new();
    out.push_str("--- ");
    out.push_str(&path.to_string_lossy());
    out.push('\n');
    out.push_str("+++ ");
    out.push_str(&path.to_string_lossy());
    out.push('\n');
    out.push_str("@@\n");

    let before_lines: Vec<&str> = before.lines().collect();
    let after_lines: Vec<&str> = after.lines().collect();
    let max_len = before_lines.len().max(after_lines.len());

    for idx in 0..max_len {
        let before_line = before_lines.get(idx).copied().unwrap_or("");
        let after_line = after_lines.get(idx).copied().unwrap_or("");
        if before_line == after_line {
            if !before_line.is_empty() || !after_line.is_empty() {
                out.push(' ');
                out.push_str(before_line);
                out.push('\n');
            }
            continue;
        }
        if !before_line.is_empty() {
            out.push('-');
            out.push_str(before_line);
            out.push('\n');
        }
        if !after_line.is_empty() {
            out.push('+');
            out.push_str(after_line);
            out.push('\n');
        }
    }

    out
}

fn wildcard_match(pattern: &str, text: &str) -> bool {
    let (mut p_idx, mut t_idx) = (0usize, 0usize);
    let (mut star_idx, mut match_idx) = (None, 0usize);
    let p: Vec<char> = pattern.chars().collect();
    let t: Vec<char> = text.chars().collect();

    while t_idx < t.len() {
        if p_idx < p.len() && (p[p_idx] == '?' || p[p_idx] == t[t_idx]) {
            p_idx += 1;
            t_idx += 1;
        } else if p_idx < p.len() && p[p_idx] == '*' {
            star_idx = Some(p_idx);
            p_idx += 1;
            match_idx = t_idx;
        } else if let Some(si) = star_idx {
            p_idx = si + 1;
            match_idx += 1;
            t_idx = match_idx;
        } else {
            return false;
        }
    }

    while p_idx < p.len() && p[p_idx] == '*' {
        p_idx += 1;
    }

    p_idx == p.len()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn make_executor(dir: &Path) -> ToolExecutor {
        let policy = ToolPolicy {
            workspace_root: dir.to_path_buf(),
            ..ToolPolicy::default()
        };
        ToolExecutor::with_policy(policy)
    }

    // ═══════════════════════════════════════════════════════════════════════
    // Read Tool Tests
    // ═══════════════════════════════════════════════════════════════════════

    #[test]
    fn read_returns_numbered_lines() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.txt");
        fs::write(&path, "line one\nline two\nline three\n").unwrap();
        let exec = make_executor(dir.path());
        let result = exec
            .execute(ToolInput::Read {
                path: path.clone(),
                offset: None,
                limit: None,
            })
            .unwrap();
        let text = result.to_string_lossy();
        assert!(text.contains("1\tline one"));
        assert!(text.contains("2\tline two"));
        assert!(text.contains("3\tline three"));
    }

    #[test]
    fn read_with_offset_and_limit() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.txt");
        fs::write(&path, "line1\nline2\nline3\nline4\nline5\n").unwrap();
        let exec = make_executor(dir.path());
        let result = exec
            .execute(ToolInput::Read {
                path: path.clone(),
                offset: Some(1),
                limit: Some(2),
            })
            .unwrap();
        let text = result.to_string_lossy();
        assert!(text.contains("showing lines 2-3 of 5"));
        assert!(text.contains("line2"));
        assert!(text.contains("line3"));
        assert!(!text.contains("line1"));
        assert!(!text.contains("line4"));
    }

    #[test]
    fn read_nonexistent_file_returns_error() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("nonexistent.txt");
        let exec = make_executor(dir.path());
        let result = exec.execute(ToolInput::Read {
            path,
            offset: None,
            limit: None,
        });
        assert!(result.is_err());
    }

    // ═══════════════════════════════════════════════════════════════════════
    // Write Tool Tests
    // ═══════════════════════════════════════════════════════════════════════

    #[test]
    fn write_creates_file() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("new.txt");
        let exec = make_executor(dir.path());
        let result = exec
            .execute(ToolInput::Write {
                path: path.clone(),
                content: "hello world".to_string(),
            })
            .unwrap();
        let text = result.to_string_lossy();
        assert!(text.contains("Successfully wrote"));
        assert_eq!(fs::read_to_string(&path).unwrap(), "hello world");
    }

    #[test]
    fn write_creates_parent_directories() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("sub").join("dir").join("file.txt");
        let exec = make_executor(dir.path());
        exec.execute(ToolInput::Write {
            path: path.clone(),
            content: "nested".to_string(),
        })
        .unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "nested");
    }

    #[test]
    fn write_overwrites_existing_file() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("overwrite.txt");
        fs::write(&path, "old content").unwrap();
        let exec = make_executor(dir.path());
        exec.execute(ToolInput::Write {
            path: path.clone(),
            content: "new content".to_string(),
        })
        .unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "new content");
    }

    // ═══════════════════════════════════════════════════════════════════════
    // Edit Tool Tests
    // ═══════════════════════════════════════════════════════════════════════

    #[test]
    fn edit_replaces_unique_string() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("edit.txt");
        fs::write(&path, "hello world\nfoo bar\n").unwrap();
        let exec = make_executor(dir.path());
        let result = exec
            .execute(ToolInput::Edit {
                path: path.clone(),
                old_string: "foo bar".to_string(),
                new_string: "baz qux".to_string(),
            })
            .unwrap();
        let text = result.to_string_lossy();
        assert!(text.contains("Successfully edited"));
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            "hello world\nbaz qux\n"
        );
    }

    #[test]
    fn edit_fails_when_string_not_found() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("edit.txt");
        fs::write(&path, "hello world\n").unwrap();
        let exec = make_executor(dir.path());
        let result = exec.execute(ToolInput::Edit {
            path: path.clone(),
            old_string: "nonexistent".to_string(),
            new_string: "replacement".to_string(),
        });
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("not found"));
    }

    #[test]
    fn edit_fails_when_string_not_unique() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("edit.txt");
        fs::write(&path, "hello\nhello\nhello\n").unwrap();
        let exec = make_executor(dir.path());
        let result = exec.execute(ToolInput::Edit {
            path: path.clone(),
            old_string: "hello".to_string(),
            new_string: "world".to_string(),
        });
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("3 times"));
    }

    #[test]
    fn edit_preserves_surrounding_content() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("edit.txt");
        fs::write(&path, "before\ntarget_line\nafter\n").unwrap();
        let exec = make_executor(dir.path());
        exec.execute(ToolInput::Edit {
            path: path.clone(),
            old_string: "target_line".to_string(),
            new_string: "replaced_line".to_string(),
        })
        .unwrap();
        let content = fs::read_to_string(&path).unwrap();
        assert!(content.contains("before\n"));
        assert!(content.contains("replaced_line\n"));
        assert!(content.contains("after\n"));
    }

    // ═══════════════════════════════════════════════════════════════════════
    // Bash Tool Tests
    // ═══════════════════════════════════════════════════════════════════════

    #[test]
    fn bash_simple_echo() {
        let dir = TempDir::new().unwrap();
        let exec = make_executor(dir.path());
        let result = exec
            .execute(ToolInput::Bash {
                command: "echo hello_world".to_string(),
                timeout: None,
            })
            .unwrap();
        let text = result.to_string_lossy();
        assert!(text.contains("hello_world"));
    }

    #[test]
    fn bash_captures_stderr() {
        let dir = TempDir::new().unwrap();
        let exec = make_executor(dir.path());
        let result = exec
            .execute(ToolInput::Bash {
                command: "echo err_msg >&2".to_string(),
                timeout: None,
            })
            .unwrap();
        let text = result.to_string_lossy();
        assert!(text.contains("err_msg"));
    }

    #[test]
    fn bash_returns_exit_code() {
        let dir = TempDir::new().unwrap();
        let exec = make_executor(dir.path());
        let result = exec
            .execute(ToolInput::Bash {
                command: "exit 42".to_string(),
                timeout: None,
            })
            .unwrap();
        let text = result.to_string_lossy();
        assert!(text.contains("exit code"));
        assert!(text.contains("42"));
    }

    #[test]
    fn bash_combined_stdout_stderr() {
        let dir = TempDir::new().unwrap();
        let exec = make_executor(dir.path());
        let result = exec
            .execute(ToolInput::Bash {
                command: "echo OUT && echo ERR >&2".to_string(),
                timeout: None,
            })
            .unwrap();
        let text = result.to_string_lossy();
        assert!(text.contains("OUT"));
        assert!(text.contains("ERR"));
    }

    #[test]
    fn bash_timeout_kills_slow_command() {
        let dir = TempDir::new().unwrap();
        let exec = make_executor(dir.path());
        let result = exec
            .execute(ToolInput::Bash {
                command: "sleep 30".to_string(),
                timeout: Some(1),
            })
            .unwrap();
        let text = result.to_string_lossy();
        assert!(text.contains("timed out"));
    }

    #[test]
    fn bash_runs_in_workspace_root() {
        let dir = TempDir::new().unwrap();
        let exec = make_executor(dir.path());
        let result = exec
            .execute(ToolInput::Bash {
                command: "pwd".to_string(),
                timeout: None,
            })
            .unwrap();
        let text = result.to_string_lossy();
        // The output should contain the temp dir path
        let dir_str = dir.path().to_string_lossy();
        assert!(
            text.contains(&*dir_str),
            "pwd output '{}' should contain '{}'",
            text,
            dir_str
        );
    }

    // ═══════════════════════════════════════════════════════════════════════
    // Grep Tool Tests
    // ═══════════════════════════════════════════════════════════════════════

    #[test]
    fn grep_finds_matching_lines() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("search.txt");
        fs::write(&path, "alpha beta\ngamma delta\nalpha omega\n").unwrap();
        let exec = make_executor(dir.path());
        let result = exec
            .execute(ToolInput::Grep {
                pattern: "alpha".to_string(),
                paths: vec![path],
            })
            .unwrap();
        let text = result.to_string_lossy();
        assert!(text.contains("alpha beta"));
        assert!(text.contains("alpha omega"));
        assert!(!text.contains("gamma delta"));
    }

    #[test]
    fn grep_supports_regex() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("regex.txt");
        fs::write(&path, "foo123bar\nbaz456qux\nfoo789xyz\n").unwrap();
        let exec = make_executor(dir.path());
        let result = exec
            .execute(ToolInput::Grep {
                pattern: r"foo\d+".to_string(),
                paths: vec![path],
            })
            .unwrap();
        let text = result.to_string_lossy();
        assert!(text.contains("foo123bar"));
        assert!(text.contains("foo789xyz"));
        assert!(!text.contains("baz456qux"));
    }

    #[test]
    fn grep_invalid_regex_returns_error() {
        let dir = TempDir::new().unwrap();
        let exec = make_executor(dir.path());
        let result = exec.execute(ToolInput::Grep {
            pattern: "[invalid".to_string(),
            paths: vec![dir.path().to_path_buf()],
        });
        assert!(result.is_err());
    }

    #[test]
    fn grep_searches_directory_recursively() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("a.txt"), "findme here\n").unwrap();
        let sub = dir.path().join("sub");
        fs::create_dir(&sub).unwrap();
        fs::write(sub.join("b.txt"), "findme nested\n").unwrap();
        let exec = make_executor(dir.path());
        let result = exec
            .execute(ToolInput::Grep {
                pattern: "findme".to_string(),
                paths: vec![dir.path().to_path_buf()],
            })
            .unwrap();
        let text = result.to_string_lossy();
        assert!(text.contains("findme here"));
        assert!(text.contains("findme nested"));
    }

    #[test]
    fn grep_skips_hidden_dirs() {
        let dir = TempDir::new().unwrap();
        let hidden = dir.path().join(".hidden");
        fs::create_dir(&hidden).unwrap();
        fs::write(hidden.join("secret.txt"), "findme secret\n").unwrap();
        fs::write(dir.path().join("visible.txt"), "findme visible\n").unwrap();
        let exec = make_executor(dir.path());
        let result = exec
            .execute(ToolInput::Grep {
                pattern: "findme".to_string(),
                paths: vec![dir.path().to_path_buf()],
            })
            .unwrap();
        let text = result.to_string_lossy();
        assert!(text.contains("findme visible"));
        assert!(!text.contains("findme secret"));
    }

    // ═══════════════════════════════════════════════════════════════════════
    // Glob Tool Tests
    // ═══════════════════════════════════════════════════════════════════════

    #[test]
    fn glob_finds_matching_files() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("a.rs"), "").unwrap();
        fs::write(dir.path().join("b.rs"), "").unwrap();
        fs::write(dir.path().join("c.txt"), "").unwrap();
        let exec = make_executor(dir.path());
        let result = exec
            .execute(ToolInput::Glob {
                pattern: "*.rs".to_string(),
                root: Some(dir.path().to_path_buf()),
            })
            .unwrap();
        // Glob returns Paths with full paths
        match &result {
            ToolResult::Paths(paths) => {
                let names: Vec<String> = paths
                    .iter()
                    .filter_map(|p| p.file_name())
                    .map(|n| n.to_string_lossy().to_string())
                    .collect();
                assert!(names.contains(&"a.rs".to_string()), "Missing a.rs in {:?}", names);
                assert!(names.contains(&"b.rs".to_string()), "Missing b.rs in {:?}", names);
                assert!(!names.contains(&"c.txt".to_string()), "Should not contain c.txt");
            }
            other => panic!("Expected Paths, got {:?}", other),
        }
    }

    #[test]
    fn glob_recursive_pattern() {
        let dir = TempDir::new().unwrap();
        let sub = dir.path().join("sub");
        fs::create_dir(&sub).unwrap();
        fs::write(dir.path().join("top.py"), "").unwrap();
        fs::write(sub.join("nested.py"), "").unwrap();
        let exec = make_executor(dir.path());
        let result = exec
            .execute(ToolInput::Glob {
                pattern: "*.py".to_string(),
                root: Some(dir.path().to_path_buf()),
            })
            .unwrap();
        match &result {
            ToolResult::Paths(paths) => {
                let names: Vec<String> = paths
                    .iter()
                    .filter_map(|p| p.file_name())
                    .map(|n| n.to_string_lossy().to_string())
                    .collect();
                assert!(
                    names.contains(&"top.py".to_string())
                        || names.contains(&"nested.py".to_string()),
                    "Should find at least one .py file, got {:?}",
                    names
                );
            }
            other => panic!("Expected Paths, got {:?}", other),
        }
    }

    // ═══════════════════════════════════════════════════════════════════════
    // ListFiles Tool Tests
    // ═══════════════════════════════════════════════════════════════════════

    #[test]
    fn list_files_shows_entries() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("file.txt"), "content").unwrap();
        let sub = dir.path().join("subdir");
        fs::create_dir(&sub).unwrap();
        let exec = make_executor(dir.path());
        let result = exec
            .execute(ToolInput::ListFiles {
                path: dir.path().to_path_buf(),
            })
            .unwrap();
        let text = result.to_string_lossy();
        assert!(text.contains("file.txt"));
        assert!(text.contains("subdir"));
        assert!(text.contains("dir"));
        assert!(text.contains("file"));
    }

    #[test]
    fn list_files_error_on_nonexistent() {
        let dir = TempDir::new().unwrap();
        let exec = make_executor(dir.path());
        let result = exec.execute(ToolInput::ListFiles {
            path: dir.path().join("nonexistent"),
        });
        assert!(result.is_err());
    }

    // ═══════════════════════════════════════════════════════════════════════
    // execute_from_json Tests
    // ═══════════════════════════════════════════════════════════════════════

    #[test]
    fn execute_from_json_read() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.txt");
        fs::write(&path, "hello\n").unwrap();
        let exec = make_executor(dir.path());
        let (result, is_error) = exec.execute_from_json(
            "Read",
            &serde_json::json!({"path": path.to_str().unwrap()}),
        );
        assert!(!is_error);
        assert!(result.contains("hello"));
    }

    #[test]
    fn execute_from_json_write() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("out.txt");
        let exec = make_executor(dir.path());
        let (result, is_error) = exec.execute_from_json(
            "Write",
            &serde_json::json!({"path": path.to_str().unwrap(), "content": "written"}),
        );
        assert!(!is_error);
        assert!(result.contains("Successfully wrote"));
        assert_eq!(fs::read_to_string(&path).unwrap(), "written");
    }

    #[test]
    fn execute_from_json_edit() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("edit.txt");
        fs::write(&path, "old_content\n").unwrap();
        let exec = make_executor(dir.path());
        let (result, is_error) = exec.execute_from_json(
            "Edit",
            &serde_json::json!({
                "path": path.to_str().unwrap(),
                "old_string": "old_content",
                "new_string": "new_content"
            }),
        );
        assert!(!is_error);
        assert!(result.contains("Successfully edited"));
    }

    #[test]
    fn execute_from_json_bash() {
        let dir = TempDir::new().unwrap();
        let exec = make_executor(dir.path());
        let (result, is_error) = exec.execute_from_json(
            "Bash",
            &serde_json::json!({"command": "echo from_json"}),
        );
        assert!(!is_error);
        assert!(result.contains("from_json"));
    }

    #[test]
    fn execute_from_json_unknown_tool() {
        let dir = TempDir::new().unwrap();
        let exec = make_executor(dir.path());
        let (result, is_error) =
            exec.execute_from_json("UnknownTool", &serde_json::json!({}));
        assert!(is_error);
        assert!(result.contains("unknown tool"));
    }

    #[test]
    fn execute_from_json_list_files() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("x.txt"), "").unwrap();
        let exec = make_executor(dir.path());
        let (result, is_error) = exec.execute_from_json(
            "ListFiles",
            &serde_json::json!({"path": dir.path().to_str().unwrap()}),
        );
        assert!(!is_error);
        assert!(result.contains("x.txt"));
    }

    // ═══════════════════════════════════════════════════════════════════════
    // Tool Definitions Tests
    // ═══════════════════════════════════════════════════════════════════════

    #[test]
    fn builtin_definitions_has_all_tools() {
        let defs = builtin_tool_definitions();
        let names: Vec<&str> = defs.iter().map(|d| d.name.as_str()).collect();
        assert!(names.contains(&"Read"));
        assert!(names.contains(&"Edit"));
        assert!(names.contains(&"Write"));
        assert!(names.contains(&"Bash"));
        assert!(names.contains(&"Grep"));
        assert!(names.contains(&"Glob"));
        assert!(names.contains(&"ListFiles"));
    }

    #[test]
    fn tool_definitions_have_required_fields() {
        let defs = builtin_tool_definitions();
        for def in &defs {
            assert!(!def.name.is_empty());
            assert!(!def.description.is_empty());
            assert!(def.input_schema.is_object());
            let schema = def.input_schema.as_object().unwrap();
            assert_eq!(schema.get("type").unwrap(), "object");
            assert!(schema.contains_key("properties"));
            assert!(schema.contains_key("required"));
        }
    }

    // ═══════════════════════════════════════════════════════════════════════
    // Bash Security Tests
    // ═══════════════════════════════════════════════════════════════════════

    #[test]
    fn bash_blocks_dangerous_rm_rf() {
        let dir = TempDir::new().unwrap();
        let exec = make_executor(dir.path());
        let result = exec.execute(ToolInput::Bash {
            command: "rm -rf /".to_string(),
            timeout: None,
        });
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("dangerous"));
    }

    #[test]
    fn bash_blocks_shutdown() {
        let dir = TempDir::new().unwrap();
        let exec = make_executor(dir.path());
        let result = exec.execute(ToolInput::Bash {
            command: "shutdown -h now".to_string(),
            timeout: None,
        });
        assert!(result.is_err());
    }

    #[test]
    fn bash_allows_safe_commands() {
        let dir = TempDir::new().unwrap();
        let exec = make_executor(dir.path());
        let result = exec.execute(ToolInput::Bash {
            command: "echo safe".to_string(),
            timeout: None,
        });
        assert!(result.is_ok());
    }

    #[test]
    fn bash_empty_command_returns_error() {
        let dir = TempDir::new().unwrap();
        let exec = make_executor(dir.path());
        let result = exec.execute(ToolInput::Bash {
            command: "".to_string(),
            timeout: None,
        });
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("no command"));
    }

    #[test]
    fn bash_filters_api_keys_from_env() {
        let dir = TempDir::new().unwrap();
        let exec = make_executor(dir.path());
        // Set a sensitive env var temporarily
        std::env::set_var("ANTHROPIC_API_KEY", "sk-test-secret");
        let result = exec
            .execute(ToolInput::Bash {
                command: "echo $ANTHROPIC_API_KEY".to_string(),
                timeout: None,
            })
            .unwrap();
        let text = result.to_string_lossy();
        assert!(
            !text.contains("sk-test-secret"),
            "API key should be filtered from child env"
        );
        std::env::remove_var("ANTHROPIC_API_KEY");
    }

    #[test]
    fn bash_keeps_path_in_env() {
        let dir = TempDir::new().unwrap();
        let exec = make_executor(dir.path());
        let result = exec
            .execute(ToolInput::Bash {
                command: "echo $PATH".to_string(),
                timeout: None,
            })
            .unwrap();
        let text = result.to_string_lossy();
        // PATH should still be available
        assert!(!text.trim().is_empty(), "PATH should be present");
    }

    // ═══════════════════════════════════════════════════════════════════════
    // Read Binary/Image Detection Tests
    // ═══════════════════════════════════════════════════════════════════════

    #[test]
    fn read_detects_binary_file() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("binary.dat");
        fs::write(&path, b"\x00\x01\x02\x03binary data").unwrap();
        let exec = make_executor(dir.path());
        let result = exec
            .execute(ToolInput::Read {
                path: path.clone(),
                offset: None,
                limit: None,
            })
            .unwrap();
        let text = result.to_string_lossy();
        assert!(text.contains("[Binary file:"));
    }

    #[test]
    fn read_detects_image_extensions() {
        let dir = TempDir::new().unwrap();
        for ext in &["png", "jpg", "gif", "webp"] {
            let path = dir.path().join(format!("image.{}", ext));
            fs::write(&path, "fake image data").unwrap();
            let exec = make_executor(dir.path());
            let result = exec
                .execute(ToolInput::Read {
                    path: path.clone(),
                    offset: None,
                    limit: None,
                })
                .unwrap();
            let text = result.to_string_lossy();
            assert!(
                text.contains("[Image file:"),
                "Expected image detection for .{}, got: {}",
                ext,
                text
            );
        }
    }

    #[test]
    fn read_empty_image_returns_error() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("empty.png");
        fs::write(&path, "").unwrap();
        let exec = make_executor(dir.path());
        let result = exec.execute(ToolInput::Read {
            path,
            offset: None,
            limit: None,
        });
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("empty"));
    }

    // ═══════════════════════════════════════════════════════════════════════
    // Write Size Limit Tests
    // ═══════════════════════════════════════════════════════════════════════

    #[test]
    fn write_rejects_oversized_content() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("huge.txt");
        let content = "x".repeat(WRITE_MAX_BYTES + 1);
        let exec = make_executor(dir.path());
        let result = exec.execute(ToolInput::Write { path, content });
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("too large"));
    }

    #[test]
    fn write_accepts_max_size_content() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("max.txt");
        let content = "x".repeat(WRITE_MAX_BYTES);
        let exec = make_executor(dir.path());
        let result = exec.execute(ToolInput::Write {
            path,
            content,
        });
        assert!(result.is_ok());
    }

    // ═══════════════════════════════════════════════════════════════════════
    // Token Estimation Tests
    // ═══════════════════════════════════════════════════════════════════════

    #[test]
    fn estimate_tokens_empty() {
        assert_eq!(estimate_tokens(""), 0);
    }

    #[test]
    fn estimate_tokens_ascii() {
        // "hello world" = 11 chars, ~3 tokens
        let tokens = estimate_tokens("hello world");
        assert!(tokens >= 2 && tokens <= 5, "tokens = {}", tokens);
    }

    #[test]
    fn estimate_tokens_cjk_hiragana() {
        // 5 hiragana chars = ~5 tokens
        let tokens = estimate_tokens("こんにちは");
        assert_eq!(tokens, 5);
    }

    #[test]
    fn estimate_tokens_cjk_kanji() {
        let tokens = estimate_tokens("漢字テスト");
        assert_eq!(tokens, 5);
    }

    #[test]
    fn estimate_tokens_mixed() {
        // "Hello世界" = 5 ASCII + 2 CJK
        let tokens = estimate_tokens("Hello世界");
        // 2 CJK + ceil(5/4) = 2 + 2 = 4
        assert!(tokens >= 3 && tokens <= 5, "tokens = {}", tokens);
    }

    #[test]
    fn estimate_tokens_korean() {
        let tokens = estimate_tokens("안녕하세요");
        assert_eq!(tokens, 5);
    }

    #[test]
    fn estimate_tokens_fullwidth() {
        let tokens = estimate_tokens("ＡＢＣ");
        assert_eq!(tokens, 3);
    }

    // ═══════════════════════════════════════════════════════════════════════
    // Helper Function Tests
    // ═══════════════════════════════════════════════════════════════════════

    #[test]
    fn dangerous_command_detection() {
        assert!(is_dangerous_command("rm -rf /"));
        assert!(is_dangerous_command("sudo rm -rf /*"));
        assert!(is_dangerous_command("shutdown -h now"));
        assert!(!is_dangerous_command("echo hello"));
        assert!(!is_dangerous_command("ls -la"));
        assert!(!is_dangerous_command("rm file.txt"));
    }

    #[test]
    fn clean_env_filters_sensitive_vars() {
        std::env::set_var("OPENAI_API_KEY", "test-key");
        let env = build_clean_env();
        let has_openai = env.iter().any(|(k, _)| k == "OPENAI_API_KEY");
        assert!(!has_openai, "OPENAI_API_KEY should be filtered");
        std::env::remove_var("OPENAI_API_KEY");
    }

    #[test]
    fn clean_env_keeps_safe_vars() {
        let env = build_clean_env();
        let has_path = env.iter().any(|(k, _)| k == "PATH");
        assert!(has_path, "PATH should be kept");
    }

    #[test]
    fn binary_detection() {
        assert!(is_binary_content(b"\x00\x01\x02"));
        assert!(is_binary_content(b"text\x00more"));
        assert!(!is_binary_content(b"pure text content"));
        assert!(!is_binary_content(b""));
    }

    #[test]
    fn image_path_detection() {
        assert!(is_image_path(Path::new("photo.png")));
        assert!(is_image_path(Path::new("photo.JPG")));
        assert!(is_image_path(Path::new("photo.webp")));
        assert!(!is_image_path(Path::new("code.rs")));
        assert!(!is_image_path(Path::new("noext")));
    }

    // ═══════════════════════════════════════════════════════════════════════
    // ToolPolicy Tests
    // ═══════════════════════════════════════════════════════════════════════

    #[test]
    fn policy_default_allows_all() {
        let policy = ToolPolicy::default();
        let result = policy.check(&ToolInput::Read {
            path: PathBuf::from("test.txt"),
            offset: None,
            limit: None,
        });
        assert!(result.is_ok());
    }

    #[test]
    fn policy_read_only_blocks_write() {
        let policy = ToolPolicy {
            permissions: Some(PermissionsConfig {
                approval_policy: Some("read-only".to_string()),
                allowed_tools: None,
                deny: None,
            }),
            ..ToolPolicy::default()
        };
        let result = policy.check(&ToolInput::Write {
            path: PathBuf::from("test.txt"),
            content: "data".to_string(),
        });
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("read-only"));
    }

    #[test]
    fn policy_read_only_allows_read() {
        let policy = ToolPolicy {
            permissions: Some(PermissionsConfig {
                approval_policy: Some("read-only".to_string()),
                allowed_tools: None,
                deny: None,
            }),
            ..ToolPolicy::default()
        };
        let result = policy.check(&ToolInput::Read {
            path: PathBuf::from("test.txt"),
            offset: None,
            limit: None,
        });
        assert!(result.is_ok());
    }

    #[test]
    fn policy_deny_blocks_matching_tool() {
        let policy = ToolPolicy {
            permissions: Some(PermissionsConfig {
                approval_policy: None,
                allowed_tools: None,
                deny: Some(vec!["Bash".to_string()]),
            }),
            ..ToolPolicy::default()
        };
        let result = policy.check(&ToolInput::Bash {
            command: "echo hi".to_string(),
            timeout: None,
        });
        assert!(result.is_err());
    }

    #[test]
    fn policy_allowed_tools_restricts() {
        let policy = ToolPolicy {
            permissions: Some(PermissionsConfig {
                approval_policy: None,
                allowed_tools: Some(vec!["Read".to_string()]),
                deny: None,
            }),
            ..ToolPolicy::default()
        };
        // Read should work
        assert!(policy.check(&ToolInput::Read {
            path: PathBuf::from("test.txt"),
            offset: None,
            limit: None,
        }).is_ok());
        // Write should be blocked
        assert!(policy.check(&ToolInput::Write {
            path: PathBuf::from("test.txt"),
            content: "data".to_string(),
        }).is_err());
    }

    #[test]
    fn policy_sandbox_read_only_blocks_bash() {
        let policy = ToolPolicy {
            sandbox: Some(SandboxConfig {
                mode: Some("read-only".to_string()),
                allowed_paths: None,
                blocked_paths: None,
            }),
            ..ToolPolicy::default()
        };
        let result = policy.check(&ToolInput::Bash {
            command: "echo hi".to_string(),
            timeout: None,
        });
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("sandbox"));
    }

    #[test]
    fn policy_sandbox_blocked_paths() {
        let dir = TempDir::new().unwrap();
        let policy = ToolPolicy {
            sandbox: Some(SandboxConfig {
                mode: None,
                allowed_paths: None,
                blocked_paths: Some(vec!["/etc/*".to_string()]),
            }),
            workspace_root: dir.path().to_path_buf(),
            ..ToolPolicy::default()
        };
        let result = policy.check(&ToolInput::Read {
            path: PathBuf::from("/etc/passwd"),
            offset: None,
            limit: None,
        });
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("blocked"));
    }

    #[test]
    fn wildcard_match_basic() {
        assert!(wildcard_match("*.rs", "hello.rs"));
        assert!(!wildcard_match("*.rs", "hello.py"));
        assert!(wildcard_match("test*", "test_hello"));
        assert!(wildcard_match("*", "anything"));
        assert!(wildcard_match("a?c", "abc"));
        assert!(!wildcard_match("a?c", "abbc"));
    }

    // ═══════════════════════════════════════════════════════════════════════
    // Bash Output Truncation Tests
    // ═══════════════════════════════════════════════════════════════════════

    #[test]
    fn bash_truncates_large_output() {
        let dir = TempDir::new().unwrap();
        let exec = make_executor(dir.path());
        // Generate output larger than BASH_MAX_OUTPUT_BYTES
        let result = exec
            .execute(ToolInput::Bash {
                command: format!("python3 -c \"print('x' * {})\"", BASH_MAX_OUTPUT_BYTES + 1000),
                timeout: Some(10),
            })
            .unwrap();
        let text = result.to_string_lossy();
        assert!(
            text.contains("truncated"),
            "Large output should be truncated"
        );
    }

    #[test]
    fn bash_no_truncation_for_small_output() {
        let dir = TempDir::new().unwrap();
        let exec = make_executor(dir.path());
        let result = exec
            .execute(ToolInput::Bash {
                command: "echo small".to_string(),
                timeout: None,
            })
            .unwrap();
        let text = result.to_string_lossy();
        assert!(!text.contains("truncated"));
    }

    // ═══════════════════════════════════════════════════════════════════════
    // Read Long Line Truncation Tests
    // ═══════════════════════════════════════════════════════════════════════

    #[test]
    fn read_truncates_long_lines() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("long.txt");
        let long_line = "x".repeat(READ_MAX_LINE_CHARS + 500);
        fs::write(&path, &long_line).unwrap();
        let exec = make_executor(dir.path());
        let result = exec
            .execute(ToolInput::Read {
                path,
                offset: None,
                limit: None,
            })
            .unwrap();
        let text = result.to_string_lossy();
        assert!(text.contains("truncated"));
        assert!(text.len() < long_line.len() + 200);
    }

    // ═══════════════════════════════════════════════════════════════════════
    // Write Reports Lines
    // ═══════════════════════════════════════════════════════════════════════

    #[test]
    fn write_reports_bytes_and_lines() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("report.txt");
        let exec = make_executor(dir.path());
        let result = exec
            .execute(ToolInput::Write {
                path,
                content: "line1\nline2\nline3\n".to_string(),
            })
            .unwrap();
        let text = result.to_string_lossy();
        assert!(text.contains("bytes"));
        assert!(text.contains("lines"));
        assert!(text.contains("3 lines"));
    }

    #[test]
    fn build_diff_shows_changes() {
        let diff = build_diff(Path::new("test.txt"), "old line\n", "new line\n");
        assert!(diff.contains("-old line"));
        assert!(diff.contains("+new line"));
    }

    #[test]
    fn tool_definitions_required_fields_exist_in_properties() {
        let defs = builtin_tool_definitions();
        for def in &defs {
            let schema = def.input_schema.as_object().unwrap();
            let properties = schema.get("properties").unwrap().as_object().unwrap();
            let required = schema.get("required").unwrap().as_array().unwrap();
            for req in required {
                let field = req.as_str().unwrap();
                assert!(
                    properties.contains_key(field),
                    "Tool '{}' has required field '{}' not in properties",
                    def.name,
                    field
                );
            }
        }
    }
}
