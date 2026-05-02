# Claude Code / Codex Parity Report

作成日: 2026-05-03

## 判定

Tengu は、ローカルで動作するコーディングエージェント CLI として、Claude Code / Codex に近い主要な実用導線を満たしている。

判定は **実用同等に到達、完全同等ではない** とする。

この判定は次を前提にする。

- LLM 推論は Anthropic / OpenAI / Google / Ollama のバックエンドへ委譲する
- ツール実行、ファイル操作、セッション管理、MCP、Git 補助はローカルで行う
- 外部サービスへの実接続や完全な対話端末 E2E は、環境依存の証跡として別管理する

## 達成済み

| 領域 | 判定 | 根拠 |
| ---- | ---- | ---- |
| CLI / TUI 基本導線 | 達成 | TUI、headless `-p`、`json` / `stream-json`、セッション再開を実装済み |
| エージェントループ | 達成 | LLM tool_use / function calling、ToolUse / ToolResult の反復、会話履歴保持を実装済み |
| LLM バックエンド | 達成 | Anthropic、OpenAI、Google、Ollama の抽象化とモデル選択を実装済み |
| ローカルツール | 達成 | Read、Edit、Write、Bash、Grep、Glob、ListFiles、WebFetch、WebSearch を実装済み |
| 権限制御と安全既定 | 達成 | allow / deny、regex、否定パターン、`.env` 既定拒否、危険コマンド拒否、監査ログを実装済み |
| プロジェクト文脈 | 達成 | 階層的 `.tengu/AGENT.md` と legacy `.tengu/TENGU.md`、構造化 project memory を実装済み |
| ファイル参照 UX | 達成 | TUI の `@` 補完、fuzzy 検索、recent files、`.gitignore` 考慮を実装済み |
| Git / ホスティング補助 | 達成 | review、diff、commit、GitHub/GitLab issue / PR / label / comment / review の導線を実装済み |
| 認証保護 | 達成 | API key 状態確認、暗号化 token store、login/logout/status を実装済み |
| 状態復元 | 達成 | セッション永続化、fork、save/load、checkpoint、rollback を実装済み |
| 回帰証跡 | 達成 | 主要 CLI E2E、long-session roundtrip、大規模 repo file tools、core coverage 80% 証跡を追加済み |

## 残る差分

| 領域 | 状態 | 扱い |
| ---- | ---- | ---- |
| 完全な対話端末 E2E | 未完 | 擬似端末での TUI 入力・描画・中断まで含む証跡は今後の品質強化として残す |
| 外部サービス実接続 E2E | 未完 | `gh` / `glab`、各 LLM API、MCP 実サーバーの live 検証は環境依存のため別手順で扱う |
| full coverage 80% | 未完 | core coverage は 80% を達成済み。interactive TUI と network adapter を含む full coverage は継続改善対象 |
| セマンティック KB | 部分 | project memory の永続保存と全文検索は実装済み。PDF indexing や embedding 検索は optional future work |
| ベンダー固有 UI 完全再現 | 対象外 | Claude Code / Codex の私的実装や hosted workflow の完全再現は目標外 |

## 検証証跡

ALT-036 時点の主な検証は次の通り。

```text
cargo test
322 unit tests passed
5 e2e tests passed

cargo clippy -- -D warnings
passed

markdownlint-cli2 README.md TASK.md docs/TASK.md docs/REQUIREMENTS_MATRIX.md docs/REQUIREMENTS_GAP_ANALYSIS.md docs/PARITY_REPORT.md
0 errors
```

関連する追加証跡:

- ALT-028: 主要 CLI 導線の binary-level E2E
- ALT-029: 固定 toolchain / matching LLVM tools で core coverage 81.12%
- ALT-030: TUI file reference completion と `.gitignore` 対応
- ALT-031: GitHub/GitLab issue / PR / label / comment / review 導線
- ALT-032: 暗号化 token store
- ALT-033: checkpoint / rollback / diff restore
- ALT-034: structured project memory と system prompt 挿入
- ALT-035: long-session persistence と large-repository file tools regression

## 最終判断

Phase H の目的である「Claude Code / Codex と同等の実用感・信頼性に近づける」は達成済みと判断する。

ただし、同等性の範囲は **ローカル CLI エージェントとしての主要ワークフロー** に限定する。完全な外部サービス互換、完全な対話端末 E2E、全コード領域 80% coverage、セマンティック knowledge base は、製品強化または optional future work として残す。

## 次の推奨

1. 擬似端末ベースの TUI E2E を追加する
2. live 環境向けの opt-in integration test profile を分離する
3. full coverage の未測定領域を interactive TUI / network adapter ごとに縮小する
4. 必要になった時点で embedding-based knowledge base を別タスク化する
