# 👺 Tengu

A powerful AI coding agent CLI that unifies multiple LLMs.

Tengu is a flexible, multi-LLM coding agent that integrates with MCP servers, supports custom agents, and provides fine-grained permission control for your development workflow.

## ✨ Features

- **Multi-LLM Support**: OpenAI, Anthropic, Google, and local models
- **MCP Integration**: Connect to Model Context Protocol servers
- **Custom Agents**: Define specialized agents with custom prompts and tools
- **Permission Control**: Fine-grained tool permissions with glob patterns
- **Streaming JSON**: `start` / `chunk` / `usage` / `tool` / `error` / `end` events for automation
- **Project Configuration**: Hierarchical `.tengu/TENGU.md` files for project context

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

## 📖 Examples

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
```

In TUI, use `/review`, `/review --base main`, or `/review --preset security`.
You can also use `/new`, `/clear`, `/resume`, `/save`, `/save <path>`, `/load <path>`, `/fork`, and `/diff` (optionally `/diff --stat`) for local session management and quick diff inspection.
Use `/image <path> [more_paths...]` to attach images to the next TUI prompt.
Dragging image file paths into the TUI input also auto-attaches them for the next prompt.
For local git actions, `/commit <message>` and `/pr [args]` ask for `y/n` confirmation before running `git commit` or `gh pr create`, and `/editor [path]` opens your `$VISUAL` or `$EDITOR`.
Saved sessions now restore conversation history, visible logs, queued prompts, pending image attachments, and approval prompts that can be acknowledged again after restore.
Additional TUI workflow commands now include `/plan`, `/taskwriter`, `/apply-plan`, `/compact`, `/memory`, `/init`, `/config`, `/doctor`, `/add-dir`, `/agents`, `/login`, `/logout`, `/pr_comments`, `/terminal-setup`, `/strategy`, `/bg`, `/usage`, `/model`, and `/vim`.
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
```

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

### Project Context (./.tengu/TENGU.md)

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
