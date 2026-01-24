# Requirements Matrix

| Requirement | Section | Priority | MappedTasks | Notes |
|---|---|---|---|---|
| Core CLI modes and formats | 1. Core Functional Requirements | Must | TASK-008,TASK-009,TASK-010 | Interactive REPL, headless mode, output formats |
| Session persistence and commands | 1.2 Session Management | Must | TASK-007 | Resume, list, save/load, fork |
| Provider abstraction and model switching | 2. LLM Model Management | Must | TASK-004 | OpenAI/Anthropic/Google/Local, per-session switch |
| Model parameter configuration | 2.3 Model Parameters | Must | TASK-003,TASK-004 | max_tokens, temperature, reasoning_effort |
| Hierarchical AGENT.md handling | 3. System Prompt & Context | Must | TASK-011 | Layered priority and CLI overrides |
| Built-in tools and definitions | 4.1 Built-in Tools | Must | TASK-005 | Read/Write/Shell/Search/Web |
| Permissions policy and matching | 4.2 Permission Control | Must | TASK-006 | allow/deny, glob/regex/negation |
| Sandbox levels and path control | 4.3 Sandbox | Must | TASK-006 | none/read-only/workspace/full |
| MCP servers and tool discovery | 5. MCP Integration | Must | TASK-012 | STDIO/HTTP, tool schema parsing |
| Hooks and automation | 6. Hooks & Automation | Must | TASK-013 | pre/post tool hooks, env vars |
| Slash commands | 7. Slash Commands | Must | TASK-014 | session, status, tools, git, help |
| Custom agents | 8. Custom Agents | Must | TASK-017 | agent definitions, switching |
| File references and completion | 9. File Operations | Must | TASK-005,TASK-008 | @ syntax, glob, fuzzy completion |
| Image inputs | 9.2 Image Files | Must | TASK-008,TASK-009 | file/clipboard/drag-drop |
| Git integration | 10. Git Integration | Must | TASK-015 | diff, commit, PR support |
| Config files | 11. Configuration | Must | TASK-003 | TOML config, defaults |
| Authentication | 12. Authentication | Must | TASK-016 | API keys, OAuth, token store |
| TUI and rendering | 13. Output/UI | Must | TASK-008 | TUI layout, highlighting, streaming |
| Advanced features | 14. Optional Features | Optional | TASK-017 | memory, checkpoint, KB, cloud, review |
| Implementation phases | 15. Phases | Must | TASK-001 | coverage tracking for MVP→Enterprise |
| Recommended stack | 16. Tech Stack | Should | TASK-002 | architecture guidance |
| Testing requirements | 17. Testing | Must | TASK-019 | unit/integration/e2e |
| Documentation requirements | 18. Documentation | Must | TASK-020 | README/USAGE/CONFIG/MCP_GUIDE |
| Performance requirements | 19. Performance | Must | TASK-018 | startup/latency/memory targets |
| Security requirements | 20. Security | Must | TASK-006,TASK-016 | secrets, approvals, audit logs |
| References and glossary | 21. References | Should | TASK-001 | traceability only |
