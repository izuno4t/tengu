# TASKS

## Task List

### Phase 0: 完了済み基盤

| ID | Status | Summary | DependsOn |
|----|--------|---------|-----------|
| TASK-001 | ✅ | 要件対応マトリクスを作成する | - |
| TASK-002 | ✅ | アーキテクチャ概要ドキュメントを作成する | TASK-001 |
| TASK-003 | ✅ | 設定ローダーとデフォルト値実装を作成する | TASK-002 |
| TASK-004 | ✅ | モデルプロバイダー抽象化を実装する | TASK-002 |
| TASK-005 | ✅ | ツール実行基盤と内蔵ツール群を実装する | TASK-002 |

### Phase 1: 起動・基本動作の確認

| ID | Status | Summary | DependsOn |
|----|--------|---------|-----------|
| TASK-006 | ✅ | 起動とワンショットの動作確認を完了する | TASK-003,TASK-004,TASK-005 |
| TASK-011 | ✅ | 出力フォーマットの実装と確認を完了する | TASK-006 |
| TASK-012 | ✅ | システムプロンプト階層制御の確認を完了する | TASK-003,TASK-006 |

### Phase 2: LLM・ファイル・セッションの確認

| ID | Status | Summary | DependsOn |
|----|--------|---------|-----------|
| TASK-007 | ✅ | LLM最小問い合わせの実装と確認を完了する | TASK-004,TASK-006 |
| TASK-008 | ✅ | ファイル系ツールの確認を完了する | TASK-005,TASK-006 |
| TASK-009 | ✅ | セッション永続化と再開の確認を完了する | TASK-003,TASK-006 |
| TASK-024 | ✅ | エージェント実行ループの最小動作を完了する | TASK-004,TASK-005,TASK-006 |
| TASK-010 | ✅ | TUI基本操作の確認を完了する | TASK-009 |
| TASK-025 | ✅ | 対話REPLでLLM問い合わせを完了する | TASK-007,TASK-006 |
| TASK-026 | ✅ | セッション索引を廃止して一覧生成に切替える | TASK-009 |
| TASK-027 | ✅ | エージェントにツール実行連携を実装する | TASK-024,TASK-005 |
| TASK-028 | ✅ | 計画→実行ループの最小実装を完了する | TASK-024 |
| TASK-029 | ✅ | ツール選択ロジックの最小実装を完了する | TASK-027 |
| TASK-030 | ✅ | 変更差分の提示と適用を完了する | TASK-027 |
| TASK-031 | ✅ | 失敗時の再試行とエラー共有を完了する | TASK-028 |

### Phase 3: UI・権限・連携の確認

| ID | Status | Summary | DependsOn |
|----|--------|---------|-----------|
| TASK-013 | ✅ | パーミッションとサンドボックスの確認を完了する | TASK-003,TASK-005,TASK-006 |
| TASK-014 | ✅ | MCP統合の確認を完了する | TASK-003,TASK-005,TASK-006 |
| TASK-032 | ✅ | MCPプロトコル仕様メモを作成する | - |
| TASK-033 | ✅ | MCP設定ファイルmcp.tomlの読み書きを実装する | TASK-032 |
| TASK-034 | ✅ | MCPサーバー管理コマンドの永続化を実装する | TASK-033 |
| TASK-035 | ✅ | STDIO MCP接続とツール検出を実装する | TASK-033 |
| TASK-036 | ✅ | HTTP/SSE MCP接続とツール検出を実装する | TASK-033 |
| TASK-037 | ✅ | MCPツール統合をCLIとTUIに反映する | TASK-034,TASK-035,TASK-036 |
| TASK-015 | ⏳ | フック機構の確認を完了する | TASK-005,TASK-006 |
| TASK-016 | ⏳ | スラッシュコマンドの確認を完了する | TASK-006 |
| TASK-017 | ⏳ | Git統合の確認を完了する | TASK-005,TASK-006 |
| TASK-018 | ⏳ | 認証管理と監査ログの確認を完了する | TASK-003,TASK-013 |

### Phase 4: 品質・ドキュメント

| ID | Status | Summary | DependsOn |
|----|--------|---------|-----------|
| TASK-019 | ⏳ | テストスイートの実装と確認を完了する | TASK-003,TASK-004,TASK-005 |
| TASK-020 | ⏳ | 必須ドキュメント一式を作成する | TASK-003,TASK-004,TASK-005 |
| TASK-021 | ⏳ | パフォーマンス計測と閾値検証を完了する | TASK-010,TASK-011 |

### Phase 5: リリース後拡張TODO

| ID | Status | Summary | DependsOn |
|----|--------|---------|-----------|
| TASK-022 | ⏳ | 複数LLM実接続の拡張TODOを整理する | TASK-007 |
| TASK-023 | ⏳ | 高度機能群の拡張TODOを整理する | TASK-014 |

## Task Details (only when clarification needed)

### TASK-022

- Note: リリース向けの複数プロバイダー実接続を後段に回す

### TASK-023

- Note: メモリー/チェックポイント/KB/クラウド/レビューモードを含む

### TASK-007

- Note: LLMは推論/生成のみ、エージェント意思決定はTenguで行う

### TASK-024

- Note: ベンダー固有のエージェント機能には依存しない

### TASK-025

- Note: 対話入力をLLMに送信し応答を表示する

### TASK-027

- Note: LLM応答がJSONのときRead/Write等のツールを実行できるようにする

### TASK-028

- Note: 指示→計画→実行→結果の最小ループを持つ

### TASK-029

- Note: LLM応答から使用ツールを決定できるようにする

### TASK-030

- Note: 書き込み前に差分を提示できるようにする

### TASK-031

- Note: 失敗理由をLLMに渡し再試行できるようにする

### TASK-014

- Note: TASK-032〜TASK-037 の完了で確認完了とする

### TASK-032

- Note: STDIO と HTTP/SSE の仕様を調査して要点をまとめる
- Note: ツール名は名前空間で区別する

### TASK-033

- Note: 設定保存先は .tengu/mcp.toml を使用する

### TASK-035

- Note: ツール参照は @server/tool 形式で扱う

### TASK-036

- Note: ツール参照は @server/tool 形式で扱う

### TASK-037

- Note: CLI と TUI の双方で MCP 一覧/選択を扱う
