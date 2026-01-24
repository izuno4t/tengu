# Repository Guidelines

## Project Structure & Module Organization
- `src/` contains Rust sources. Key modules: `agent/`, `cli.rs`, `config/`, `llm/`, `mcp/`, `session/`, `tools/`, `tui/`, `main.rs`.
- `docs/` holds requirement and planning documents.
- Root files: `README.md`, `CONTRIBUTING.md`, `LICENSE`.
Example: add new tool logic under `src/tools/` and wire it in `src/cli.rs`.

## Build, Test, and Development Commands
- `cargo build` — compile the project.
- `cargo build --release` — optimized build (used for release binaries).
- `cargo run` — run the CLI locally.
- `cargo test` — run all tests.
- `cargo test <test_name>` — run a specific test.
- `cargo clippy` — lint and follow Rust best practices.
- `cargo fmt` — format code.
- `cargo tarpaulin --out Html` — coverage report (if installed).
- `cargo doc --open` — build and open API docs.

## Coding Style & Naming Conventions
- Rust 2021 edition; follow `cargo fmt` and `cargo clippy` output.
- Add doc comments for all public functions.
- Prefer clear module boundaries aligned with `src/` subdirectories.
- Example naming: `snake_case` for functions, `CamelCase` for types.

## Testing Guidelines
- Use `cargo test` for unit tests; place tests in module `tests` blocks.
- Keep tests small and focused on one behavior.
- Coverage: use `cargo tarpaulin` when verifying overall coverage.

## Commit & Pull Request Guidelines
- Git history is currently short and uses descriptive sentences (e.g., “Add README…”).
- Contribution guide recommends Conventional Commits:
  - `feat:`, `fix:`, `docs:`, `refactor:`, `test:`, `chore:`
- PRs should include a clear summary, link related Issues, and update docs/tests when needed.
- Suggested branch naming: `feature/<short-name>` (example: `feature/mcp-hooks`).

## Configuration & Security Tips
- Store API keys in environment variables (e.g., `OPENAI_API_KEY`).
- Use project context via `AGENT.md` files as described in `README.md`.

## Task-Based Work
- Use `docs/TASK.md` as the source of truth when executing tasks.
- If you work in a task-based flow, update task statuses in `docs/TASK.md`.
