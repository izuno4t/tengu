# Claude Code基準 AIコーディングエージェントCLI 要件定義書

## 📋 概要

本ドキュメントは、AIコーディングエージェントのコマンドラインインターフェース（CLI）を構築するための要件定義書です。まずClaude Codeを基準仕様として中核機能を成立させ、そのうえで他ベンダーCLIに見られる機能要件を段階的に追加できる構成で整理します。

### 基準仕様と拡張対象

| ツール          | ベンダー  | 主要モデル               | 本書での扱い                           |
| --------------- | --------- | ------------------------ | -------------------------------------- |
| **Claude Code** | Anthropic | Claude Sonnet/Opus/Haiku | 基準仕様。優先的に追従する対象         |
| **Codex CLI**   | OpenAI    | GPT-5, o3-mini           | Claude Code基準の上に追加検討する拡張要件 |
| **Gemini CLI**  | Google    | Gemini 2.5/3 Pro         | Claude Code基準の上に追加検討する拡張要件 |
| **Kiro CLI**    | AWS       | Claude (Auto mode)       | Claude Code基準の上に追加検討する拡張要件 |

### 要件整理の前提

- 実装順序の第一段階はClaude Codeの操作感、権限制御、ツール実行体験、拡張性を成立させること
- 多ベンダー要件は削除しない。Claude Code基準の中核機能を満たしたあとに拡張要件として積み上げる
- 他CLIに見られる一般的な設計は、Claude Code基準と矛盾しない範囲で統合する
- 挙動解釈に迷いがある場合は、まずClaude Codeに近づく方向を優先し、その後に拡張差分を定義する
- 本書の各要件は「Claude Code基準で成立し、将来的に多ベンダー拡張可能なローカルCLIエージェント」を成立させる観点で定義する

### 本書の読み方

- 各章の「必須機能」「必須要件」は、原則としてClaude Code基準の中核要件として読む
- 他ベンダー固有の機能、Optional、高度機能、将来拡張に関する記述は、Claude Code基準を満たした後に積み上げる拡張要件として読む
- 実装やタスク分解では、各章を「先に基準実装」「後で拡張実装」の順で処理する

### 適用区分マップ

**区分ラベル**

- `Claude Code基準`: 最初に成立させる中核要件
- `基準先行 + 拡張統合`: 先にClaude Code基準で成立させ、その後に多ベンダー差分を統合する要件
- `多ベンダー拡張`: Claude Code基準の実装後に追加する要件

**1. コア機能要件**

- `1.1 実行モード`: Claude Code基準
- `1.1.1 対話モード (Interactive/REPL)`: Claude Code基準
- `1.1.2 非対話モード (Headless/Print mode)`: Claude Code基準
- `1.1.3 出力フォーマット`: 基準先行 + 拡張統合
- `1.2 セッション管理`: Claude Code基準
- `1.2.1 セッション永続化`: Claude Code基準
- `1.2.2 セッション操作`: Claude Code基準

**2. LLMモデル管理要件**

- `2.1 モデルプロバイダー抽象化`: 基準先行 + 拡張統合
- `2.1.1 サポートプロバイダー`: 基準先行 + 拡張統合
- `2.1.2 プロバイダー設定`: 基準先行 + 拡張統合
- `2.2 モデル切り替え`: 基準先行 + 拡張統合
- `2.2.1 セッション内切り替え`: 基準先行 + 拡張統合
- `2.2.2 起動時指定`: 基準先行 + 拡張統合
- `2.3 モデルパラメータ設定`: 基準先行 + 拡張統合

**3. システムプロンプト & コンテキスト管理要件**

- `3.1 プロジェクト設定ファイル`: Claude Code基準
- `3.1.1 階層的設定`: Claude Code基準
- `3.1.2 設定ファイル形式 (Markdown)`: Claude Code基準
- `3.2 システムプロンプト制御`: Claude Code基準
- `3.2.1 コマンドライン引数`: Claude Code基準
- `3.2.2 優先順位`: Claude Code基準
- `コーディング標準`: Claude Code基準
- `アーキテクチャ`: Claude Code基準
- `ファイル構成`: Claude Code基準

**4. ツール & パーミッション管理要件**

- `4.1 ビルトインツール`: Claude Code基準
- `4.1.1 必須ツール一覧`: Claude Code基準
- `4.1.2 ツール定義形式`: Claude Code基準
- `4.2 パーミッション制御`: Claude Code基準
- `4.2.1 承認ポリシー`: Claude Code基準
- `4.2.2 ツール別制御`: Claude Code基準
- `4.2.3 パターンマッチング仕様`: Claude Code基準
- `4.3 サンドボックス機能`: 基準先行 + 拡張統合
- `4.3.1 サンドボックスモード`: 基準先行 + 拡張統合

**5. MCP (Model Context Protocol) 統合要件**

- `5.1 MCP Server管理`: Claude Code基準
- `5.1.1 対応形式`: Claude Code基準
- `5.1.2 設定ファイル`: Claude Code基準
- `5.1.3 MCPコマンド`: Claude Code基準
- `5.2 MCPツール検出`: Claude Code基準

**6. フック & オートメーション要件**

- `6.1 フック種類`: Claude Code基準
- `6.1.1 エージェントライフサイクルフック`: Claude Code基準
- `6.1.2 ツール実行フック`: Claude Code基準
- `6.2 フック実行仕様`: Claude Code基準

**7. スラッシュコマンド要件**

- `7.1 必須ビルトインコマンド`: Claude Code基準
- `7.1.1 セッション管理`: Claude Code基準
- `7.1.2 設定・ステータス`: Claude Code基準
- `7.1.3 Git統合`: Claude Code基準
- `7.1.4 その他`: Claude Code基準
- `7.2 カスタムコマンド`: 多ベンダー拡張
- `7.2.1 定義場所`: 多ベンダー拡張
- `7.2.2 カスタムコマンド形式`: 多ベンダー拡張

**8. カスタムエージェント要件**

