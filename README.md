# 👺 Tengu

A powerful AI coding agent CLI that unifies multiple LLMs.

Tengu is a flexible, multi-LLM coding agent that integrates with MCP servers, supports custom agents, and provides fine-grained permission control for your development workflow.

## ✨ Features

- **Autonomous Agent Loop**: Autonomous tool execution loop using
  LLM-native tool_use APIs (LLM -> ToolUse -> Result -> LLM...).
- **Multi-LLM Support**: Anthropic, OpenAI, Google, and Ollama support
  tool_use / function calling across providers.
- **Built-in Tools**: Read, Edit, Write, Bash, Grep, Glob, ListFiles,
  WebFetch, WebSearch, and agent orchestration helpers for coding and
  research workflows.
- **MCP Integration**: Connect to external tool servers over STDIO and HTTP
  transports.
- **Permission Control**: Per-tool permissions, sandboxing, and glob pattern
  matching.
- **Interactive TUI**: Streaming output, Markdown rendering, and tool
  execution logs.
- **Project Configuration**: Manage project context with hierarchical
  `.tengu/AGENT.md` files, with legacy `.tengu/TENGU.md` compatibility.

## Project Status

Core CLI/TUI, multi-provider LLM routing, built-in tools, MCP, permissions,
hooks, session resume, Git/review workflows, checkpoints, security defaults,
performance checks, encrypted token storage, and required user documentation are
implemented.
Remaining tracked gaps are broader E2E evidence, 80% coverage proof in a matching
LLVM environment, and advanced optional features.

## 🚀 Quick Start

### Installation

```bash
# Via Cargo (recommended)
cargo install tengu

# From source
git clone https://github.com/yourusername/tengu.git
cd tengu
cargo build --release
```

### Setup

```bash
# Set API keys
export ANTHROPIC_API_KEY="sk-ant-..."
export OPENAI_API_KEY="sk-..."
```

### Basic Usage

```bash
# Interactive TUI mode
tengu

# One-shot execution
tengu -p "Analyze this codebase"

# With specific model
tengu --model claude-sonnet-4 -p "Write tests"

# Allow file editing
tengu -p "Fix bugs" --allowed-tools "Read,Write,Shell"
```

### Connectivity Checks

Use these commands to verify each backend can start streaming responses from your local environment.

```bash
# Anthropic
export ANTHROPIC_API_KEY="sk-ant-..."
tengu --model claude-sonnet-4-20250514 -p "Reply with OK" --output-format stream-json

# OpenAI
export OPENAI_API_KEY="sk-..."
tengu --model gpt-4o-mini -p "Reply with OK" --output-format stream-json

# Google
export GOOGLE_API_KEY="..."
tengu --model gemini-2.0-flash -p "Reply with OK" --output-format stream-json
```

Expected behavior:

- A `start` event is printed first
- One or more `chunk` events follow
- A `usage` event may appear if the provider returns usage metadata
- An `end` event is printed last
- If a backend fails, an `error` event is emitted before exit

For `--output-format json`, Tengu prints a `{"type":"usage", ...}` object before the final `{"type":"response", ...}` object when the provider returns usage metadata.

You can also inspect provider auth readiness with `tengu auth status`.

For encrypted local token storage, set a passphrase and run login while the
provider API key environment variable is present:

```bash
export TENGU_AUTH_PASSPHRASE="use-a-long-local-passphrase"
export ANTHROPIC_API_KEY="sk-ant-..."
tengu auth login
tengu auth status
```

`auth login` writes encrypted tokens to `~/.tengu/auth/tokens.json` and session
metadata to `~/.tengu/auth/session.json`. `auth logout` removes both files.
Tengu does not print stored token values.

## 📖 Examples

For the full command guide, see [USAGE.md](USAGE.md). For configuration details,
see [CONFIGURATION.md](CONFIGURATION.md). For MCP server setup, see
[MCP_GUIDE.md](MCP_GUIDE.md).

### File Operations

```bash
# Create new file
tengu -p "Create utils.rs with helper functions" --allowed-tools "Write"

# Edit existing files
tengu -p "Fix lint errors in all .rs files" --allowed-tools "Read,Write,Shell(cargo *)"
```

### Review

```bash
# Review the current working tree diff
tengu review

# Review changes against a base branch
tengu review --base main

# Focus the review on security risks
tengu review --base main --preset security

# Check local performance baselines
tengu perf
tengu perf --format json --strict
```

### GitHub / GitLab

Tengu wraps the official `gh` and `glab` CLIs for issue, PR/MR, comment, review,
and label operations.

```bash
# GitHub issue operations
tengu issue view 123 --comments
tengu issue create --title "Fix parser" --body "Parser fails on empty input" --label bug
tengu issue labels 123 --add bug --remove triage

# GitLab issue operations
tengu issue --provider gitlab view 123 --comments
tengu issue --provider gitlab create --title "Fix parser" --body "Parser fails on empty input" --label bug

# PR/MR operations
tengu pr create --fill --draft
tengu pr --provider gitlab create --fill --draft
tengu pr comment 123 --body "Looks good"
tengu pr --provider gitlab comment 123 --body "Looks good"
tengu pr review 123 --action request-changes --body "Please add tests"

# Label operations
tengu label list
tengu label create bug --color ff0000 --description "Broken behavior"
tengu label --provider gitlab list
```

### Checkpoints

Tengu stores file snapshots under `.tengu/checkpoints` so edits can be inspected
or restored without creating Git commits or stashes. `Write` and `Edit` tool
operations create automatic checkpoints before changing files.

```bash
tengu checkpoint create src/main.rs --reason "before refactor"
tengu checkpoint list
tengu checkpoint diff
tengu checkpoint restore
```

