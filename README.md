# 👺 Tengu

**A powerful AI coding agent CLI that unifies multiple LLMs**

Tengu is a flexible, multi-LLM coding agent that integrates with MCP servers, supports custom agents, and provides fine-grained permission control for your development workflow.

## ✨ Features

- **Multi-LLM Support**: OpenAI, Anthropic, Google, and local models
- **MCP Integration**: Connect to Model Context Protocol servers
- **Custom Agents**: Define specialized agents with custom prompts and tools
- **Permission Control**: Fine-grained tool permissions with glob patterns
- **Hooks & Automation**: Pre/post-execution hooks for workflow automation
- **Project Configuration**: Hierarchical `AGENT.md` files for project context

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
# Interactive mode
tengu

# One-shot execution
tengu -p "Analyze this codebase"

# With specific model
tengu --model claude-sonnet-4 -p "Write tests"

# Allow file editing
tengu -p "Fix bugs" --allowed-tools "Read,Write,Edit"
```

## 📖 Examples

### File Operations

```bash
# Create new file
tengu -p "Create utils.rs with helper functions" --allowed-tools "Write"

# Edit existing files
tengu -p "Fix lint errors in all .rs files" --allowed-tools "Read,Write,Edit,Bash(cargo:*)"
```

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

# Create new agent
tengu agent create my-agent
```

### CI/CD

```yaml
# GitHub Actions example
- name: Auto-fix lint
  run: |
    tengu -p "Run lint and fix errors" \
      --allowed-tools "Read,Write,Edit,Bash(cargo:*)"
```

## ⚙️ Configuration

See [`config.toml.example`](config.toml.example) and [`AGENT.md.example`](AGENT.md.example) for full configuration options.

### Basic Config (~/.tengu/config.toml)

```toml
[model]
provider = "anthropic"
default = "claude-sonnet-4-20250514"

[permissions]
approval_policy = "on-request"
allowed_tools = ["Read", "Write", "Bash(git *)"]
```

### Project Context (./AGENT.md)

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

**Soar high like a Tengu, command AI with ease 👺**
