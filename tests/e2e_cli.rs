use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use serde_json::Value;
use tempfile::TempDir;

struct E2eProject {
    root: TempDir,
    home: TempDir,
}

impl E2eProject {
    fn new() -> Self {
        Self {
            root: TempDir::new().expect("create project tempdir"),
            home: TempDir::new().expect("create home tempdir"),
        }
    }

    fn path(&self) -> &Path {
        self.root.path()
    }

    fn run(&self, args: &[&str]) -> Output {
        let output = Command::new(env!("CARGO_BIN_EXE_tengu"))
            .args(args)
            .current_dir(self.root.path())
            .env("HOME", self.home.path())
            .env_remove("ANTHROPIC_API_KEY")
            .env_remove("OPENAI_API_KEY")
            .env_remove("GOOGLE_API_KEY")
            .output()
            .expect("run tengu binary");

        assert!(
            output.status.success(),
            "command failed: tengu {}\nstatus: {}\nstdout:\n{}\nstderr:\n{}",
            args.join(" "),
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        output
    }

    fn run_fail(&self, args: &[&str]) -> Output {
        let output = Command::new(env!("CARGO_BIN_EXE_tengu"))
            .args(args)
            .current_dir(self.root.path())
            .env("HOME", self.home.path())
            .env_remove("ANTHROPIC_API_KEY")
            .env_remove("OPENAI_API_KEY")
            .env_remove("GOOGLE_API_KEY")
            .output()
            .expect("run tengu binary");

        assert!(
            !output.status.success(),
            "command unexpectedly succeeded: tengu {}\nstdout:\n{}\nstderr:\n{}",
            args.join(" "),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        output
    }
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).to_string()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).to_string()
}

fn assert_contains(haystack: &str, needle: &str) {
    assert!(
        haystack.contains(needle),
        "expected output to contain {needle:?}, got:\n{haystack}"
    );
}

#[test]
fn e2e_auth_status_and_session_resume_prompt_do_not_require_network() {
    let project = E2eProject::new();

    let auth = stdout(&project.run(&["auth", "status"]));
    assert_contains(&auth, "provider: anthropic");
    assert_contains(&auth, "ANTHROPIC_API_KEY=missing");
    assert_contains(&auth, "session=none");

    let created = stdout(&project.run(&["new"]));
    assert_contains(&created, "new session:");
    let session_id = created
        .trim()
        .strip_prefix("new session: ")
        .expect("new command prints session id")
        .to_string();

    let list = stdout(&project.run(&["sessions", "list"]));
    assert_contains(&list, &session_id);
    assert_contains(&list, "updated=");

    let resume = stdout(&project.run(&["resume"]));
    assert_contains(&resume, &session_id);
    assert_contains(&resume, "tengu resume <session-id>");
    assert_contains(&resume, "tengu resume --last");
}

#[test]
fn e2e_agent_and_mcp_commands_use_project_scoped_files() {
    let project = E2eProject::new();

    let empty_agents = stdout(&project.run(&["agent", "list"]));
    assert_contains(&empty_agents, "no agents");

    let created = stdout(&project.run(&["agent", "create", "reviewer"]));
    assert_contains(&created, ".tengu/agents/reviewer.json");

    let agents = stdout(&project.run(&["agent", "list"]));
    assert_contains(&agents, "reviewer Custom agent: reviewer");

    let removed = stdout(&project.run(&["agent", "remove", "reviewer"]));
    assert_contains(&removed, "agent removed: reviewer");

    let no_mcp = stdout(&project.run(&["mcp", "list"]));
    assert_contains(&no_mcp, "no mcp servers");

    let added = stdout(&project.run(&["mcp", "add", "echoer", "--", "echo", "hello"]));
    assert_contains(&added, "mcp server added: echoer");

    let mcp_list = stdout(&project.run(&["mcp", "list"]));
    assert_contains(&mcp_list, "echoer stdio echo hello");

    let removed = stdout(&project.run(&["mcp", "remove", "echoer"]));
    assert_contains(&removed, "mcp server removed: echoer");
}

#[test]
fn e2e_tool_commands_read_write_search_and_enforce_sensitive_defaults() {
    let project = E2eProject::new();
    fs::write(
        project.path().join("notes.txt"),
        "alpha\nbeta\nalpha beta\n",
    )
    .expect("write fixture");

    let read = stdout(&project.run(&["tool", "read", "notes.txt"]));
    assert_contains(&read, "1\talpha");
    assert_contains(&read, "3\talpha beta");

    let grep = stdout(&project.run(&["tool", "grep", "alpha", "notes.txt"]));
    assert_contains(&grep, "notes.txt:1:alpha");
    assert_contains(&grep, "notes.txt:3:alpha beta");

    let glob = stdout(&project.run(&["tool", "glob", "*.txt"]));
    assert_contains(&glob, "notes.txt");

    let write = stdout(&project.run(&["tool", "write", "generated.txt", "created by e2e"]));
    assert_contains(&write, "--- generated.txt");
    assert_contains(&write, "Successfully wrote generated.txt");
    assert_eq!(
        fs::read_to_string(project.path().join("generated.txt")).expect("read generated file"),
        "created by e2e"
    );

    fs::write(project.path().join(".env"), "OPENAI_API_KEY=sk-secret").expect("write env file");
    let denied = project.run_fail(&["tool", "read", ".env"]);
    assert_contains(&stderr(&denied), "security policy blocked sensitive path");
}

#[test]
fn e2e_large_repository_file_tools_handle_many_files() {
    let project = E2eProject::new();
    let src = project.path().join("src");
    fs::create_dir_all(&src).expect("create src fixture");

    for idx in 0..160 {
        let module = src.join(format!("module_{idx:03}"));
        fs::create_dir_all(&module).expect("create module fixture");
        fs::write(
            module.join(format!("file_{idx:03}.rs")),
            format!("pub fn value_{idx:03}() -> &'static str {{\n    \"needle_{idx:03}\"\n}}\n"),
        )
        .expect("write source fixture");
    }
    fs::create_dir_all(project.path().join("target")).expect("create ignored fixture dir");
    fs::write(project.path().join("target/generated.rs"), "needle_159")
        .expect("write generated fixture");

    let glob = stdout(&project.run(&["tool", "glob", "*.rs", "src"]));
    assert_contains(&glob, "src/module_000/file_000.rs");
    assert_contains(&glob, "src/module_159/file_159.rs");

    let grep = stdout(&project.run(&["tool", "grep", "needle_137", "src"]));
    assert_contains(&grep, "src/module_137/file_137.rs:2:");
    assert_contains(&grep, "needle_137");

    let read = stdout(&project.run(&["tool", "read", "src/module_159/file_159.rs"]));
    assert_contains(&read, "1\tpub fn value_159()");
    assert_contains(&read, "2\t    \"needle_159\"");
}

#[test]
fn e2e_perf_json_has_required_metrics_and_bad_format_fails() {
    let project = E2eProject::new();

    let perf = stdout(&project.run(&["perf", "--format", "json"]));
    let json: Value = serde_json::from_str(&perf).expect("perf output is json");
    let metrics = json["metrics"].as_array().expect("metrics array");
    let names = metrics
        .iter()
        .filter_map(|metric| metric["name"].as_str())
        .collect::<Vec<_>>();

    assert!(names.contains(&"startup_path"));
    assert!(names.contains(&"command_dispatch"));
    assert!(names.contains(&"file_read_1mb"));
    assert!(names.contains(&"rss_memory"));

    let bad = project.run_fail(&["perf", "--format", "xml"]);
    assert_contains(&stderr(&bad), "unsupported perf format: xml");
}