- `8.1 エージェント定義`: 基準先行 + 拡張統合
- `8.1.1 設定ファイル構造`: 基準先行 + 拡張統合
- `8.2 エージェント操作`: 基準先行 + 拡張統合
- `8.2.1 CLIコマンド`: 基準先行 + 拡張統合
- `8.2.2 エージェント起動`: 基準先行 + 拡張統合

**9. ファイル操作要件**

- `9.1 ファイル参照`: Claude Code基準
- `9.1.1 @ 記法`: Claude Code基準
- `9.1.2 ファイル補完`: Claude Code基準
- `9.2 画像ファイル対応`: 基準先行 + 拡張統合
- `9.2.1 画像入力`: 基準先行 + 拡張統合
- `9.2.2 サポート形式`: 基準先行 + 拡張統合

**10. Git統合要件**

- `10.1 Git操作`: Claude Code基準
- `10.1.1 必須機能`: Claude Code基準
- `10.1.2 Git履歴活用`: Claude Code基準
- `10.2 GitHub/GitLab統合`: 多ベンダー拡張
- `10.2.1 必須機能（gh/glab CLI使用）`: 多ベンダー拡張

**11. 設定ファイル要件**

- `11.1 設定ファイル形式`: 基準先行 + 拡張統合
- `11.2 設定例`: 基準先行 + 拡張統合

**12. 認証要件**

- `12.1 認証方式`: 基準先行 + 拡張統合
- `12.1.1 API Key認証`: Claude Code基準
- `12.1.2 OAuth認証`: 多ベンダー拡張
- `12.2 セッショントークン管理`: 基準先行 + 拡張統合

**13. 出力・UI要件**

- `13.1 TUI (Terminal User Interface)`: Claude Code基準
- `13.1.1 必須要素`: Claude Code基準
- `13.1.2 推奨フレームワーク`: 基準先行 + 拡張統合
- `13.2 シンタックスハイライト`: Claude Code基準
- `13.3 ストリーミング出力`: Claude Code基準

**14. 高度な機能要件（Optional）**

- `14.1 メモリー機能`: 多ベンダー拡張
- `14.2 チェックポイント機能`: 多ベンダー拡張
- `14.3 ナレッジベース`: 多ベンダー拡張
- `14.4 クラウド実行`: 多ベンダー拡張
- `14.5 レビューモード`: 多ベンダー拡張

**15. 実装優先順位**

- `Phase 1: MVP（Minimum Viable Product）`: Claude Code基準
- `Phase 2: 拡張機能`: 多ベンダー拡張
- `Phase 3: 高度な機能`: 多ベンダー拡張
- `Phase 4: エンタープライズ機能`: 多ベンダー拡張

**16. 技術スタック推奨**

- `16.1 プログラミング言語`: 基準先行 + 拡張統合
- `16.2 主要ライブラリ（Rust）`: Claude Code基準
- `16.3 アーキテクチャ`: Claude Code基準

**17. テスト要件**

- `17.1 ユニットテスト`: Claude Code基準
- `17.2 統合テスト`: 基準先行 + 拡張統合
- `17.3 E2Eテスト`: 基準先行 + 拡張統合

**18. ドキュメント要件**

- `18.1 必須ドキュメント`: 基準先行 + 拡張統合

**19. パフォーマンス要件**

- `19.1 レスポンス時間`: Claude Code基準
- `19.2 メモリ使用量`: Claude Code基準
- `19.3 並行処理`: 基準先行 + 拡張統合

**20. セキュリティ要件**

- `20.1 機密情報の保護`: Claude Code基準
- `20.2 コマンド実行の安全性`: Claude Code基準
- `20.3 監査ログ`: 基準先行 + 拡張統合

**21. 参考資料 / 付録**

- `21.1 公式ドキュメント`: 基準先行 + 拡張統合
- `21.2 仕様・標準`: 基準先行 + 拡張統合
- `21.3 関連ツール`: 多ベンダー拡張
- `付録A: 用語集`: 基準先行 + 拡張統合
- `付録B: サンプル設定ファイル`: 基準先行 + 拡張統合
- `完全な設定例`: 基準先行 + 拡張統合

---

## 1. コア機能要件

この章は、Claude Code基準として最初に成立させる基本操作を定義し、後続のCLI拡張はこの土台の上に追加する。

### 1.1 実行モード
区分: Claude Code基準

#### 1.1.1 対話モード (Interactive/REPL)
区分: Claude Code基準
**必須機能:**
- フルスクリーンTUIによる対話環境
- リアルタイムストリーミング出力
- マルチラインプロンプト入力
- 履歴ナビゲーション（↑↓キー）
- セッション永続化

**実装例:**
```bash
# 基本起動
$ your-agent

# ディレクトリ指定で起動
$ your-agent --add-dir /path/to/project
```

#### 1.1.2 非対話モード (Headless/Print mode)
区分: Claude Code基準
**必須機能:**
- ワンショット実行
- パイプラインサポート
- スクリプト統合
- CI/CD対応

**実装例:**
```bash
# ワンショット実行
$ your-agent -p "プロンプトテキスト"

# パイプ入力
$ cat error.log | your-agent -p "このエラーを解析して"

# CI/CDでの利用
$ your-agent -p "lint実行してエラーがあれば修正" --allowed-tools "Read,Write,Shell(cargo *)"
```

#### 1.1.3 出力フォーマット
区分: 基準先行 + 拡張統合
**必須機能:**
- プレーンテキスト（デフォルト）
- JSON構造化出力
- ストリーミングJSON（`stream-json`）

**`stream-json` イベント仕様:**
- `start`: ストリーム開始
- `chunk`: 生成テキストの差分チャンク
- `tool`: ツール実行結果または適用結果
- `error`: ストリーム中のエラー
- `end`: ストリーム終了

**イベント例:**
```json
{"type":"start","mode":"llm"}
{"type":"chunk","mode":"llm","delta":"Hello"}
{"type":"tool","mode":"tool","content":"status: 0"}
{"type":"end","mode":"llm"}
```

**実装例:**
```bash
# JSON出力
$ your-agent -p "コードベース解析" --output-format json

# ストリーミングJSON
$ your-agent -p "テスト実行" --output-format stream-json
```

### 1.2 セッション管理
区分: Claude Code基準

