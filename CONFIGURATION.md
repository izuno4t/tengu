# Tengu Configuration Guide

Tengu は `~/.tengu/config.toml` と `./.tengu/config.toml` を読む。プロジェクト設定はローカル実行時のカレントディレクトリにある `./.tengu/config.toml` を使う。

## 最小設定

```toml
[model]
provider = "anthropic"
default = "claude-sonnet-4-20250514"
```

## モデル設定

```toml
[model]
provider = "anthropic"
default = "claude-sonnet-4-20250514"
max_tokens = 16384
temperature = 0.2
reasoning_effort = "medium"
cache_prompts = true

[model.parameters]
max_tokens = 16384
temperature = 0.2
reasoning_effort = "medium"
cache_prompts = true
```

`model.max_tokens` と `model.parameters.max_tokens` は実行経路に反映される。その他の provider 固有パラメータは、現在は設定として読み込めるが provider ごとの送信対応は段階的に追加する。

## Provider設定

```toml
[model.anthropic]
api_key_env = "ANTHROPIC_API_KEY"
base_url = "https://api.anthropic.com"

[model.openai]
api_key_env = "OPENAI_API_KEY"
base_url = "https://api.openai.com"
organization = "org_xxx"
project = "proj_xxx"

[model.google]
api_key_env = "GOOGLE_API_KEY"

[model.local]
base_url = "http://localhost:11434"
```

`base_url` は Anthropic、OpenAI、Google、Local の backend 初期化へ反映される。Local は `OLLAMA_BASE_URL` または `model.local.base_url` を使える。

## 権限設定

```toml
[permissions]
approval_policy = "on-request"
allowed_tools = [
  "Read",
  "Write",
  "Bash(cargo *)",
  "!Read(.env*)",
]
deny = [
  "Bash(git push*)",
]
```

### approval_policy

- `always`: ツール実行前に承認を要求する
- `read-only`: `Write`、`Edit`、`Bash`、`Shell` を拒否する
- `on-request` / `auto`: 既存の allow / deny ルールと sandbox に従う

### ルール構文

- `Read`: ツール名一致
- `Bash(git *)`: ツール名と対象パターン一致
- `Bash(git (status|log|diff))`: 正規表現一致
- `!Read(.env*)`: 許可リスト内の除外、または拒否リスト内の例外

## Sandbox設定

```toml
[sandbox]
mode = "workspace-write"
allowed_paths = ["./src", "./docs"]
blocked_paths = ["./.env", "./secrets"]
```

### mode

- `none`: sandbox 制限を追加しない
- `read-only`: 書き込み系と shell 系を拒否する
- `workspace-write`: workspace 外への書き込みを拒否し、shell 系を拒否する

`allowed_paths` がある場合、対象パスはその範囲に限定される。`blocked_paths` は常に拒否される。

## Security設定

```toml
[security]
audit_log = ".tengu/audit.jsonl"
audit_enabled = true
allow_env_files = false
blocked_paths = ["secrets/**", "private/**"]
```

`.env` と `.env.*` はデフォルトで拒否される。`allow_env_files = true` は意図的に必要なプロジェクトだけで使う。

`audit_log` を指定すると、ツール実行の成功、失敗、拒否を JSON Lines で記録する。監査ログでは Write 内容、Bash コマンド、WebFetch body などの機密化しやすい入力は redacted として保存される。

## Hooks設定

```toml
[[hooks.preToolUse]]
matcher = "Bash(git *)"
command = "echo pre:$tool"
timeout_ms = 5000
on_error = "fail"

[[hooks.postToolUse]]
matcher = "Write(*)"
command = "echo post:$file"
timeout_ms = 5000
on_error = "warn"
```

`matcher` は permission rule と同じ形式でツール名や対象に一致する。hook には `tool`、`file`、`input`、`output` 環境変数が渡され、stdin にはツール入力 JSON が渡される。

`on_error` は以下を指定できる。

- `fail`: hook 失敗をツール失敗として扱う
- `warn`: stderr に警告を出して続行する
- `ignore`: 無視して続行する

## Auth設定

```toml
[auth]
anthropic_api_key_env = "ANTHROPIC_API_KEY"
openai_api_key_env = "OPENAI_API_KEY"
google_api_key_env = "GOOGLE_API_KEY"
token_store = "~/.tengu/auth/tokens.json"
session_path = "~/.tengu/auth/session.json"
oauth_enabled = false
```

現在の認証は環境変数ベースが中心で、OAuth と暗号化トークン保存は未完了である。

## Project Context

Tengu は以下の順序で system prompt 用の context を読む。

1. `~/.tengu/AGENT.md`
2. `~/.tengu/TENGU.md`
3. `./.tengu/AGENT.md`
4. `./.tengu/TENGU.md`
5. `./workspace/.tengu/AGENT.md`
6. `./workspace/.tengu/TENGU.md`

明示指定の `--system-prompt-file` と `--system-prompt` は階層読み込みより優先される。`--append-system-prompt-file` と `--append-system-prompt` は末尾に追加される。

## Theme設定

TUI の色は `~/.tengu/theme.toml` で上書きできる。

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

未指定キーは組み込みテーマへフォールバックする。

## 環境変数展開

設定値内の `$NAME` と `${NAME}` は読み込み時に展開される。未定義の環境変数は元の文字列のまま残る。
