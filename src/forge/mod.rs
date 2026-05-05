use std::path::Path;
use std::process::Command;

use clap::ValueEnum;

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum ForgeProvider {
    Github,
    Gitlab,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum ReviewAction {
    Comment,
    Approve,
    RequestChanges,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForgeCommand {
    pub program: String,
    pub args: Vec<String>,
}

impl ForgeCommand {
    pub fn label(&self) -> String {
        if self.args.is_empty() {
            self.program.clone()
        } else {
            format!("{} {}", self.program, self.args.join(" "))
        }
    }

    pub fn run_in_dir(&self, cwd: &Path) -> String {
        let output = Command::new(&self.program)
            .args(&self.args)
            .current_dir(cwd)
            .output();
        format_command_result(&self.label(), output)
    }
}

pub fn issue_view(
    provider: ForgeProvider,
    number: &str,
    comments: bool,
    repo: Option<&str>,
) -> ForgeCommand {
    let mut cmd = base(provider, &["issue", view_verb(provider)]);
    cmd.args.push(number.to_string());
    if comments {
        cmd.args.push(comments_flag(provider).to_string());
    }
    push_repo(&mut cmd, repo);
    cmd
}

pub fn issue_create(
    provider: ForgeProvider,
    title: &str,
    body: Option<&str>,
    labels: &[String],
    repo: Option<&str>,
) -> ForgeCommand {
    let mut cmd = base(provider, &["issue", "create"]);
    match provider {
        ForgeProvider::Github => {
            cmd.args.extend(["--title".to_string(), title.to_string()]);
            if let Some(body) = body {
                cmd.args.extend(["--body".to_string(), body.to_string()]);
            }
            for label in labels {
                cmd.args.extend(["--label".to_string(), label.clone()]);
            }
        }
        ForgeProvider::Gitlab => {
            cmd.args.extend(["--title".to_string(), title.to_string()]);
            if let Some(body) = body {
                cmd.args
                    .extend(["--description".to_string(), body.to_string()]);
            }
            for label in labels {
                cmd.args.extend(["--label".to_string(), label.clone()]);
            }
        }
    }
    push_repo(&mut cmd, repo);
    cmd
}

pub fn issue_comment(
    provider: ForgeProvider,
    number: &str,
    body: &str,
    repo: Option<&str>,
) -> ForgeCommand {
    let mut cmd = match provider {
        ForgeProvider::Github => base(provider, &["issue", "comment"]),
        ForgeProvider::Gitlab => base(provider, &["issue", "note"]),
    };
    cmd.args.push(number.to_string());
    match provider {
        ForgeProvider::Github => cmd.args.extend(["--body".to_string(), body.to_string()]),
        ForgeProvider::Gitlab => cmd.args.extend(["--message".to_string(), body.to_string()]),
    }
    push_repo(&mut cmd, repo);
    cmd
}

pub fn issue_labels(
    provider: ForgeProvider,
    number: &str,
    add: &[String],
    remove: &[String],
    repo: Option<&str>,
) -> ForgeCommand {
    let mut cmd = match provider {
        ForgeProvider::Github => base(provider, &["issue", "edit"]),
        ForgeProvider::Gitlab => base(provider, &["issue", "update"]),
    };
    cmd.args.push(number.to_string());
    for label in add {
        cmd.args.push(
            match provider {
                ForgeProvider::Github => "--add-label",
                ForgeProvider::Gitlab => "--label",
            }
            .to_string(),
        );
        cmd.args.push(label.clone());
    }
    for label in remove {
        cmd.args.push(
            match provider {
                ForgeProvider::Github => "--remove-label",
                ForgeProvider::Gitlab => "--unlabel",
            }
            .to_string(),
        );
        cmd.args.push(label.clone());
    }
    push_repo(&mut cmd, repo);
    cmd
}

pub fn pr_create(provider: ForgeProvider, args: &[String], repo: Option<&str>) -> ForgeCommand {
    let mut cmd = match provider {
        ForgeProvider::Github => base(provider, &["pr", "create"]),
        ForgeProvider::Gitlab => base(provider, &["mr", "create"]),
    };
    cmd.args.extend(args.iter().cloned());
    push_repo(&mut cmd, repo);
    cmd
}

pub fn pr_comments(
    provider: ForgeProvider,
    target: Option<&str>,
    repo: Option<&str>,
) -> ForgeCommand {
    let mut cmd = match provider {
        ForgeProvider::Github => base(provider, &["pr", "view"]),
        ForgeProvider::Gitlab => base(provider, &["mr", "note", "list"]),
    };
    if let Some(target) = target {
        cmd.args.push(target.to_string());
    }
    if provider == ForgeProvider::Github {
        cmd.args.push("--comments".to_string());
    }
    push_repo(&mut cmd, repo);
    cmd
}

pub fn pr_comment(
    provider: ForgeProvider,
    target: Option<&str>,
    body: &str,
    repo: Option<&str>,
) -> ForgeCommand {
    let mut cmd = match provider {
        ForgeProvider::Github => base(provider, &["pr", "comment"]),
        ForgeProvider::Gitlab => base(provider, &["mr", "note"]),
    };
    if let Some(target) = target {
        cmd.args.push(target.to_string());
    }
    match provider {
        ForgeProvider::Github => cmd.args.extend(["--body".to_string(), body.to_string()]),
        ForgeProvider::Gitlab => cmd.args.extend(["--message".to_string(), body.to_string()]),
    }
    push_repo(&mut cmd, repo);
    cmd
}

pub fn pr_review(
    provider: ForgeProvider,
    target: Option<&str>,
    action: ReviewAction,
    body: Option<&str>,
    repo: Option<&str>,
) -> ForgeCommand {
    let mut cmd = match (provider, action) {
        (ForgeProvider::Github, _) => base(provider, &["pr", "review"]),
        (ForgeProvider::Gitlab, ReviewAction::Approve) => base(provider, &["mr", "approve"]),
        (ForgeProvider::Gitlab, _) => base(provider, &["mr", "note"]),
    };
    if let Some(target) = target {
        cmd.args.push(target.to_string());
    }
    match (provider, action) {
        (ForgeProvider::Github, ReviewAction::Comment) => cmd.args.push("--comment".to_string()),
        (ForgeProvider::Github, ReviewAction::Approve) => cmd.args.push("--approve".to_string()),
        (ForgeProvider::Github, ReviewAction::RequestChanges) => {
            cmd.args.push("--request-changes".to_string());
        }
        (ForgeProvider::Gitlab, ReviewAction::Approve) => {}
        (ForgeProvider::Gitlab, ReviewAction::Comment) => {
            if let Some(body) = body {
                cmd.args.extend(["--message".to_string(), body.to_string()]);
            }
        }
        (ForgeProvider::Gitlab, ReviewAction::RequestChanges) => {
            let message = body
                .map(|body| format!("Request changes: {body}"))
                .unwrap_or_else(|| "Request changes".to_string());
            cmd.args.extend(["--message".to_string(), message]);
        }
    }
    if provider == ForgeProvider::Github {
        if let Some(body) = body {
            cmd.args.extend(["--body".to_string(), body.to_string()]);
        }
    }
    push_repo(&mut cmd, repo);
    cmd
}

pub fn label_list(provider: ForgeProvider, repo: Option<&str>) -> ForgeCommand {
    let mut cmd = base(provider, &["label", "list"]);
    push_repo(&mut cmd, repo);
    cmd
}

pub fn label_create(
    provider: ForgeProvider,
    name: &str,
    color: Option<&str>,
    description: Option<&str>,
    repo: Option<&str>,
) -> ForgeCommand {
    let mut cmd = base(provider, &["label", "create"]);
    match provider {
        ForgeProvider::Github => cmd.args.push(name.to_string()),
        ForgeProvider::Gitlab => cmd.args.extend(["--name".to_string(), name.to_string()]),
    }
    if let Some(color) = color {
        cmd.args.extend(["--color".to_string(), color.to_string()]);
    }
    if let Some(description) = description {
        cmd.args
            .extend(["--description".to_string(), description.to_string()]);
    }
    push_repo(&mut cmd, repo);
    cmd
}

pub fn label_edit(
    provider: ForgeProvider,
    name: &str,
    new_name: Option<&str>,
    color: Option<&str>,
    description: Option<&str>,
    repo: Option<&str>,
) -> ForgeCommand {
    let mut cmd = base(provider, &["label", "edit"]);
    match provider {
        ForgeProvider::Github => cmd.args.push(name.to_string()),
        ForgeProvider::Gitlab => cmd
            .args
            .extend(["--label-id".to_string(), name.to_string()]),
    }
    if let Some(new_name) = new_name {
        cmd.args.push(
            match provider {
                ForgeProvider::Github => "--name",
                ForgeProvider::Gitlab => "--new-name",
            }
            .to_string(),
        );
        cmd.args.push(new_name.to_string());
    }
    if let Some(color) = color {
        cmd.args.extend(["--color".to_string(), color.to_string()]);
    }
    if let Some(description) = description {
        cmd.args
            .extend(["--description".to_string(), description.to_string()]);
    }
    push_repo(&mut cmd, repo);
    cmd
}

pub fn label_delete(provider: ForgeProvider, name: &str, repo: Option<&str>) -> ForgeCommand {
    let mut cmd = base(provider, &["label", "delete"]);
    cmd.args.push(name.to_string());
    if provider == ForgeProvider::Github {
        cmd.args.push("--yes".to_string());
    }
    push_repo(&mut cmd, repo);
    cmd
}

fn base(provider: ForgeProvider, args: &[&str]) -> ForgeCommand {
    ForgeCommand {
        program: match provider {
            ForgeProvider::Github => "gh",
            ForgeProvider::Gitlab => "glab",
        }
        .to_string(),
        args: args.iter().map(|arg| (*arg).to_string()).collect(),
    }
}

fn push_repo(cmd: &mut ForgeCommand, repo: Option<&str>) {
    if let Some(repo) = repo.filter(|repo| !repo.trim().is_empty()) {
        cmd.args.extend(["--repo".to_string(), repo.to_string()]);
    }
}

fn view_verb(provider: ForgeProvider) -> &'static str {
    match provider {
        ForgeProvider::Github => "view",
        ForgeProvider::Gitlab => "view",
    }
}

fn comments_flag(provider: ForgeProvider) -> &'static str {
    match provider {
        ForgeProvider::Github => "--comments",
        ForgeProvider::Gitlab => "--comments",
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_github_issue_create_with_labels() {
        let cmd = issue_create(
            ForgeProvider::Github,
            "bug",
            Some("broken"),
            &["bug".to_string(), "help wanted".to_string()],
            Some("owner/repo"),
        );
        assert_eq!(cmd.program, "gh");
        assert_eq!(
            cmd.args,
            vec![
                "issue",
                "create",
                "--title",
                "bug",
                "--body",
                "broken",
                "--label",
                "bug",
                "--label",
                "help wanted",
                "--repo",
                "owner/repo"
            ]
        );
    }

    #[test]
    fn builds_gitlab_issue_create_with_description() {
        let cmd = issue_create(
            ForgeProvider::Gitlab,
            "bug",
            Some("broken"),
            &["bug".to_string()],
            None,
        );
        assert_eq!(cmd.program, "glab");
        assert_eq!(
            cmd.args,
            vec![
                "issue",
                "create",
                "--title",
                "bug",
                "--description",
                "broken",
                "--label",
                "bug"
            ]
        );
    }

    #[test]
    fn builds_optional_forge_arguments_when_absent() {
        assert_eq!(
            ForgeCommand {
                program: "gh".to_string(),
                args: vec![]
            }
            .label(),
            "gh"
        );
        assert_eq!(
            issue_view(ForgeProvider::Github, "7", false, Some("   ")).args,
            vec!["issue", "view", "7"]
        );
        assert_eq!(
            issue_create(ForgeProvider::Github, "bug", None, &[], None).args,
            vec!["issue", "create", "--title", "bug"]
        );
        assert_eq!(
            issue_create(ForgeProvider::Gitlab, "bug", None, &[], None).args,
            vec!["issue", "create", "--title", "bug"]
        );
        assert_eq!(
            pr_comment(ForgeProvider::Github, None, "body", None).args,
            vec!["pr", "comment", "--body", "body"]
        );
        assert_eq!(
            label_create(ForgeProvider::Gitlab, "bug", None, None, None).args,
            vec!["label", "create", "--name", "bug"]
        );
        assert_eq!(
            label_edit(ForgeProvider::Github, "bug", None, None, None, None).args,
            vec!["label", "edit", "bug"]
        );
    }

    #[test]
    fn builds_pr_comment_for_both_providers() {
        assert_eq!(
            pr_comment(ForgeProvider::Github, Some("12"), "looks good", None).args,
            vec!["pr", "comment", "12", "--body", "looks good"]
        );
        assert_eq!(
            pr_comment(ForgeProvider::Gitlab, Some("12"), "looks good", None).args,
            vec!["mr", "note", "12", "--message", "looks good"]
        );
    }

    #[test]
    fn builds_review_commands() {
        assert_eq!(
            pr_review(
                ForgeProvider::Github,
                Some("12"),
                ReviewAction::RequestChanges,
                Some("needs tests"),
                None,
            )
            .args,
            vec![
                "pr",
                "review",
                "12",
                "--request-changes",
                "--body",
                "needs tests"
            ]
        );
        assert_eq!(
            pr_review(
                ForgeProvider::Gitlab,
                Some("12"),
                ReviewAction::Approve,
                None,
                None,
            )
            .args,
            vec!["mr", "approve", "12"]
        );
    }

    #[test]
    fn builds_label_management_commands() {
        assert_eq!(
            label_create(
                ForgeProvider::Github,
                "bug",
                Some("ff0000"),
                Some("Broken behavior"),
                None,
            )
            .args,
            vec![
                "label",
                "create",
                "bug",
                "--color",
                "ff0000",
                "--description",
                "Broken behavior"
            ]
        );
        assert_eq!(
            issue_labels(
                ForgeProvider::Gitlab,
                "42",
                &["bug".to_string()],
                &["triage".to_string()],
                None,
            )
            .args,
            vec![
                "issue",
                "update",
                "42",
                "--label",
                "bug",
                "--unlabel",
                "triage"
            ]
        );
    }

    #[test]
    fn builds_issue_view_comment_and_label_commands() {
        assert_eq!(
            issue_view(ForgeProvider::Github, "7", true, Some("owner/repo")).args,
            vec!["issue", "view", "7", "--comments", "--repo", "owner/repo"]
        );
        assert_eq!(
            issue_comment(ForgeProvider::Gitlab, "7", "fixed", Some("group/project")).args,
            vec![
                "issue",
                "note",
                "7",
                "--message",
                "fixed",
                "--repo",
                "group/project"
            ]
        );
        assert_eq!(
            issue_labels(
                ForgeProvider::Github,
                "7",
                &["bug".to_string()],
                &["wip".to_string()],
                Some("owner/repo"),
            )
            .args,
            vec![
                "issue",
                "edit",
                "7",
                "--add-label",
                "bug",
                "--remove-label",
                "wip",
                "--repo",
                "owner/repo"
            ]
        );
    }

    #[test]
    fn builds_pr_create_and_comment_listing_commands() {
        assert_eq!(
            pr_create(
                ForgeProvider::Github,
                &["--title".to_string(), "ready".to_string()],
                Some("owner/repo"),
            )
            .args,
            vec!["pr", "create", "--title", "ready", "--repo", "owner/repo"]
        );
        assert_eq!(
            pr_create(ForgeProvider::Gitlab, &["--fill".to_string()], None).args,
            vec!["mr", "create", "--fill"]
        );
        assert_eq!(
            pr_comments(ForgeProvider::Github, None, None).args,
            vec!["pr", "view", "--comments"]
        );
        assert_eq!(
            pr_comments(ForgeProvider::Gitlab, Some("8"), Some("group/project")).args,
            vec!["mr", "note", "list", "8", "--repo", "group/project"]
        );
    }

    #[test]
    fn builds_remaining_review_and_label_commands() {
        assert_eq!(
            pr_review(
                ForgeProvider::Github,
                Some("12"),
                ReviewAction::Comment,
                Some("note"),
                None,
            )
            .args,
            vec!["pr", "review", "12", "--comment", "--body", "note"]
        );
        assert_eq!(
            pr_review(
                ForgeProvider::Github,
                Some("12"),
                ReviewAction::Approve,
                None,
                None,
            )
            .args,
            vec!["pr", "review", "12", "--approve"]
        );
        assert_eq!(
            pr_review(
                ForgeProvider::Gitlab,
                Some("12"),
                ReviewAction::RequestChanges,
                Some("needs work"),
                None,
            )
            .args,
            vec![
                "mr",
                "note",
                "12",
                "--message",
                "Request changes: needs work"
            ]
        );
        assert_eq!(
            pr_review(
                ForgeProvider::Gitlab,
                Some("12"),
                ReviewAction::Comment,
                None,
                None,
            )
            .args,
            vec!["mr", "note", "12"]
        );
        assert_eq!(
            label_list(ForgeProvider::Gitlab, Some("group/project")).args,
            vec!["label", "list", "--repo", "group/project"]
        );
        assert_eq!(
            label_edit(
                ForgeProvider::Github,
                "bug",
                Some("defect"),
                Some("00ff00"),
                None,
                None,
            )
            .args,
            vec!["label", "edit", "bug", "--name", "defect", "--color", "00ff00"]
        );
        assert_eq!(
            label_edit(
                ForgeProvider::Gitlab,
                "bug",
                Some("defect"),
                None,
                Some("Broken behavior"),
                None,
            )
            .args,
            vec![
                "label",
                "edit",
                "--label-id",
                "bug",
                "--new-name",
                "defect",
                "--description",
                "Broken behavior"
            ]
        );
        assert_eq!(
            label_delete(ForgeProvider::Github, "bug", None).args,
            vec!["label", "delete", "bug", "--yes"]
        );
        assert_eq!(
            label_delete(ForgeProvider::Gitlab, "bug", Some("group/project")).args,
            vec!["label", "delete", "bug", "--repo", "group/project"]
        );
        assert_eq!(
            pr_review(
                ForgeProvider::Gitlab,
                None,
                ReviewAction::RequestChanges,
                None,
                None,
            )
            .args,
            vec!["mr", "note", "--message", "Request changes"]
        );
        assert_eq!(
            label_delete(ForgeProvider::Github, "bug", Some("owner/repo")).args,
            vec!["label", "delete", "bug", "--yes", "--repo", "owner/repo"]
        );
    }

    #[test]
    fn command_label_and_run_failure_are_readable() {
        let dir = tempfile::tempdir().unwrap();
        let cmd = ForgeCommand {
            program: "definitely-not-a-forge-binary".to_string(),
            args: vec!["issue".to_string(), "list".to_string()],
        };

        assert_eq!(cmd.label(), "definitely-not-a-forge-binary issue list");
        assert!(cmd.run_in_dir(dir.path()).contains("failed"));
    }

    #[test]
    fn command_run_formats_success_and_failure_outputs() {
        let dir = tempfile::tempdir().unwrap();
        let echo = ForgeCommand {
            program: "printf".to_string(),
            args: vec!["ok".to_string()],
        };
        assert_eq!(echo.run_in_dir(dir.path()), "ok");

        let success_without_stdout = ForgeCommand {
            program: "true".to_string(),
            args: vec![],
        };
        assert_eq!(
            success_without_stdout.run_in_dir(dir.path()),
            "true succeeded"
        );

        let failure_without_stderr = ForgeCommand {
            program: "false".to_string(),
            args: vec![],
        };
        assert_eq!(
            failure_without_stderr.run_in_dir(dir.path()),
            "false failed"
        );
    }
}