#### 1.2.1 セッション永続化
区分: Claude Code基準
**必須機能:**
- 自動セッション保存
- セッションID管理
- セッション一覧表示
- セッション再開機能

**データ構造:**
```
~/.your-agent/sessions/
├── {session-id}.json       # セッションデータ
└── sessions.db            # セッションインデックス
```

#### 1.2.2 セッション操作
区分: Claude Code基準
**必須コマンド:**
```bash
# 最新セッションを再開
/resume

# セッション一覧
/sessions list

# セッション保存
/save /path/to/session.json

# セッション読み込み
/load /path/to/session.json

# 新規セッション
/new

# セッション分岐
/fork
```

---

## 2. LLMモデル管理要件

この章は、まずClaude Code基準のモデル選択と実行経路を定義し、多プロバイダー化は互換層としてその上に追加する。

### 2.1 モデルプロバイダー抽象化
区分: 基準先行 + 拡張統合

#### 2.1.1 サポートプロバイダー
区分: 基準先行 + 拡張統合
**必須:**
- OpenAI (GPT-4o, o3-mini, etc.)
- Anthropic (Claude Sonnet, Opus, Haiku)
- Google (Gemini Pro/Ultra)
- ローカルモデル (Ollama, LM Studio)

#### 2.1.2 プロバイダー設定
区分: 基準先行 + 拡張統合
**設定ファイル例 (TOML):**
```toml
[model]
provider = "anthropic"
default = "claude-sonnet-4-20250514"

[model.anthropic]
api_key_env = "ANTHROPIC_API_KEY"
base_url = "https://api.anthropic.com"
max_tokens = 8192

[model.openai]
api_key_env = "OPENAI_API_KEY"
base_url = "https://api.openai.com/v1"

[model.local]
base_url = "http://localhost:1234/v1"
api_key = "not-needed"
```

### 2.2 モデル切り替え
区分: 基準先行 + 拡張統合

#### 2.2.1 セッション内切り替え
区分: 基準先行 + 拡張統合
**必須コマンド:**
```bash
# モデル選択画面
/model

# 直接指定
/model claude-opus-4

# 現行実装では表示のみ
/model
```

#### 2.2.2 起動時指定
区分: 基準先行 + 拡張統合
```bash
$ your-agent --model claude-sonnet-4
$ your-agent --model gpt-4o
```

### 2.3 モデルパラメータ設定
区分: 基準先行 + 拡張統合

**設定可能項目:**
- `max_tokens`: 最大トークン数
- `temperature`: ランダム性
- `reasoning_effort`: 推論努力レベル（対応モデルのみ）
- `cache_prompts`: プロンプトキャッシュ有効化

**設定例:**
```toml
[model.parameters]
max_tokens = 8192
temperature = 0.7
reasoning_effort = "high"  # low/medium/high
```

---

## 3. システムプロンプト & コンテキスト管理要件

この章は、Claude Code基準のコンテキスト合成とプロジェクト指示を先に定義し、他CLI由来の補助設定は拡張として扱う。

### 3.1 プロジェクト設定ファイル
区分: Claude Code基準

#### 3.1.1 階層的設定
区分: Claude Code基準
**ファイル構造:**
```
# グローバル設定
~/.your-agent/AGENT.md

# プロジェクト設定
/project/.your-agent/AGENT.md

# ワークスペース設定（優先度最高）
/project/workspace/.your-agent/AGENT.md
```

#### 3.1.2 設定ファイル形式 (Markdown)
区分: Claude Code基準
**AGENT.md 例:**
```markdown
# プロジェクトコンテキスト

## コーディング標準
- TypeScript使用
- ESLint設定に従う
- Jest でテスト作成
- React では関数コンポーネント + Hooks

## アーキテクチャ
- Frontend: Next.js with TypeScript
- Backend: Node.js with Express
- Database: PostgreSQL with Prisma
- State: Zustand

## ファイル構成
- Components: `src/components/`
- Utilities: `src/utils/`
- Tests: `*.test.ts` 形式
```

### 3.2 システムプロンプト制御
区分: Claude Code基準

#### 3.2.1 コマンドライン引数
区分: Claude Code基準
```bash
# システムプロンプト完全置換
$ your-agent --system-prompt "カスタムプロンプト"

# ファイルから読み込み
$ your-agent --system-prompt-file ./custom-prompt.md

# 既存プロンプトに追加
$ your-agent --append-system-prompt "追加指示"

# ファイルから追加
$ your-agent --append-system-prompt-file ./additions.md
```

#### 3.2.2 優先順位
区分: Claude Code基準
1. `--system-prompt` / `--system-prompt-file` （最高優先）
2. `--append-system-prompt` / `--append-system-prompt-file`
3. ワークスペース設定 (`./.your-agent/AGENT.md`)
4. プロジェクト設定 (`./project/.your-agent/AGENT.md`)
5. グローバル設定 (`~/.your-agent/AGENT.md`)

---

## 4. ツール & パーミッション管理要件

この章は、Claude Code基準の承認付きツール実行を先に満たし、追加ツール種別や高度な制御は段階的拡張として扱う。

### 4.1 ビルトインツール
区分: Claude Code基準

#### 4.1.1 必須ツール一覧
区分: Claude Code基準
| ツール名 | 機能 | リスク |
|---------|------|--------|
| `Read` | ファイル読み込み | 低 |
| `Write` | ファイル書き込み | 中 |
| `Bash`/`Shell` | コマンド実行 | 高 |
| `Grep`/`Search` | ファイル検索 | 低 |
| `Glob`/`FindFiles` | パターン検索 | 低 |
| `WebFetch` | URL取得 | 中 |
| `WebSearch` | Web検索 | 低 |

#### 4.1.2 ツール定義形式
区分: Claude Code基準
```json
{
  "name": "read_file",
  "description": "ファイルの内容を読み込む",
  "parameters": {
    "type": "object",
    "properties": {
      "path": {
        "type": "string",
        "description": "読み込むファイルのパス"
      }
    },
    "required": ["path"]
  }
}
```

### 4.2 パーミッション制御
区分: Claude Code基準