### Coverage

```bash
# Line coverage summary with an 80% minimum
scripts/coverage.sh

# HTML report
COVERAGE_MIN_LINES=80 scripts/coverage.sh html
```

The coverage helper prefers `cargo llvm-cov` and falls back to
`cargo tarpaulin` when available. If the active `rustc` does not match the
available LLVM tools, set `LLVM_COV` and `LLVM_PROFDATA` to matching binaries.
The CI coverage job measures core non-interactive code at an 80% line threshold
and excludes interactive TUI rendering and network transport adapters.

In TUI, use `/review`, `/review --base main`, or `/review --preset security`.
You can also use `/new`, `/clear`, `/resume`, `/resume --last`, `/resume <session-id>`, `/save`, `/save <path>`, `/load <path>`, `/fork`, and `/diff` (optionally `/diff --stat`) for local session management and quick diff inspection. From the CLI, `tengu sessions list` shows resumable session IDs, and `tengu resume --last` or `tengu resume <session-id>` opens the selected session in the TUI.
Use `/image <path> [more_paths...]` to attach images to the next TUI prompt.
Dragging image file paths into the TUI input also auto-attaches them for the next prompt.
For local git and hosting actions, `/commit <message>`, `/pr [args]`, `/mr [args]`, `/issue ...`, `/label ...`, `/pr_comment ...`, and `/pr_review ...` ask for `y/n` confirmation before running `git`, `gh`, or `glab`; `/editor [path]` opens your `$VISUAL` or `$EDITOR`.
Saved sessions now restore conversation history, visible logs, queued prompts, pending image attachments, and approval prompts that can be acknowledged again after restore.
Additional TUI workflow commands now include `/plan`, `/taskwriter`, `/apply-plan`, `/compact`, `/memory`, `/init`, `/config`, `/doctor`, `/add-dir`, `/agents`, `/login`, `/logout`, `/pr_comments`, `/terminal-setup`, `/strategy`, `/bg`, `/usage`, `/model`, `/vim`, `/checkpoint`, and `/rollback`.
Use `/checkpoint list`, `/checkpoint create <path> [more_paths...]`, `/checkpoint diff [id]`, and `/rollback [id]` to inspect or restore saved snapshots.
`/config` supports `list`, `get <key>`, and `set <key> <value>` for common local settings such as `model.default`, `model.provider`, and `plan_mode`.
`/usage` shows provider-reported usage metadata when the selected provider returns it, aggregates it per provider, and preserves it in saved TUI sessions. `/usage export <path>` writes the current aggregated usage snapshot as JSON. Exact billing still depends on each provider pricing model and billing surfaces.

### Image Input

```bash
# Ask about a screenshot
tengu --image screenshot.png -p "Describe the UI issues in this screen"

# Pass multiple images
tengu --image before.png,after.png -p "Compare these two screenshots"
```

`--image` is currently supported for headless execution. Images are sent to the selected remote LLM provider, while tools and file operations stay local.

### MCP Servers

```bash
# Add PostgreSQL MCP server
tengu mcp add postgres -- npx @modelcontextprotocol/server-postgres postgresql://localhost/mydb

# Use MCP in queries
tengu -p "Get latest 10 users from database"
```

### Custom Agents

```bash
# List agents
tengu agent list

# Use specific agent
tengu --agent code-reviewer

# Create a local scaffold
tengu agent create my-agent

# Generate an agent with the current LLM provider
tengu agent generate
```

`agent create` writes a local scaffold under `./.tengu/agents/`, `agent generate` asks the current model to produce an agent JSON and saves it locally, and `--agent <name>` loads that prompt into the session.

### CI/CD

```yaml
# GitHub Actions example
- name: Auto-fix lint
  run: |
    tengu -p "Run lint and fix errors" \
      --allowed-tools "Read,Write,Shell(cargo *)"
```

## ⚙️ Configuration

Tengu reads configuration from `~/.tengu/config.toml` and `./.tengu/config.toml`.

### Basic Config (~/.tengu/config.toml)

```toml
[model]
provider = "anthropic"
default = "claude-sonnet-4-20250514"

[permissions]
approval_policy = "on-request"
allowed_tools = ["Read", "Write", "Bash(git *)"]

[security]
audit_log = ".tengu/audit.jsonl"
blocked_paths = ["secrets/**"]
```

`.env` and `.env.*` are blocked by default for file-oriented tools. Set `allow_env_files = true` under `[security]` only when that project intentionally permits those files.

### TUI Theme (~/.tengu/theme.toml)

TUI colors can be overridden by placing a theme file at `~/.tengu/theme.toml`.
Only keys you set are overridden; others fall back to `src/tui/theme.toml`.

```toml
user = "green"
assistant = "white"
system = "white"
status = "yellow"
queue = "dark_grey"
heading = "cyan"
inline_code = "cyan"
divider = "grey"
footer = "grey"
```

### Project Context (./.tengu/AGENT.md)

Tengu reads `.tengu/AGENT.md` and legacy `.tengu/TENGU.md` files from global, project, and workspace scopes.

```markdown
# Project Context

## Coding Standards
- Language: Rust 2021
- Follow clippy recommendations
- Document all public functions
```

## 🤝 Contributing

Contributions are welcome! See [CONTRIBUTING.md](CONTRIBUTING.md) for details.

## 📄 License

MIT License - see [LICENSE](LICENSE) for details.

## 🙏 Acknowledgments

Inspired by:

- [Claude Code](https://code.claude.com) - Anthropic
- [Codex CLI](https://github.com/openai/codex) - OpenAI
- [Aider](https://aider.chat/)
- [Model Context Protocol](https://modelcontextprotocol.io/)

---

Soar high like a Tengu, command AI with ease 👺
