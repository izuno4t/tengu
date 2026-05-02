# Tengu Usage Guide

この文書は Tengu の基本操作と主要コマンドをまとめる。設定の詳細は [CONFIGURATION.md](CONFIGURATION.md)、MCP の運用手順は [MCP_GUIDE.md](MCP_GUIDE.md) を参照する。

## 起動

```bash
# 対話TUIを起動
tengu

# 1回だけプロンプトを実行
tengu -p "Analyze this codebase"

# モデルを指定
tengu --model claude-sonnet-4-20250514 -p "Write tests"
```

## 出力形式

```bash
# 通常テキスト
tengu -p "Summarize this repository" --output-format text

# JSON
tengu -p "Summarize this repository" --output-format json

# ストリーミングJSON
tengu -p "Reply with OK" --output-format stream-json
```

`stream-json` は `start`、`chunk`、`usage`、`tool_call`、`tool_result`、`end` などのイベントを行単位の JSON として出力する。

## ツール許可

```bash
# Read と Write を許可
tengu -p "Fix typo" --allowed-tools "Read,Write"

# cargo 系 Bash だけを許可
tengu -p "Run tests" --allowed-tools "Read,Bash(cargo *)"

# Read から .env 系を除外
tengu -p "Inspect files" --allowed-tools "Read,!Read(.env*)"
```

`.env` と `.env.*` は設定に関係なくデフォルトで拒否される。プロジェクトが明示的に必要とする場合のみ `[security] allow_env_files = true` を設定する。

## セッション操作

```bash
# 新規セッションを作成
tengu new

# セッション一覧
tengu sessions list

# 最新セッションをTUIで再開
tengu resume --last

# セッションIDを指定して再開
tengu resume <session-id>
```

TUI では以下の slash command を使う。

```text
/new
/clear
/resume
/resume --last
/resume <session-id>
/save
/save ./session.json
/load ./session.json
/fork
```

保存済みセッションは会話履歴、表示ログ、キュー、添付画像、usage、承認待ち状態を復元する。

## レビューとGit

```bash
# 作業ツリー差分をレビュー
tengu review

# base...HEAD をレビュー
tengu review --base main

# 観点を指定
tengu review --base main --preset security
```

TUI では以下を使う。

```text
/diff
/diff --stat
/review
/review --base main
/review --preset correctness
/commit <message>
/pr --draft --fill
/pr_comments
```

`/commit` と `/pr` は実行前に `y` / `n` の確認を表示する。

## 画像入力

```bash
# 画像を1つ渡す
tengu --image screenshot.png -p "Describe UI issues"

# 複数画像を渡す
tengu --image before.png,after.png -p "Compare these"
```

画像入力は headless 実行でサポートされる。TUI では `/image <path>` または画像パスのドラッグ入力で次のプロンプトに添付できる。

## カスタムエージェント

```bash
# 一覧
tengu agent list

# ローカル雛形を作成
tengu agent create code-reviewer

# LLMで生成
tengu agent generate

# エージェントを使う
tengu --agent code-reviewer -p "Review this change"
```

プロジェクトエージェントは `./.tengu/agents/` に保存される。

## 認証状態

```bash
tengu auth status
tengu auth login
tengu auth logout
```

API キーは環境変数から読む。主な変数は `ANTHROPIC_API_KEY`、`OPENAI_API_KEY`、`GOOGLE_API_KEY`。

## 性能確認

```bash
# テキストで確認
tengu perf

# JSONで確認し、基準未達なら失敗
tengu perf --format json --strict
```

`perf` は起動経路、軽量コマンド、1MB ファイル読み込み、RSS メモリをローカルで測定する。LLM 応答開始時間は外部プロバイダー依存のため、このローカル計測には含めない。

## カバレッジ

```bash
# 80% の行カバレッジを下限にサマリを出力
scripts/coverage.sh

# HTML レポートを生成
COVERAGE_MIN_LINES=80 scripts/coverage.sh html

# LCOV を target/coverage/lcov.info に出力
scripts/coverage.sh lcov
```

`scripts/coverage.sh` は `cargo llvm-cov` を優先し、利用できない場合は `cargo tarpaulin` を使う。`rustc` と LLVM ツールの配布元が異なる環境では、同じ toolchain の `LLVM_COV` と `LLVM_PROFDATA` を指定する。

## 終了コード

- 正常終了: `0`
- CLI引数エラー、設定エラー、ツール拒否、`--strict` 性能基準未達: 非ゼロ