#### 4.2.1 承認ポリシー
区分: Claude Code基準
**設定値:**
- `always`: 常に承認を求める（最安全）
- `on-request`: エージェントが必要と判断した時のみ
- `read-only`: 読み取り専用操作のみ自動承認
- `auto`: 全自動（危険）

**設定例:**
```toml
[permissions]
approval_policy = "on-request"
```

#### 4.2.2 ツール別制御
区分: Claude Code基準
**設定例:**
```json
{
  "permissions": {
    "allowedTools": [
      "Read",
      "Write",
      "Bash(git *)",
      "Bash(npm test)",
      "Bash(npm run build)"
    ],
    "deny": [
      "Read(./.env)",
      "Read(./.env.*)",
      "Write(./production.*)",
      "Bash(rm -rf *)"
    ]
  }
}
```

#### 4.2.3 パターンマッチング仕様
区分: Claude Code基準
**サポート形式:**
- Glob パターン: `*.py`, `src/**/*.ts`
- 正規表現: `Bash(git (status|log|diff))`
- 否定パターン: `!Write(node_modules/**)`

### 4.3 サンドボックス機能
区分: 基準先行 + 拡張統合

#### 4.3.1 サンドボックスモード
区分: 基準先行 + 拡張統合
**レベル:**
- `none`: サンドボックスなし
- `read-only`: 読み取り専用
- `workspace-write`: ワークスペース内のみ書き込み可
- `full-access`: フルアクセス（信頼された環境のみ）

**設定例:**
```toml
[sandbox]
mode = "workspace-write"
allowed_paths = [
  "./src",
  "./tests",
  "./docs"
]
blocked_paths = [
  "./.env",
  "./production.config.*"
]
```

---

## 5. MCP (Model Context Protocol) 統合要件

この章は、Claude Code基準のMCP利用体験を基準とし、接続方式や運用差分の拡張はその上に積み増す。

### 5.1 MCP Server管理
区分: Claude Code基準

#### 5.1.1 対応形式
区分: Claude Code基準
- **STDIO Server**: ローカルプロセスとして起動
- **HTTP Server (SSE)**: リモートサーバーに接続

#### 5.1.2 設定ファイル
区分: Claude Code基準
**TOML形式例:**
```toml
[mcp_servers.filesystem]
command = "npx"
args = ["-y", "@modelcontextprotocol/server-filesystem", "/path/to/files"]
env = {}

[mcp_servers.github]
command = "npx"
args = ["-y", "@modelcontextprotocol/server-github"]
env = { GITHUB_TOKEN = "${GITHUB_TOKEN}" }
timeout_sec = 30

[mcp_servers.remote_api]
url = "https://api.example.com/mcp"
bearer_token_env_var = "API_TOKEN"
http_headers = { "X-Custom-Header" = "value" }
```

#### 5.1.3 MCPコマンド
区分: Claude Code基準
```bash
# MCPサーバー追加
$ your-agent mcp add <name> --command <cmd> --args <args>

# MCPサーバー一覧
$ your-agent mcp list

# MCPサーバー削除
$ your-agent mcp remove <name>

# セッション内でMCP確認
> /mcp
```

### 5.2 MCPツール検出
区分: Claude Code基準

**必須機能:**
- サーバー起動時の自動ツール検出
- ツール一覧の取得
- ツールスキーマの解析
- ツール名の競合解決

**ツール参照形式:**
```
@server_name                    # サーバーの全ツール
@server_name/tool_name          # 特定ツール
```

---

## 6. フック & オートメーション要件

この章は、Claude Code基準で有効な自動化ポイントを先に定義し、ベンダー差分のある自動化は追加拡張として整理する。

### 6.1 フック種類
区分: Claude Code基準

#### 6.1.1 エージェントライフサイクルフック
区分: Claude Code基準
```json
{
  "hooks": {
    "agentSpawn": [
      {
        "command": "git status",
        "timeout_ms": 5000,
        "cache_ttl_seconds": 300
      }
    ],
    "userPromptSubmit": [
      {
        "command": "ls -la"
      }
    ]
  }
}
```

#### 6.1.2 ツール実行フック
区分: Claude Code基準
```json
{
  "hooks": {
    "preToolUse": [
      {
        "matcher": "shell",
        "command": "echo '$(date) - Command:' >> /tmp/audit.log"
      },
      {
        "matcher": "write",
        "command": "git add $file"
      }
    ],
    "postToolUse": [
      {
        "matcher": "Write(*.py)",
        "command": "black $file"
      },
      {
        "matcher": "Write(*.rs)",
        "command": "cargo fmt --all"
      }
    ]
  }
}
```

### 6.2 フック実行仕様
区分: Claude Code基準

**環境変数:**
- `$file`: 操作対象ファイルパス
- `$tool`: ツール名
- `$input`: ツール入力（stdin経由）
- `$output`: ツール出力

**実行制御:**
- `timeout_ms`: タイムアウト（ミリ秒）
- `cache_ttl_seconds`: キャッシュTTL
- `on_error`: エラー時の動作（`ignore` | `warn` | `fail`）

---

## 7. スラッシュコマンド要件

この章は、Claude Code基準の対話操作コマンドを先に揃え、他CLI固有の補助コマンドは追加要件として扱う。

### 7.1 必須ビルトインコマンド
区分: Claude Code基準

#### 7.1.1 セッション管理
区分: Claude Code基準
```
/new              新規セッション開始
/clear            履歴クリア
/resume           セッション再開（ピッカー表示）
/resume --last    最新セッション再開
/fork             現在のセッションを分岐
/save <path>      セッション保存
/load <path>      セッション読み込み
```

#### 7.1.2 設定・ステータス
区分: Claude Code基準
```
/model            モデル選択
/approvals        承認ポリシー変更
/status           現在の設定表示
/tools            ツール一覧表示
/mcp              MCPサーバー一覧
```

#### 7.1.3 Git統合
区分: Claude Code基準
```
/diff             Git差分表示
/commit           コミット作成
/pr               プルリクエスト作成
```

#### 7.1.4 その他
区分: Claude Code基準
```
/help             ヘルプ表示
/exit, /quit      終了
/editor           外部エディタ起動
```

