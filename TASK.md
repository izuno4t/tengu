# 実行計画: Tengu コーディングエージェント実用化

## 概要

Tengu を参考実装 (co-vibe) と同等の実用コーディングエージェントに仕上げる。
Phase A-E は基盤構築、Phase F は実用化のための追加修正。
Phase G は `docs/REQUIREMENTS.md` に対する不足を埋める。

## Phase A: メッセージモデルの刷新（G-002, G-003） ✅

| ID | Status | Task | Details |
| ---- | ------ | ---- | ------- |
| T-001 | ✅ | Message 型の定義 | role(user/assistant), content blocks (Text/ToolUse/ToolResult) を定義 |
| T-002 | ✅ | ChatRequest/ChatResponse の導入 | messages: `Vec<Message>` + tools: `Vec<ToolDefinition>` + system prompt |
| T-003 | ✅ | Anthropic バックエンドに chat メソッド追加 | tool definitions 送信、tool_use レスポンス解析 |
| T-004 | ✅ | OpenAI バックエンドに chat メソッド追加 | tool→function calling 変換 |
| T-005 | ✅ | Google バックエンドに chat メソッド追加 | tool→functionDeclarations 変換 |

## Phase B: エージェントループの再構築（G-001, G-007） ✅

| ID | Status | Task | Details |
| ---- | ------ | ---- | ------- |
| T-006 | ✅ | AgentRunner に run_agent_loop 実装 | LLM→ToolUse→Result→LLM... を end_turn まで繰り返す（最大50ターン） |
| T-007 | ✅ | ToolResult を ContentBlock::ToolResult として会話に追加 | execute_from_json → (result_text, is_error) |
| T-008 | ✅ | Write/Edit ツールで実際にファイル書き込み | execute_from_json が直接ファイルを操作する |
| T-009 | ✅ | ToolEventHandler でUIにツール実行を通知 | Text/ToolCall/ToolResult イベント |

## Phase C: ツールの強化（G-004, G-005, G-006） ✅

| ID | Status | Task | Details |
| ---- | ------ | ---- | ------- |
| T-010 | ✅ | Edit ツールの追加 | old_string→new_string の部分置換。一意性チェック付き |
| T-011 | ✅ | Bash ツールの強化 | sh -c 実行、stdout+stderr統合、タイムアウト、exit code返却 |
| T-012 | ✅ | Grep の正規表現対応 | regex crate を使用。hidden/target/node_modules 自動除外 |
| T-013 | ✅ | ツール定義(JSON Schema)の生成 | builtin_tool_definitions() で全7ツールの定義を提供 |

## Phase D: TUI/CLIの統合（G-008） ✅

| ID | Status | Task | Details |
| ---- | ------ | ---- | ------- |
| T-014 | ✅ | TUI でのエージェントループ統合 | ToolEventHandler経由でToolCall/ToolResult表示 |
| T-015 | ✅ | CLI ヘッドレスモードでのループ統合 | run_prompt で tool_use 対応、system_prompt 反映 |
| T-016 | ✅ | セッション履歴にメッセージ配列を保存 | Session に messages: `Vec<Value>` フィールド追加 |

## Phase E: 品質向上 ✅

| ID | Status | Task | Details |
| ---- | ------ | ---- | ------- |
| T-017 | ✅ | ビルド確認・テスト | cargo build 警告ゼロ、cargo test 45テスト全通過 |
| T-018 | ✅ | ドキュメント最終更新 | ARCHITECTURE.md 刷新、GAP_ANALYSIS.md 新規作成、README.md 更新 |
| T-019 | ✅ | clippy / 警告解消 | 全 warning 解消済み |

## Phase F: 実用化修正 ✅

20件の critical issues を修正し、co-vibe と同等の実用レベルに引き上げる。

