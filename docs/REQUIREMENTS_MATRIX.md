# Requirements Matrix

Updated: 2026-05-02

This matrix tracks `docs/REQUIREMENTS.md` against the current implementation.
Task IDs refer to the product task list in `TASK.md`.

Status meanings:

- `Complete`: the current implementation substantially satisfies the requirement.
- `Partial`: the main path exists, but documented behavior or verification is incomplete.
- `Gap`: the requirement is not implemented or lacks enough evidence to treat as complete.

| Requirement | Section | Priority | Status | MappedTasks | Notes |
| --- | --- | --- | --- | --- | --- |
| Core CLI modes and formats | 1. Core Functional Requirements | Must | Partial | ALT-014,ALT-015 | TUI, headless `-p`, `json`, and real `stream-json` exist. `--cwd`, `--add-dir`, and some startup controls need execution-path alignment. |
| Session persistence and commands | 1.2 Session Management | Must | Partial | ALT-014,ALT-021 | File-based sessions, list, save/load, fork, and TUI resume exist. CLI `resume` mostly prints metadata, and resume E2E coverage should be strengthened. |
| Provider abstraction and model switching | 2. LLM Model Management | Must | Partial | ALT-014,ALT-019 | Anthropic, OpenAI, Google, and Ollama backends exist. TUI `/model` and CLI `--model` exist, but config schema and provider-specific settings are narrower than requirements. |
| Model parameter configuration | 2.3 Model Parameters | Must | Partial | ALT-014,ALT-019 | `max_tokens` is represented. `temperature`, `reasoning_effort`, and `cache_prompts` are not fully modeled or passed through. |
| Hierarchical AGENT.md handling | 3. System Prompt & Context | Must | Partial | ALT-014,ALT-020 | Hierarchical `.tengu/TENGU.md` loading exists with CLI overrides. Requirement names `AGENT.md`; compatibility and documentation need alignment. |
| Built-in tools and definitions | 4.1 Built-in Tools | Must | Complete | ALT-014,ALT-016 | Read, Edit, Write, Bash/Shell, Grep, Glob/ListFiles, WebFetch, WebSearch, SubAgent, and ParallelAgents are implemented with generated tool definitions. |
| Permissions policy and matching | 4.2 Permission Control | Must | Partial | ALT-014,ALT-017 | Allow/deny and approval basics exist. Regex and negation pattern semantics are missing, and CLI `--allowed-tools` is not connected. |
| Sandbox levels and path control | 4.3 Sandbox | Must | Partial | ALT-014,ALT-017,ALT-023 | Basic sandbox modes and path limits exist. Behavior needs stronger verification and security defaults for secrets such as `.env`. |
| MCP servers and tool discovery | 5. MCP Integration | Must | Complete | ALT-014 | STDIO and HTTP/SSE tool discovery, config persistence, and CLI listing are implemented at the current requirement level. |
| Hooks and automation | 6. Hooks & Automation | Must | Gap | ALT-014,ALT-018 | No hook config model, hook executor, lifecycle hook, or pre/post tool integration is present. |
| Slash commands | 7. Slash Commands | Must | Partial | ALT-014,ALT-021,ALT-022 | Many required and extra TUI slash commands exist, including session, status, tools, MCP, diff, commit, PR, editor, help, and exit. Behavior should be covered by integration/E2E tests. |
| Custom commands | 7.2 Custom Commands | Optional | Complete | ALT-014 | Project and user markdown command expansion exists for `/project:<name>` and `/<name>`, including frontmatter stripping and argument placeholders. |
| Custom agents | 8. Custom Agents | Must | Partial | ALT-014,ALT-019 | Agent list/create/remove/generate and `--agent` prompt injection exist. Full requirement fields such as tools, resources, MCP servers, and hooks are not modeled. |
| File references and completion | 9. File Operations | Must | Partial | ALT-014,ALT-021 | Basic file and image attachment paths exist. TAB completion, fuzzy matching, recent file history, and `.gitignore` handling need implementation evidence. |
| Image inputs | 9.2 Image Files | Must | Complete | ALT-014 | Headless `--image`, TUI `/image`, dropped image-path detection, base64 payload loading, and PNG/JPEG/GIF/WebP media detection are implemented. |
| Git integration | 10. Git Integration | Must | Partial | ALT-014,ALT-022 | TUI `/diff`, `/commit`, `/pr`, and review diff prompts exist. Git history analysis and integration/E2E coverage remain incomplete. |
| GitHub/GitLab integration | 10.2 GitHub/GitLab Integration | Optional | Partial | ALT-014,ALT-022 | TUI `gh pr create` and `gh pr view --comments` helpers exist. Issue/comment/label management is not fully implemented. |
| Config files | 11. Configuration | Must | Partial | ALT-014,ALT-019,ALT-020 | TOML config loading, defaults, env expansion, and TUI `/config` exist. Schema does not cover all documented sections. |
| Authentication | 12. Authentication | Must | Partial | ALT-014,ALT-023 | API-key env status and session recording exist. OAuth and encrypted token storage are not implemented. |
| TUI and rendering | 13. Output/UI | Must | Complete | ALT-014 | TUI layout, multiline input, status display, streaming, Markdown rendering, highlighting, progress, and cancellation paths exist. |
| Advanced features | 14. Optional Features | Optional | Partial | ALT-014 | Review mode, memory scaffold, usage aggregation, and cloud LLM inference through providers exist. Checkpoints and knowledge base are not implemented. |
| Implementation phases | 15. Phases | Must | Partial | ALT-014,ALT-027 | Product task list exists and now includes gap-closure tasks. Completion claims need final realignment after Phase D. |
| Recommended stack | 16. Tech Stack | Should | Complete | ALT-014 | Rust, clap, ratatui, crossterm, reqwest, tokio, serde, toml, and related architecture layers are in place. |
| Testing requirements | 17. Testing | Must | Partial | ALT-014,ALT-022,ALT-026 | `cargo test` passes 44 tests. Coverage target, integration tests, and E2E tests require more evidence. |
| Documentation requirements | 18. Documentation | Must | Gap | ALT-014,ALT-025 | README and contributor docs exist. `USAGE.md`, `CONFIGURATION.md`, and `MCP_GUIDE.md` are missing. |
| Performance requirements | 19. Performance | Must | Gap | ALT-014,ALT-024 | No startup, command latency, file-read latency, memory, or streaming-start measurement evidence is recorded. |
| Security requirements | 20. Security | Must | Partial | ALT-014,ALT-017,ALT-018,ALT-023 | Approval and sandbox basics exist. Default `.env` denial, secret redaction, encrypted token storage, dangerous-command warnings, and audit logs need implementation. |
| References and glossary | 21. References | Should | Complete | ALT-014 | References and glossary are present in `docs/REQUIREMENTS.md`; no runtime work is required. |