### 7.2 カスタムコマンド
区分: 多ベンダー拡張

#### 7.2.1 定義場所
区分: 多ベンダー拡張
```
~/.your-agent/commands/         # グローバル
./.your-agent/commands/         # プロジェクト
```

#### 7.2.2 カスタムコマンド形式
区分: 多ベンダー拡張
**ファイル: `.your-agent/commands/security-review.md`**
```markdown
---
name: security-review
description: セキュリティレビューを実行
---

以下の手順でセキュリティレビューを実施してください:

1. コードベース全体をスキャン
2. 脆弱性パターンを検出
3. 重大度別にレポート作成
4. 推奨される修正方法を提案
```

**使用例:**
```bash
> /project:security-review
> /project:security-review src/auth.rs
> /security-review
```

**現行実装メモ:**
- `/.tengu/commands/<name>.md` を `/project:<name>` で実行できる
- `~/.tengu/commands/<name>.md` または `./.tengu/commands/<name>.md` を `/<name>` で実行できる
- 先頭の frontmatter は除去し、本文を展開して LLM 入力へ渡す

---

## 8. カスタムエージェント要件

この章は、Claude Code基準のエージェント拡張性を基準にし、将来的なベンダー固有カスタマイズは上位拡張として扱う。

### 8.1 エージェント定義
区分: 基準先行 + 拡張統合

#### 8.1.1 設定ファイル構造
区分: 基準先行 + 拡張統合
**場所:**
```
~/.your-agent/agents/           # グローバルエージェント
./.your-agent/agents/           # ローカルエージェント
```

**設定例: `code-reviewer.json`**
```json
{
  "name": "code-reviewer",
  "description": "コードレビュー専門エージェント",
  "prompt": "あなたはシニアコードレビュアーです。コード品質、セキュリティ、ベストプラクティスに焦点を当ててレビューしてください。",
  "model": "claude-sonnet-4",
  "tools": ["Read", "Grep", "Bash(git *)"],
  "allowedTools": ["Read", "Grep"],
  "toolsSettings": {
    "read": {
      "allowedPaths": ["src/**", "tests/**"]
    }
  },
  "resources": [
    "file://README.md",
    "file://CONTRIBUTING.md"
  ],
  "mcpServers": {
    "git": {
      "command": "git-mcp-server",
      "args": []
    }
  },
  "hooks": {
    "agentSpawn": [
      {"command": "git status"}
    ]
  }
}
```

### 8.2 エージェント操作
区分: 基準先行 + 拡張統合

#### 8.2.1 CLIコマンド
区分: 基準先行 + 拡張統合
```bash
# エージェント一覧
$ your-agent agent list

# エージェント作成（ローカルひな形）
$ your-agent agent create my-agent

# エージェント生成（現在のLLMでJSON作成）
$ your-agent agent generate

# エージェント指定で起動
$ your-agent --agent code-reviewer
```

**現行実装メモ:**
- `tengu agent create <name>` は `./.tengu/agents/<name>.json` にローカルひな形を作成する
- `tengu agent generate` は現在のモデル設定を使って `name/description/prompt` を生成し、ローカルへ保存する
- `--agent <name>` はローカル優先でエージェント定義を読み込み、システムプロンプトへ連結する

#### 8.2.2 エージェント起動
区分: 基準先行 + 拡張統合
```bash
# エージェント指定で起動
$ your-agent --agent code-reviewer

# 別エージェントを指定して起動
$ your-agent --agent debugger
```

---

## 9. ファイル操作要件

この章は、Claude Code基準の安全な読取・編集・差分適用を最優先とし、その上で補助的な編集体験を追加する。

### 9.1 ファイル参照
区分: Claude Code基準

#### 9.1.1 @ 記法
区分: Claude Code基準
```bash
# 単一ファイル
> このファイルを解析して @src/main.py

# 複数ファイル
> 以下を比較 @src/old.py @src/new.py

# Globパターン
> 全テストファイルをレビュー @tests/**/*.test.ts

# ディレクトリ
> プロジェクト構造を説明 @src/
```

#### 9.1.2 ファイル補完
区分: Claude Code基準
**必須機能:**
- TABキーによる補完
- Fuzzyマッチング
- 最近使用したファイル履歴
- `.gitignore` 考慮

### 9.2 画像ファイル対応
区分: 基準先行 + 拡張統合

#### 9.2.1 画像入力
区分: 基準先行 + 拡張統合
```bash
# コマンドライン
$ your-agent --image screenshot.png -p "この画面を分析"

# 複数画像
$ your-agent --image design.png,mockup.png -p "差分を説明"
```

**現行実装メモ:**
- `tengu` の現行CLIでは headless 実行時に `--image` をサポートする
- 画像は選択中の LLM プロバイダーへ送信し、ツール実行とファイル操作はローカルに残す
- TUI での画像貼り付けやドラッグ&ドロップは未実装

#### 9.2.2 サポート形式
区分: 基準先行 + 拡張統合
- PNG, JPEG, GIF, WebP
- Base64エンコード対応
- サイズ制限: 5MB推奨

---

## 10. Git統合要件

この章は、Claude Code基準で必要な最小Git連携を先に定義し、自動化や高度な連携は拡張段階で扱う。

### 10.1 Git操作
区分: Claude Code基準

#### 10.1.1 必須機能
区分: Claude Code基準
- コミットメッセージ生成
- 差分表示・解析
- ブランチ検出
- PR作成支援
- コンフリクト解決支援

#### 10.1.2 Git履歴活用
区分: Claude Code基準
```bash
> 最新3コミットを分析
> v1.2.3の変更内容を説明
> 誰がこの機能を実装した？
> このAPIが変更された理由は？
```

### 10.2 GitHub/GitLab統合
区分: 多ベンダー拡張

#### 10.2.1 必須機能（gh/glab CLI使用）
区分: 多ベンダー拡張
- Issue取得・作成
- PR作成・レビュー
- コメント追加
- ラベル管理

**使用例:**
```bash
> Issue #123を修正してPR作成
> このPRのコメントに全て対応
```

---

## 11. 設定ファイル要件

