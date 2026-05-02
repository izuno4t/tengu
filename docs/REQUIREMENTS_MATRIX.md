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
| Session persistence and commands | 1.2 Session Management | Must | Complete | ALT-014,ALT-021 | File-based sessions, list, save/load, fork, and TUI resume exist. CLI `resume --last` and `resume <session-id>` now open the selected session in the TUI; no-arg resume lists resumable sessions and next commands. |
| Provider abstraction and model switching | 2. LLM Model Management | Must | Complete | ALT-014,ALT-019 | Anthropic, OpenAI, Google, and Ollama backends exist. TUI `/model`, CLI `--model`, provider-specific config schema, and provider base URL wiring exist. |
| Model parameter configuration | 2.3 Model Parameters | Must | Partial | ALT-014,ALT-019 | `max_tokens`, `temperature`, `reasoning_effort`, and `cache_prompts` are modeled. `max_tokens` is wired; other provider-specific request parameters remain future work. |
| Hierarchical AGENT.md handling | 3. System Prompt & Context | Must | Complete | ALT-014,ALT-020 | Hierarchical `.tengu/AGENT.md` and legacy `.tengu/TENGU.md` loading exist with CLI overrides. `/init` creates `AGENT.md` by default while preserving `TENGU.md` compatibility. |
| Built-in tools and definitions | 4.1 Built-in Tools | Must | Complete | ALT-014,ALT-016 | Read, Edit, Write, Bash/Shell, Grep, Glob/ListFiles, WebFetch, WebSearch, SubAgent, and ParallelAgents are implemented with generated tool definitions. |
| Permissions policy and matching | 4.2 Permission Control | Must | Complete | ALT-014,ALT-015,ALT-017 | Allow/deny, approval basics, CLI `--allowed-tools`, regex matching, and negation pattern semantics are implemented. |
| Sandbox levels and path control | 4.3 Sandbox | Must | Partial | ALT-014,ALT-017,ALT-023 | Basic sandbox modes and path limits exist. `.env` / `.env.*` are denied by default and `[security] blocked_paths` can add sensitive path blocks. Broader sandbox behavior still needs more verification. |
| MCP servers and tool discovery | 5. MCP Integration | Must | Complete | ALT-014 | STDIO and HTTP/SSE tool discovery, config persistence, and CLI listing are implemented at the current requirement level. |
| Hooks and automation | 6. Hooks & Automation | Must | Partial | ALT-014,ALT-018 | Hook config model and `preToolUse`/`postToolUse` execution are implemented with matcher, env vars, stdin input, timeout, and `on_error`. Lifecycle hook execution remains future work. |
| Slash commands | 7. Slash Commands | Must | Partial | ALT-014,ALT-021,ALT-022,ALT-028 | Many required and extra TUI slash commands exist, including `/resume --last` and `/resume <session-id>`, status, tools, MCP, diff, commit, PR, editor, help, and exit. Initial binary-level E2E coverage exists; full interactive terminal coverage remains incomplete. |
| Custom commands | 7.2 Custom Commands | Optional | Complete | ALT-014 | Project and user markdown command expansion exists for `/project:<name>` and `/<name>`, including frontmatter stripping and argument placeholders. |
| Custom agents | 8. Custom Agents | Must | Partial | ALT-014,ALT-019 | Agent list/create/remove/generate and `--agent` prompt injection exist. Full requirement fields such as tools, resources, MCP servers, and hooks are not modeled. |
| File references and completion | 9. File Operations | Must | Partial | ALT-014,ALT-021 | Basic file and image attachment paths exist. TAB completion, fuzzy matching, recent file history, and `.gitignore` handling need implementation evidence. |
| Image inputs | 9.2 Image Files | Must | Complete | ALT-014 | Headless `--image`, TUI `/image`, dropped image-path detection, base64 payload loading, and PNG/JPEG/GIF/WebP media detection are implemented. |
| Git integration | 10. Git Integration | Must | Partial | ALT-014,ALT-022 | TUI `/diff`, `/commit`, `/pr`, and review diff prompts exist. Integration-style tests cover real git diff output, local commit execution, PR confirmation args, and review prompt generation; broader git history analysis remains incomplete. |
| GitHub/GitLab integration | 10.2 GitHub/GitLab Integration | Optional | Partial | ALT-014,ALT-022 | TUI `gh pr create` and `gh pr view --comments` helpers exist. PR creation args are covered up to confirmation; live issue/comment/label management is not fully implemented. |
| Config files | 11. Configuration | Must | Complete | ALT-014,ALT-019,ALT-020 | TOML config loading, defaults, env expansion, TUI `/config`, hooks, auth, provider-specific model settings, model parameters, and AGENT.md/TENGU.md compatibility exist. |
| Authentication | 12. Authentication | Must | Partial | ALT-014,ALT-023 | API-key env status and session recording exist. OAuth and encrypted token storage are not implemented. |
| TUI and rendering | 13. Output/UI | Must | Complete | ALT-014 | TUI layout, multiline input, status display, streaming, Markdown rendering, highlighting, progress, and cancellation paths exist. |
| Advanced features | 14. Optional Features | Optional | Partial | ALT-014 | Review mode, memory scaffold, usage aggregation, and cloud LLM inference through providers exist. Checkpoints and knowledge base are not implemented. |
| Implementation phases | 15. Phases | Must | Complete | ALT-014,ALT-027 | Product task lists include the gap-closure work and completion claims are realigned with implemented evidence and remaining gaps. |
| Recommended stack | 16. Tech Stack | Should | Complete | ALT-014 | Rust, clap, ratatui, crossterm, reqwest, tokio, serde, toml, and related architecture layers are in place. |
| Testing requirements | 17. Testing | Must | Partial | ALT-014,ALT-022,ALT-026,ALT-028,ALT-029 | `cargo test` passes the current suite, including integration-style Git/review tests and binary-level E2E tests for auth, sessions, agent, MCP, tools, and perf. Fixed-toolchain core coverage is measured at 81.12% with CI coverage gating; full interactive TUI/E2E coverage still requires more evidence. |
| Documentation requirements | 18. Documentation | Must | Complete | ALT-014,ALT-025 | README, contributor docs, `USAGE.md`, `CONFIGURATION.md`, and `MCP_GUIDE.md` exist. README links to the three required user-facing guides. |
| Performance requirements | 19. Performance | Must | Partial | ALT-014,ALT-024 | `tengu perf` measures local startup-path latency, command dispatch latency, 1MB file-read latency, and RSS memory against configured baselines. LLM streaming-start evidence remains external-provider dependent. |
| Security requirements | 20. Security | Must | Partial | ALT-014,ALT-017,ALT-018,ALT-023 | Approval and sandbox basics, default `.env` denial, configurable sensitive path blocks, filtered child env, dangerous-command blocking, and opt-in JSONL audit logs exist. OAuth/encrypted token storage remain incomplete. |
| References and glossary | 21. References | Should | Complete | ALT-014 | References and glossary are present in `docs/REQUIREMENTS.md`; no runtime work is required. |
