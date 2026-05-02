use std::path::Path;
use std::process::Command;

use anyhow::{anyhow, Result};

#[derive(Debug, Clone, Default)]
pub struct ReviewOptions {
    pub base: Option<String>,
    pub preset: Option<String>,
}

pub fn parse_review_args(args: &[&str]) -> Result<ReviewOptions> {
    let mut options = ReviewOptions::default();
    let mut idx = 0usize;

    while idx < args.len() {
        match args[idx] {
            "--base" => {
                idx += 1;
                let value = args
                    .get(idx)
                    .ok_or_else(|| anyhow!("--base requires a value"))?;
                options.base = Some((*value).to_string());
            }
            "--preset" => {
                idx += 1;
                let value = args
                    .get(idx)
                    .ok_or_else(|| anyhow!("--preset requires a value"))?;
                options.preset = Some((*value).to_string());
            }
            other => {
                return Err(anyhow!("unsupported review arg: {}", other));
            }
        }
        idx += 1;
    }

    Ok(options)
}

pub fn build_review_prompt(options: &ReviewOptions) -> Result<Option<String>> {
    build_review_prompt_in_dir(options, Path::new("."))
}

fn build_review_prompt_in_dir(options: &ReviewOptions, cwd: &Path) -> Result<Option<String>> {
    let range = options.base.as_deref().map(|base| format!("{base}...HEAD"));

    let stat = run_git_diff(range.as_deref(), true, cwd)?;
    let diff = run_git_diff(range.as_deref(), false, cwd)?;
    Ok(build_review_prompt_from_diff(&stat, &diff, options))
}

fn build_review_prompt_from_diff(
    stat: &str,
    diff: &str,
    options: &ReviewOptions,
) -> Option<String> {
    if diff.trim().is_empty() {
        return None;
    }

    let preset = options
        .preset
        .as_deref()
        .unwrap_or("general")
        .trim()
        .to_ascii_lowercase();

    let focus = match preset.as_str() {
        "security" => "セキュリティリスク、権限の欠陥、秘密情報漏えい",
        "performance" => "性能劣化、無駄な I/O、不要な再計算",
        "correctness" => "バグ、回帰、仕様不整合",
        _ => "バグ、リスク、回帰、テスト不足",
    };

    let scope = options
        .base
        .as_deref()
        .map(|base| format!("{base}...HEAD"))
        .unwrap_or_else(|| "working tree".to_string());
    let prompt = format!(
        "以下の Git 差分をコードレビューしてください。\n\
         出力は重要度順に、問題点を先に列挙してください。\n\
         特に {focus} に注目してください。\n\
         問題がなければ、その旨と残留リスクを簡潔に述べてください。\n\n\
         対象範囲: {scope}\n\n\
         Diff Stat:\n{stat}\n\n\
         Unified Diff:\n{diff}"
    );

    Some(prompt)
}

fn run_git_diff(range: Option<&str>, stat_only: bool, cwd: &Path) -> Result<String> {
    let mut command = Command::new("git");
    command.arg("diff").arg("--no-ext-diff");
    command.current_dir(cwd);
    if stat_only {
        command.arg("--stat");
    }
    if let Some(range) = range {
        command.arg(range);
    }

    let output = command.output()?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!("git diff failed: {}", stderr.trim()));
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn setup_git_repo_with_worktree_diff() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        run_git(dir.path(), &["init"]);
        run_git(
            dir.path(),
            &[
                "-c",
                "user.name=Tengu Test",
                "-c",
                "user.email=tengu@example.test",
                "commit",
                "--allow-empty",
                "-m",
                "initial",
            ],
        );
        fs::write(dir.path().join("notes.txt"), "before\n").unwrap();
        run_git(dir.path(), &["add", "notes.txt"]);
        run_git(
            dir.path(),
            &[
                "-c",
                "user.name=Tengu Test",
                "-c",
                "user.email=tengu@example.test",
                "commit",
                "-m",
                "add notes",
            ],
        );
        fs::write(dir.path().join("notes.txt"), "before\nafter\n").unwrap();
        dir
    }

    fn run_git(cwd: &Path, args: &[&str]) {
        let output = Command::new("git")
            .args(args)
            .current_dir(cwd)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "git {:?} failed: {}",
            args,
            String::from_utf8_lossy(&output.stderr)
        );
    }

    #[test]
    fn parses_review_args_with_base_and_preset() {
        let options = parse_review_args(&["--base", "main", "--preset", "security"]).unwrap();
        assert_eq!(options.base.as_deref(), Some("main"));
        assert_eq!(options.preset.as_deref(), Some("security"));
    }

    #[test]
    fn rejects_unknown_review_arg() {
        let result = parse_review_args(&["--unknown"]);
        assert!(result.is_err());
    }

    #[test]
    fn builds_prompt_from_supplied_diff_and_options() {
        let prompt = build_review_prompt_from_diff(
            " src/main.rs | 2 ++",
            "diff --git a/src/main.rs b/src/main.rs",
            &ReviewOptions {
                base: Some("main".to_string()),
                preset: Some("security".to_string()),
            },
        )
        .unwrap();

        assert!(prompt.contains("対象範囲: main...HEAD"));
        assert!(prompt.contains("セキュリティリスク"));
        assert!(prompt.contains("Diff Stat:"));
        assert!(prompt.contains("Unified Diff:"));
    }

    #[test]
    fn returns_none_when_diff_is_empty() {
        let prompt =
            build_review_prompt_from_diff(" no files changed", "", &ReviewOptions::default());
        assert!(prompt.is_none());
    }

    #[test]
    fn builds_review_prompt_from_real_git_worktree_diff() {
        let dir = setup_git_repo_with_worktree_diff();

        let prompt = build_review_prompt_in_dir(
            &ReviewOptions {
                base: None,
                preset: Some("correctness".to_string()),
            },
            dir.path(),
        )
        .unwrap()
        .unwrap();

        assert!(prompt.contains("対象範囲: working tree"));
        assert!(prompt.contains("バグ、回帰、仕様不整合"));
        assert!(prompt.contains("notes.txt"));
        assert!(prompt.contains("+after"));
    }

    #[test]
    fn builds_review_prompt_from_real_git_base_range() {
        let dir = setup_git_repo_with_worktree_diff();

        let prompt = build_review_prompt_in_dir(
            &ReviewOptions {
                base: Some("HEAD~1".to_string()),
                preset: Some("performance".to_string()),
            },
            dir.path(),
        )
        .unwrap()
        .unwrap();

        assert!(prompt.contains("対象範囲: HEAD~1...HEAD"));
        assert!(prompt.contains("性能劣化"));
        assert!(prompt.contains("notes.txt"));
    }
}