この章は、Claude Code基準の基本設定を成立させるための必須構成を定義し、多ベンダー設定は互換拡張として加える。

### 11.1 設定ファイル形式
区分: 基準先行 + 拡張統合

**推奨: TOML** (可読性と表現力のバランス)

**ファイル構造:**
```
~/.your-agent/
├── config.toml              # メイン設定
├── AGENT.md                 # グローバルプロンプト
├── agents/                  # カスタムエージェント
│   ├── code-reviewer.json
│   └── debugger.json
├── commands/                # カスタムコマンド
│   ├── test.md
│   └── deploy.md
└── sessions/                # セッションデータ
    └── sessions.db
```

### 11.2 設定例
区分: 基準先行 + 拡張統合

**config.toml:**
```toml
# モデル設定
[model]
provider = "anthropic"
default = "claude-sonnet-4-20250514"
max_tokens = 8192

# パーミッション
[permissions]
approval_policy = "on-request"
allowed_tools = ["Read", "Write", "Bash(git *)"]
deny = ["Read(.env*)", "Write(production.*)"]

# サンドボックス
[sandbox]
mode = "workspace-write"
allowed_paths = ["./src", "./tests"]

# MCPサーバー
[mcp_servers.filesystem]
command = "npx"
args = ["-y", "@modelcontextprotocol/server-filesystem", "."]

[mcp_servers.github]
command = "npx"
args = ["-y", "@modelcontextprotocol/server-github"]
env = { GITHUB_TOKEN = "${GITHUB_TOKEN}" }

# フック
[[hooks.postToolUse]]
matcher = "Write(*.py)"
command = "black $file"

[[hooks.postToolUse]]
matcher = "Write(*.rs)"
command = "cargo fmt"
```

---

## 12. 認証要件

この章は、Claude Code基準の認証成立を優先し、追加プロバイダーの認証方式はその後に統合する。

### 12.1 認証方式
区分: 基準先行 + 拡張統合

#### 12.1.1 API Key認証
区分: Claude Code基準
**環境変数:**
```bash
export ANTHROPIC_API_KEY="sk-ant-..."
export OPENAI_API_KEY="sk-..."
export GOOGLE_API_KEY="..."
```

**設定ファイル:**
```toml
[auth]
anthropic_api_key_env = "ANTHROPIC_API_KEY"
openai_api_key_env = "OPENAI_API_KEY"
```

#### 12.1.2 OAuth認証
区分: 多ベンダー拡張
```bash
# 認証状態を保存
$ your-agent auth login

# ステータス確認
$ your-agent auth status

# ログアウト
$ your-agent auth logout
```

### 12.2 セッショントークン管理
区分: 基準先行 + 拡張統合

**保存場所:**
```
~/.your-agent/auth/
├── tokens.json              # トークンストア（暗号化）
└── session.json             # セッション情報
```

---

## 13. 出力・UI要件

この章は、Claude Code基準の対話体験を優先して定義し、表示強化や別CLI流のUI要素は拡張として扱う。

### 13.1 TUI (Terminal User Interface)
区分: Claude Code基準

#### 13.1.1 必須要素
区分: Claude Code基準
- メッセージエリア（スクロール可能）
- 入力エリア（マルチライン対応）
- ステータスバー（モデル、トークン使用量表示）
- プログレスインジケーター

#### 13.1.2 推奨フレームワーク
区分: 基準先行 + 拡張統合
- **Rust**: `ratatui`
- **Python**: `textual`, `rich`
- **Node.js**: `ink`, `blessed`

### 13.2 シンタックスハイライト
区分: Claude Code基準

**必須機能:**
- Markdownレンダリング
- コードブロックのハイライト
- 差分表示（unified diff形式）

### 13.3 ストリーミング出力
区分: Claude Code基準

**必須機能:**
- リアルタイム表示
- トークン単位のストリーミング
- プログレス表示
- キャンセル対応（Ctrl+C）

---

## 14. 高度な機能要件（Optional）

この章は、Claude Code基準の中核実装完了後に着手する拡張機能をまとめる章として扱う。

### 14.1 メモリー機能
区分: 多ベンダー拡張

**機能:**
- 永続的な事実の保存
- セッション間での共有
- 自動的なコンテキスト挿入

**コマンド:**
```bash
> /memory add "プロジェクトはTypeScriptを使用"
> /memory list
> /memory remove <id>
```

### 14.2 チェックポイント機能
区分: 多ベンダー拡張

**機能:**
- プロジェクト状態のスナップショット
- 任意の時点への復元
- 実験的な変更の安全な試行

**コマンド:**
```bash
> /checkpoint save "実験前"
> /checkpoint list
> /checkpoint restore <id>
```

### 14.3 ナレッジベース
区分: 多ベンダー拡張

**機能:**
- PDFドキュメントのインデックス化
- セマンティック検索
- コンテキストウィンドウ節約

### 14.4 クラウド実行
区分: 多ベンダー拡張

**機能:**
- LLM推論をクラウドAPI（Anthropic/OpenAI/Google）で実行
- ツール実行とファイル操作はローカルで維持
- API障害時のフォールバックとエラー可視化

**現行実装メモ:**
- `tengu` の現行CLIに独立した `cloud` サブコマンドは持たせない
- 通常の `-p` / TUI 実行時に、選択したプロバイダーAPIへ直接問い合わせる
- Claude Code基準では「推論はクラウド、ツールはローカル」の責務分離を保つ

### 14.5 レビューモード
区分: 多ベンダー拡張

**機能:**
- 専用レビューエージェント起動
- 差分ベースのレビュー
- プライオリティ付き指摘

**コマンド:**
```bash
> /review
> /review --base main
> /review --preset security

$ your-agent review
$ your-agent review --base main
$ your-agent review --base main --preset security
```

**現行実装メモ:**
- `tengu review` と TUI の `/review` は最小実装済み
- `git diff` を収集し、レビュー用プロンプトを生成して LLM に渡す
- `--base` は `<base>...HEAD` の差分、`--preset` は `general/security/performance/correctness` をサポートする

---

## 15. 実装優先順位

この章は、常にClaude Code基準の中核要件を先行させ、その後に多ベンダー拡張を積む順序で解釈する。

