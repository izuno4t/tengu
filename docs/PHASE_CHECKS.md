# Phase Checks

## TASK-006: 起動とワンショットの動作確認

### Commands

```bash
# TUI mode
cargo run

# One-shot mode
cargo run -- -p "hello"
```

### Expected
- TUI: interactive UI is shown.
- One-shot: prints headless mode message with prompt.

### Status
- Executed:
  - `cargo run` launches TUI.
  - `cargo run -- -p "hello"` prints headless prompt output.
- Notes: build emits unused warnings (expected for scaffolding).

## TASK-011: 出力フォーマットの実装と確認

### Commands

```bash
cargo run -- -p "hello" --output-format json
cargo run -- -p "hello" --output-format stream-json
```

### Expected
- JSON: single JSON object with `type=response`.
- Stream JSON: start/message/end events, one per line.

### Status
- Executed: both formats printed structured JSON.

## TASK-012: システムプロンプト階層制御の確認

### Commands

```bash
mkdir -p /tmp/tengu-check/project/.tengu \
  /tmp/tengu-check/project/workspace/.tengu \
  /tmp/tengu-check-home/.tengu
printf "GLOBAL" > /tmp/tengu-check-home/.tengu/TENGU.md
printf "PROJECT" > /tmp/tengu-check/project/.tengu/TENGU.md
printf "WORKSPACE" > /tmp/tengu-check/project/workspace/.tengu/TENGU.md

env HOME=/tmp/tengu-check-home \
  /Users/izuno/Documents/GitHub/izuno4t/tengu/target/debug/tengu \
  -p "hello" --verbose 2>&1
```

### Expected
- Verbose output lists sources in order: global → project → workspace.
- Prompt length reflects concatenated content.

### Status
- Executed: sources logged in expected order with concatenated length.

## TASK-007: LLM最小問い合わせの実装と確認

### Commands

```bash
cargo run -- -p "hello" --model ollama
```

### Expected
- Headless output prints prompt.
- Ollama response text is printed.

### Status
- Executed: response returned from local Ollama (`こんにちは！`).

## TASK-025: 対話TUIでLLM問い合わせ

### Commands

```bash
cargo run
# input: "hello"
# Ctrl+C to quit
```

### Expected
- TUI入力がOllamaに送信され、応答が表示される。

### Status
- Executed: user confirmed TUI queries return responses.

## TASK-010: TUI基本操作の確認

### Commands

```bash
cargo run
# Ctrl+C to quit
```

### Expected
- TUI is shown.
- Ctrl+C exits cleanly.

### Status
- Executed: TUI launched and exited with q.

## TASK-008: ファイル系ツールの確認

### Commands

```bash
mkdir -p /tmp/tengu-tools
printf "alpha\nbeta\ngamma" > /tmp/tengu-tools/sample.txt

cargo run -- tool read /tmp/tengu-tools/sample.txt
cargo run -- tool grep beta /tmp/tengu-tools/sample.txt
cargo run -- tool glob "*.txt" /tmp/tengu-tools
cargo run -- tool write /tmp/tengu-tools/out.txt "hello-tools"
```

### Expected
- Read outputs file contents.
- Grep returns matching line with line number.
- Glob lists matching paths.
- Write returns status 0 and file is created.

### Status
- Executed: read/grep/glob/write confirmed.

## TASK-009: セッション永続化と再開の確認

### Commands

```bash
cargo run -- new
cargo run -- sessions list
cargo run -- resume --last
cargo run -- sessions delete <session-id>
```

### Expected
- New creates a session and writes files under `~/.tengu/sessions/`.
- List prints session index entries.
- Resume --last resolves the latest session.
- Delete removes the session file and index entry.

### Status
- Executed: new/list/resume/delete confirmed.

## TASK-026: セッション索引の廃止

### Commands

```bash
cargo run -- new
cargo run -- sessions list
cargo run -- resume --last
cargo run -- sessions delete <session-id>
```

### Expected
- No `sessions.db` is created/updated.
- Listing is derived from session JSON files.

### Status
- Executed: list/latest work without index file.

## TASK-027: エージェントにツール実行連携

### Commands

```bash
cargo run -- -p "次のJSONのみ出力してください: {\"tool\":\"write\",\"path\":\"/tmp/tengu-tools/agent.txt\",\"content\":\"ok\"}" --model ollama
```

### Expected
- LLMがJSONを返した場合、ツール実行が行われる。
- `status: 0` が表示され、ファイルが作成される。

### Status
- Executed: tool write executed from LLM JSON response.

## TASK-024: エージェント実行ループの最小動作

### Commands

```bash
cargo run -- -p "hello" --model ollama
```

### Expected
- Tengu側のAgentRunner経由でLLM応答が返る。

### Status
- Executed: AgentRunner経由で応答を返す実装を追加。

### Config Alternative

```toml
# ~/.tengu/config.toml
[model]
backend = "ollama"
name = "llama3"
backend_url = "http://localhost:11434"
```
