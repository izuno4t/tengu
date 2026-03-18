# アーキテクチャ概要

Tengu は複数の LLM プロバイダーを統合するローカル AI コーディングエージェント CLI です。

## 設計思想

- Tengu は**エージェント本体**として意思決定・計画・ツール実行を担う
- LLM は**推論/生成のバックエンド**であり、ツール実行や権限管理は Tengu が担う
- どの LLM を使っても**同一のエージェント挙動**を提供する
- LLM のネイティブ tool_use API を活用し、**自律的なツール実行ループ**を実現する

## モジュール構成

```text
src/
├── main.rs              # エントリーポイント
├── cli.rs               # CLI引数処理・コマンド分岐
├── agent/
│   ├── agent.rs         # AgentRunner: エージェント実行ループ
│   └── store.rs         # カスタムエージェント定義ストア
├── llm/
│   ├── core.rs          # LlmBackend trait, Message/ToolUse 型定義
│   ├── anthropic.rs     # Anthropic (Claude) API統合
│   ├── openai.rs        # OpenAI API統合
│   ├── google.rs        # Google Gemini API統合
│   └── ollama.rs        # ローカル Ollama 統合
├── tools/
│   └── tools.rs         # ビルトインツール(Read/Write/Edit/Bash/Grep/Glob/ListFiles)
├── mcp/
│   ├── types.rs         # MCP型定義
│   ├── stdio.rs         # STDIO MCP接続
│   ├── http.rs          # HTTP/SSE MCP接続
│   └── store.rs         # MCP設定ストア
├── session/
│   └── session.rs       # セッション永続化・会話履歴
├── config/
│   └── config.rs        # TOML設定ファイル管理
├── tui/
│   ├── controller.rs    # TUI入出力制御
│   ├── state.rs         # アプリケーション状態
│   ├── render.rs        # 描画ロジック
│   ├── ansi.rs          # ANSI制御
│   ├── inline.rs        # インライン描画
│   └── theme.rs         # テーマ設定
└── review/
    └── mod.rs           # Git diffベースコードレビュー
```

## エージェント実行ループ（目標アーキテクチャ）

コーディングエージェントの中核は**自律的ツール実行ループ**です。

```text
ユーザー入力
    ↓
┌─────────────────────────────────────┐
│  LLM に会話履歴 + ツール定義を送信  │
│         ↓                           │
│  LLM 応答を受信                     │
│    ├── テキスト応答 → ユーザーに表示 │
│    └── tool_use → ツール実行        │
│              ↓                      │
│         ツール結果を tool_result     │
│         として会話履歴に追加         │
│              ↓                      │
│         LLM に再送信（ループ）      │
└─────────────────────────────────────┘
    ↓ (LLMがend_turnを返すまで繰り返す)
最終応答をユーザーに表示
```

### ループの特徴

1. **複数ツールの連鎖実行**: LLM が必要なだけツールを呼び出せる
2. **会話履歴の蓄積**: 各ターンのメッセージ・ツール結果が履歴に残る
3. **LLM ネイティブ tool_use**: JSON プロンプトハックではなく API の tool_use 機能を使用
4. **承認制御**: 各ツール実行前に権限チェック・ユーザー承認を挟む
5. **ストリーミング**: 応答をリアルタイムでストリーム表示

## メッセージモデル

```text
Message {
    role: user | assistant | tool_result
    content: [
        TextBlock { text }
        ToolUseBlock { id, name, input }
        ToolResultBlock { tool_use_id, content }
    ]
}
```

- LLM API にはメッセージ配列 + ツール定義を送信
- assistant の応答にはテキストと tool_use が混在しうる
- tool_result は次のリクエストで user ロールとして送信

## ツールシステム

### ビルトインツール

| ツール | 機能 | リスクレベル |
|--------|------|-------------|
| Read      | ファイル読み取り（offset/limit対応） | 低 |
| Edit      | 文字列置換による部分編集 | 中 |
| Write     | ファイル全体の書き込み | 中 |
| Bash      | シェルコマンド実行（タイムアウト付き） | 高 |
| Grep      | 正規表現によるコンテンツ検索 | 低 |
| Glob      | ファイルパターンマッチング | 低 |
| ListFiles | ディレクトリ内容一覧 | 低 |

### ツール定義形式

各ツールは LLM API の tool 定義として送信される:

```json
{
  "name": "Read",
  "description": "ファイルの内容を読み取る",
  "input_schema": {
    "type": "object",
    "properties": {
      "path": { "type": "string" }
    },
    "required": ["path"]
  }
}
```

## 権限・サンドボックス

- ツール実行前に ToolPolicy が権限チェック
- approval_policy: always / on-request / read-only / auto
- サンドボックス: none / read-only / workspace-write / full-access
- パターンマッチング: `Read(*.py)`, `Bash(git *)`, `!Write(node_modules/**)`

## セッション管理

- 会話履歴をメッセージ配列として保持
- セッションファイルに永続化（`~/.tengu/sessions/`）
- resume で過去セッションの会話コンテキストを復元
- コンテキストウィンドウ管理（MAX_CONTEXT_MESSAGES=100 で自動トリミング）

## LLM バックエンド抽象化

LlmBackend trait が各プロバイダーの差異を吸収:

- **Anthropic**: Messages API + tool_use（基準実装）
- **OpenAI**: Chat Completions API + function calling
- **Google**: Gemini API + function calling
- **Ollama**: ローカル推論（tool_use 対応はモデル依存）

各バックエンドは共通の Message 型 ↔ プロバイダー固有のリクエスト/レスポンスを変換する。

## エージェントの堅牢性

- **APIリトライ**: 429/500/502/503 エラーで指数バックオフリトライ（最大3回）
- **プロンプトキャッシュ**: Anthropic beta API でシステムプロンプトをキャッシュ
- **コンテキスト制限**: メッセージ数が MAX_CONTEXT_MESSAGES を超えると古いものを切り捨て
- **会話永続化**: AgentRunner が conversation_messages を保持し、ターン間でツール結果を保持
- **デフォルトシステムプロンプト**: ツール一覧・ガイドライン・作業ディレクトリ情報を含む包括的プロンプト