### Phase 1: MVP（Minimum Viable Product）
区分: Claude Code基準

**目標: 基本的な対話型AIエージェントの実現**

1. ✅ **REPL環境構築**
   - インタラクティブプロンプト
   - 基本的な入出力
   - セッション管理（メモリ内）

2. ✅ **LLMプロバイダー統合**
   - 少なくとも1プロバイダー（Anthropic or OpenAI）
   - ストリーミング対応
   - エラーハンドリング

3. ✅ **基本ツール実装**
   - `Read`: ファイル読み込み
   - `Write`: ファイル書き込み
   - `Shell`: コマンド実行

4. ✅ **設定ファイル読み込み**
   - TOML形式のパース
   - 環境変数の展開
   - デフォルト値の設定

5. ✅ **セッション永続化**
   - ファイルベースのセッション保存
   - `/resume` コマンド

**期間: 2-3週間**

### Phase 2: 拡張機能
区分: 多ベンダー拡張

**目標: 実用的な機能の追加**

6. ✅ **MCP統合**
   - STDIOサーバーのサポート
   - ツール検出・実行
   - 基本的なMCP管理コマンド

7. ✅ **システムプロンプト管理**
   - AGENT.mdの読み込み
   - 階層的マージ
   - コマンドライン引数での上書き

8. ✅ **パーミッション制御**
   - ツール別承認設定
   - Globパターンマッチング
   - 承認フローのUI

9. ✅ **カスタムコマンド**
   - Markdownベースのコマンド定義
   - プロジェクトスコープの呼び出し規則
   - 引数の受け渡し

**期間: 3-4週間**

### Phase 3: 高度な機能
区分: 多ベンダー拡張

**目標: プロフェッショナル向け機能**

10. ✅ **カスタムエージェント**
    - JSON設定ファイル
    - エージェント切り替え
    - AI支援のエージェント生成

11. ✅ **フック機能**
    - Pre/Postフック
    - パターンマッチング
    - 環境変数の受け渡し

12. ✅ **Git統合**
    - libgit2 or git CLIラッパー
    - 差分表示
    - コミットメッセージ生成

13. ✅ **TUIの改善**
    - リッチなMarkdownレンダリング
    - シンタックスハイライト
    - プログレス表示

**期間: 4-5週間**

### Phase 4: エンタープライズ機能
区分: 多ベンダー拡張

**目標: チーム・企業での利用**

14. ✅ **クラウドLLM推論運用**
    - プロバイダーAPIの安定運用
    - API障害時のエラー処理
    - ローカルツール実行との責務分離

15. ✅ **高度なMCP機能**
    - HTTPサーバーサポート
    - リモートMCPサーバー
    - MCPサーバーのモニタリング

16. ✅ **セキュリティ強化**
    - サンドボックス実装
    - 監査ログ
    - ポリシーエンジン

17. ✅ **チーム連携**
    - 設定の共有
    - エージェント設定のバージョン管理
    - チームテンプレート

**期間: 6-8週間**

---

## 16. 技術スタック推奨

この章は、まずClaude Code基準の実装成立に必要な技術を優先し、拡張要件に応じて追加技術を採用する前提で読む。

### 16.1 プログラミング言語
区分: 基準先行 + 拡張統合

| 言語 | メリット | デメリット |
|------|---------|----------|
| **Rust** | 高速、メモリ安全、シングルバイナリ | 学習曲線が急 |
| **Python** | 豊富なライブラリ、開発速度 | 配布が複雑、遅い |
| **Go** | シンプル、並行処理が得意 | ライブラリが少ない |
| **TypeScript** | Node.js エコシステム | パフォーマンス |

**推奨: Rust** (高速・安全・配布が容易)

### 16.2 主要ライブラリ（Rust）
区分: Claude Code基準

```toml
[dependencies]
# CLI
clap = { version = "4", features = ["derive"] }
ratatui = "0.26"
crossterm = "0.27"

# LLM
async-openai = "0.20"
anthropic-sdk = "0.2"

# 設定
serde = { version = "1", features = ["derive"] }
toml = "0.8"

# MCP
tokio = { version = "1", features = ["full"] }
serde_json = "1"

# Git
git2 = "0.18"

# その他
anyhow = "1"
tracing = "0.1"
```

### 16.3 アーキテクチャ
区分: Claude Code基準

**レイヤー構成:**
```
┌─────────────────────────────────┐
│         CLI Interface           │  (clap, ratatui)
├─────────────────────────────────┤
│      Session Management         │  (履歴、状態管理)
├─────────────────────────────────┤
│        Agent Executor           │  (エージェントループ)
├─────────────────────────────────┤
│    Tool System + MCP Client     │  (ツール実行)
├─────────────────────────────────┤
│      LLM Provider Abstraction   │  (プロバイダー抽象化)
├─────────────────────────────────┤
│    Config + Permission Engine   │  (設定、権限)
└─────────────────────────────────┘
```

---

## 17. テスト要件

この章は、Claude Code基準の中核挙動を最優先で検証対象とし、拡張機能の検証はその後に追加する。

### 17.1 ユニットテスト
区分: Claude Code基準

**カバレッジ目標: 80%以上**

**重点テスト項目:**
- 設定ファイルのパース
- パーミッションチェック
- ツール実行ロジック
- セッション永続化

### 17.2 統合テスト
区分: 基準先行 + 拡張統合

**テストシナリオ:**
- LLM API通信（モック）
- MCPサーバー起動・通信
- ファイルシステム操作
- Git操作

### 17.3 E2Eテスト
区分: 基準先行 + 拡張統合

**テストケース:**
- 基本的な対話フロー
- ファイル編集タスク
- エラー処理
- セッション再開

---

## 18. ドキュメント要件

この章は、Claude Code基準の利用と実装に必要な文書を先に整備し、拡張機能の説明は段階追加とする。

### 18.1 必須ドキュメント
区分: 基準先行 + 拡張統合

1. **README.md**
   - プロジェクト概要
   - クイックスタート
   - インストール手順

2. **USAGE.md**
   - 基本的な使い方
   - コマンドリファレンス
   - 設定例

