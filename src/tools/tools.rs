// Tools module
// ビルトインツール

use anyhow::{anyhow, Result};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

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
    Read { path: PathBuf },
    Write { path: PathBuf, content: String },
    Shell { command: String, args: Vec<String> },
    Grep { pattern: String, paths: Vec<PathBuf> },
    Glob { pattern: String, root: Option<PathBuf> },
}

#[derive(Debug)]
pub enum ToolResult {
    Text(String),
    Lines(Vec<String>),
    Paths(Vec<PathBuf>),
    Status(i32),
}

pub struct ToolExecutor;

impl ToolExecutor {
    pub fn new() -> Self {
        Self
    }

    pub fn execute(&self, input: ToolInput) -> Result<ToolResult> {
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
                    Ok(ToolResult::Text(String::from_utf8_lossy(&output.stdout).to_string()))
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