| ID | Status | Task | Details |
| ---- | ------ | ---- | ------- |
| T-020 | ✅ | TUI システムプロンプト設定 | execute_tui() で resolve_system_prompt → runner.set_system_prompt() |
| T-021 | ✅ | デフォルトシステムプロンプト | ツール一覧・ガイドラインを含む包括的なシステムプロンプト |
| T-022 | ✅ | ToolEventHandler 競合修正 | async タスク spawn 前に handler を設定 |
| T-023 | ✅ | 会話コンテキスト修正 | フラット文字列 → 構造化 Message[] による会話履歴 |
| T-024 | ✅ | 永続会話履歴 | AgentRunner に conversation_messages を保持、ターン間で tool_use/result を保持 |
| T-025 | ✅ | stream-json モード修正 | レガシー API → agent loop に切り替え、ToolEvent を JSON 出力 |
| T-026 | ✅ | Bash タイムアウト修正 | 二重 spawn 排除、polling ベースのタイムアウト実装 |
| T-027 | ✅ | Read ツール offset/limit | 大きなファイルの部分読み取り対応 |
| T-028 | ✅ | ListFiles ツール追加 | ディレクトリ内容一覧（7番目のビルトインツール） |
| T-029 | ✅ | Usage 情報の伝播 | agent loop → ToolEvent::Usage → TUI/CLI に伝播 |
| T-030 | ✅ | API リトライ | 429/500/502/503 エラーでの指数バックオフリトライ（最大3回） |
| T-031 | ✅ | コンテキストウィンドウ管理 | MAX_CONTEXT_MESSAGES (100) でメッセージ数を制限 |
| T-032 | ✅ | Anthropic プロンプトキャッシュ | anthropic-beta ヘッダー + cache_control ephemeral |
| T-033 | ✅ | max_tokens 増加 | 8192 → 16384 |
| T-034 | ✅ | CLI ヘッドレス tool event 出力 | verbose モードでツール実行を stderr に表示 |
| T-035 | ✅ | ビルド・テスト・clippy 確認 | 警告ゼロ、45テスト全通過 |

## Phase G: Requirements Gap Closure

| ID | Status | Summary | DependsOn |
| ---- | ------ | ------- | --------- |
| ALT-014 | ✅ | 要求差分マトリクスを現行コード基準で更新する | ALT-013 |
| ALT-015 | ✅ | CLI引数の未接続項目を実行経路へ反映する | ALT-014 |
| ALT-016 | ✅ | WebFetchとWebSearchツールを追加する | ALT-014 |
| ALT-017 | ✅ | パーミッション仕様をregexと否定対応に拡張する | ALT-015 |
| ALT-018 | ✅ | フック設定とpre/post実行基盤を実装する | ALT-017 |
| ALT-019 | ✅ | 設定スキーマを要求項目まで拡張する | ALT-014 |
| ALT-020 | ✅ | AGENT.md/TENGU.mdの互換方針を実装する | ALT-019 |
| ALT-021 | ✅ | セッション再開と一覧操作の対話導線を補強する | ALT-014 |
| ALT-022 | ✅ | Git/PR/レビュー導線の統合テストを追加する | ALT-021 |
| ALT-023 | ✅ | セキュリティ既定拒否と監査ログを実装する | ALT-017,ALT-018 |
| ALT-024 | ✅ | 性能計測コマンドと基準値検証を追加する | ALT-022 |
| ALT-025 | ✅ | 必須ドキュメント三点を作成する | ALT-016,ALT-023 |
| ALT-026 | ✅ | カバレッジ計測と不足テストを追加する | ALT-022,ALT-024 |
| ALT-027 | ✅ | REQUIREMENTS/TASK/READMEの完了判定を再整合する | ALT-025,ALT-026 |

## 変更サマリ

### Phase F 変更ファイル

| ファイル | 変更内容 |
| -------- | -------- |
| `src/cli.rs` | TUI system prompt設定、default_system_prompt()、stream-json agent loop化、headless tool event |
| `src/agent/agent.rs` | conversation_messages保持、Usage event、リトライ、コンテキスト制限、max_tokens増加 |
| `src/tools/tools.rs` | Read offset/limit、ListFiles追加、ToolInput更新 |
| `src/tui/controller.rs` | ToolEventHandler競合修正、Message[]会話履歴、Usage event |
| `src/llm/anthropic.rs` | prompt caching (beta header + cache_control) |