3. **CONFIGURATION.md**
   - 設定ファイルの詳細
   - 全オプションの説明
   - ベストプラクティス

4. **MCP_GUIDE.md**
   - MCPサーバーの追加方法
   - カスタムMCPサーバーの作成
   - トラブルシューティング

5. **CONTRIBUTING.md**
   - 開発環境のセットアップ
   - コーディング規約
   - プルリクエストのプロセス

---

## 19. パフォーマンス要件

この章は、まずClaude Code基準の体験を損なわない性能水準を定義し、追加機能の最適化は後続で扱う。

### 19.1 レスポンス時間
区分: Claude Code基準

- **起動時間**: < 500ms
- **コマンド実行**: < 100ms
- **ファイル読み込み**: < 50ms (1MB以下)
- **LLM応答開始**: < 2s (ストリーミング開始)

### 19.2 メモリ使用量
区分: Claude Code基準

- **アイドル時**: < 50MB
- **通常使用時**: < 200MB
- **大規模プロジェクト**: < 500MB

### 19.3 並行処理
区分: 基準先行 + 拡張統合

- 複数ツールの並列実行
- 非同期I/O
- ストリーミング処理

---

## 20. セキュリティ要件

この章は、Claude Code基準の安全性を成立させる要件を最優先とし、多ベンダー拡張に伴う追加リスクはその上で管理する。

### 20.1 機密情報の保護
区分: Claude Code基準

- APIキーの暗号化保存
- `.env` ファイルの読み取り拒否（デフォルト）
- ログからの機密情報除外

### 20.2 コマンド実行の安全性
区分: Claude Code基準

- 危険なコマンドの警告
- 承認フローの強制
- シェルインジェクション対策

### 20.3 監査ログ
区分: 基準先行 + 拡張統合

- 全ツール実行のログ記録
- タイムスタンプ付き
- ユーザー識別可能

---

## 21. 参考資料

### 21.1 公式ドキュメント
区分: 基準先行 + 拡張統合

- [Claude Code Documentation](https://code.claude.com/docs/)
- [Codex CLI Documentation](https://developers.openai.com/codex/cli/)
- [Gemini CLI Documentation](https://geminicli.com/docs/)
- [Kiro CLI Documentation](https://kiro.dev/docs/cli/)

### 21.2 仕様・標準
区分: 基準先行 + 拡張統合

- [Model Context Protocol](https://modelcontextprotocol.io/)
- [Anthropic API Reference](https://docs.anthropic.com/en/api/)
- [OpenAI API Reference](https://platform.openai.com/docs/api-reference)

### 21.3 関連ツール
区分: 多ベンダー拡張

- [Aider](https://aider.chat/) - オープンソースAIコーディングツール
- [Kiro CLI Documentation](https://kiro.dev/docs/cli/)
- [OpenAI API Reference](https://platform.openai.com/docs/api-reference)
- [Continue](https://continue.dev/) - IDE統合AIアシスタント

---

## 付録A: 用語集
区分: 基準先行 + 拡張統合

| 用語 | 説明 |
|------|------|
| **REPL** | Read-Eval-Print Loop。対話型実行環境 |
| **MCP** | Model Context Protocol。LLMとツールを接続する標準プロトコル |
| **TUI** | Terminal User Interface。ターミナル上のUI |
| **Headless** | 非対話モード。スクリプトやCI/CDでの利用を想定 |
| **Agent** | 自律的にタスクを実行するAIシステム |
| **Tool** | エージェントが利用できる機能（ファイル操作、コマンド実行等） |
| **Hook** | 特定のイベント発生時に実行される処理 |

---

## 付録B: サンプル設定ファイル
区分: 基準先行 + 拡張統合

### 完全な設定例
区分: 基準先行 + 拡張統合

```toml
# ~/.your-agent/config.toml

# ========================================
# モデル設定
# ========================================
[model]
provider = "anthropic"
default = "claude-sonnet-4-20250514"
max_tokens = 8192
temperature = 0.7

[model.anthropic]
api_key_env = "ANTHROPIC_API_KEY"
base_url = "https://api.anthropic.com"

[model.openai]
api_key_env = "OPENAI_API_KEY"
base_url = "https://api.openai.com/v1"

# ========================================
# パーミッション
# ========================================
[permissions]
approval_policy = "on-request"
allowed_tools = [
  "Read",
  "Write",
  "Bash(git *)",
  "Bash(npm test)",
  "Bash(cargo *)"
]
deny = [
  "Read(.env*)",
  "Write(production.*)",
  "Bash(rm -rf *)"
]

# ========================================
# サンドボックス
# ========================================
[sandbox]
mode = "workspace-write"
allowed_paths = ["./src", "./tests", "./docs"]
blocked_paths = ["./.env", "./secrets"]

# ========================================
# MCPサーバー
# ========================================
[mcp_servers.filesystem]
command = "npx"
args = ["-y", "@modelcontextprotocol/server-filesystem", "."]

[mcp_servers.github]
command = "npx"
args = ["-y", "@modelcontextprotocol/server-github"]
env = { GITHUB_TOKEN = "${GITHUB_TOKEN}" }
timeout_sec = 30

[mcp_servers.postgres]
command = "postgres-mcp-server"
args = ["--connection", "postgresql://localhost/mydb"]
env = { PGPASSWORD = "${DB_PASSWORD}" }

# ========================================
# フック
# ========================================
[[hooks.agentSpawn]]
command = "git status"
timeout_ms = 5000

[[hooks.postToolUse]]
matcher = "Write(*.py)"
command = "black $file"

[[hooks.postToolUse]]
matcher = "Write(*.rs)"
command = "cargo fmt --all"

[[hooks.postToolUse]]
matcher = "Write(*.ts)"
command = "npx prettier --write $file"

# ========================================
# その他
# ========================================
[ui]
theme = "dark"
syntax_highlighting = true
show_token_usage = true

[logging]
level = "info"
file = "~/.your-agent/logs/agent.log"
```

---

**ドキュメントバージョン**: 1.0  
**最終更新日**: 2026-01-17  
**作成者**: AI Agent CLI Development Team
