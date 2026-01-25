// Tools module
// ビルトインツール

use anyhow::{anyhow, Result};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};

use crate::config::{Config, PermissionsConfig, SandboxConfig};

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tool {
    Read,
    Write,
    Shell,
    Grep,
    Glob,
}

#[derive(Debug, Clone)]
pub enum ToolInput {
    Read {
        path: PathBuf,
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
    Grep {
        pattern: String,
        paths: Vec<PathBuf>,
    },
    Glob {
        pattern: String,
        root: Option<PathBuf>,
    },
}

#[derive(Debug)]
pub enum ToolResult {
    Text(String),
    Lines(Vec<String>),
    Paths(Vec<PathBuf>),
    Status(i32),
    PreviewWrite {
        path: PathBuf,
        diff: String,
        content: String,
    },
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
                    if matches!(input, ToolInput::Write { .. } | ToolInput::Shell { .. }) {
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

        if matches!(mode.as_str(), "read-only") {
            if matches!(input, ToolInput::Write { .. } | ToolInput::Shell { .. }) {
                return Err(anyhow!(
                    "sandbox denies write in read-only mode: {}",
                    tool_name(input)
                ));
            }
        }

        if matches!(mode.as_str(), "workspace-write") {
            if matches!(input, ToolInput::Shell { .. }) {
                return Err(anyhow!("sandbox denies shell in workspace-write mode"));
            }
            if matches!(input, ToolInput::Write { .. }) {
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
        } else if require_within_workspace {
            if !resolved.starts_with(&self.workspace_root) {
                return Err(anyhow!(
                    "sandbox denies write outside workspace: {}",
                    resolved_str
                ));
            }
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
            ToolInput::Read { path } => {
                let content = fs::read_to_string(&path)?;
                Ok(ToolResult::Text(content))
            }
            ToolInput::Write { path, content } => {
                if let Some(parent) = path.parent() {
                    if !parent.exists() {
                        fs::create_dir_all(parent)?;
                    }
                }
                fs::write(&path, content)?;
                Ok(ToolResult::Status(0))
            }
            ToolInput::Shell { command, args } => {
                let output = Command::new(&command).args(args).output()?;
                if output.status.success() {
                    Ok(ToolResult::Text(
                        String::from_utf8_lossy(&output.stdout).to_string(),
                    ))
                } else {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    Err(anyhow!("command failed: {} ({})", command, stderr.trim()))
                }
            }
            ToolInput::Grep { pattern, paths } => {
                let mut matches = Vec::new();
                for path in paths {
                    collect_grep_matches(&pattern, &path, &mut matches)?;
                }
                Ok(ToolResult::Lines(matches))
            }
            ToolInput::Glob { pattern, root } => {
                let root = root.unwrap_or_else(|| PathBuf::from("."));
                let mut matches = Vec::new();
                collect_glob_matches(&root, &pattern, &mut matches)?;
                Ok(ToolResult::Paths(matches))
            }
        }
    }
}

impl Default for ToolExecutor {
    fn default() -> Self {
        Self::new()
    }
}

fn tool_name(input: &ToolInput) -> &'static str {
    match input {
        ToolInput::Read { .. } => "Read",
        ToolInput::Write { .. } => "Write",
        ToolInput::Shell { .. } => "Shell",
        ToolInput::Grep { .. } => "Grep",
        ToolInput::Glob { .. } => "Glob",
    }
}

fn tool_kind(input: &ToolInput) -> Tool {
    match input {
        ToolInput::Read { .. } => Tool::Read,
        ToolInput::Write { .. } => Tool::Write,
        ToolInput::Shell { .. } => Tool::Shell,
        ToolInput::Grep { .. } => Tool::Grep,
        ToolInput::Glob { .. } => Tool::Glob,
    }
}

fn tool_paths(input: &ToolInput) -> Vec<PathBuf> {
    match input {
        ToolInput::Read { path } => vec![path.clone()],
        ToolInput::Write { path, .. } => vec![path.clone()],
        ToolInput::Grep { paths, .. } => paths.clone(),
        ToolInput::Glob { root, .. } => root.clone().map(|p| vec![p]).unwrap_or_default(),
        ToolInput::Shell { .. } => Vec::new(),
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
    let matches_name = name_lower == tool_lower || (name_lower == "bash" && tool_lower == "shell");
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
        ToolInput::Read { path } | ToolInput::Write { path, .. } => {
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

fn collect_grep_matches(pattern: &str, path: &Path, out: &mut Vec<String>) -> Result<()> {
    if path.is_dir() {
        for entry in fs::read_dir(path)? {
            let entry = entry?;
            collect_grep_matches(pattern, &entry.path(), out)?;
        }
        return Ok(());
    }

    if !path.is_file() {
        return Ok(());
    }

    let content = fs::read_to_string(path)?;
    for (idx, line) in content.lines().enumerate() {
        if line.contains(pattern) {
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
    if root.is_dir() {
        for entry in fs::read_dir(root)? {
            let entry = entry?;
            collect_glob_matches(&entry.path(), pattern, out)?;
        }
        return Ok(());
    }

    if root.is_file() {
        if matches_glob(pattern, root) {
            out.push(root.to_path_buf());
        }
    }
    Ok(())
}

fn matches_glob(pattern: &str, path: &Path) -> bool {
    let target = path.to_string_lossy();
    wildcard_match(pattern, &target)
}

fn build_diff(path: &Path, before: &str, after: &str) -> String {
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
