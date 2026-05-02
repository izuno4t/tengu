# REQUIREMENTS Gap Analysis

作成日: 2026-05-02
更新日: 2026-05-03

## 目的

`docs/REQUIREMENTS.md` に対する現行実装の不足度を整理し、要求を満たすために必要な実装タスクの根拠を記録する。

最終 parity 判定は [PARITY_REPORT.md](PARITY_REPORT.md) を正とする。

## 対象

- 要求定義: `docs/REQUIREMENTS.md`
- 現行タスク: `TASK.md`
- 主な実装: `src/cli.rs`, `src/tools/tools.rs`, `src/config/config.rs`, `src/tui/controller.rs`, `src/session/session.rs`
- 検証: `cargo test`, `cargo clippy`, `markdownlint-cli2`

## 総合評価

| 観点 | 達成度の目安 | 状態 |
| ---- | ------------ | ---- |
| Claude Code 基準の中核機能 | 90-95% | 主要導線、安全既定、状態復元、補助UXは成立している |
| `docs/REQUIREMENTS.md` 全体 | 85-90% | 外部依存証跡と高度 optional 領域を除き実装済み |
| 製品完了度 | 80-90% | 完全な対話TUI E2E、live 外部連携E2E、full coverage は継続課題 |

## 成立している主要導線

- TUI 起動、対話、ストリーミング表示、主要スラッシュコマンドは実装済み。
- Headless `-p` と `json` / `stream-json` 出力は実装済み。
- Anthropic / OpenAI / Google / Ollama の LLM バックエンド抽象は存在する。
- Read / Edit / Write / Bash / Grep / Glob / ListFiles / WebFetch / WebSearch のツールは実装済み。
- MCP の STDIO / HTTP ツール検出導線は存在する。
- カスタムコマンド、カスタムエージェント、レビュー導線、画像入力、ファイル参照補完は実装済み。
- GitHub/GitLab の issue / PR / label / comment / review 導線は `gh` / `glab` wrapper として実装済み。
- `.env` 既定拒否、監査ログ、hooks、暗号化 token store、checkpoint / rollback、project memory は実装済み。
- `cargo test` は 322 unit tests と 5 e2e tests に成功している。

## 主要領域

### 必須ツール

`docs/REQUIREMENTS.md` が必須ツールとしている `WebFetch` と `WebSearch` は、`Tool` / `ToolInput`、CLI `tool` サブコマンド、LLM向けツール定義、JSON実行経路へ追加済みである。

影響:

- URL取得とWeb検索をツール実行基盤で扱える。
- WebFetch は SSRF 対策として private / local アドレスを拒否する。

### CLI引数

`--allowed-tools`, `--cwd`, `--add-dir` は主要実行経路に反映済みである。`--cwd` は作業ディレクトリを切り替え、`--add-dir` はワークスペース文脈としてプロンプトに加わり、`--allowed-tools` はツールポリシーへ反映される。

影響:

- READMEや要求定義に近い起動時制御をCLIから扱える。
- CI/CDやスクリプト利用時にツール制限を指定できる。

### パーミッション仕様

要求で求められている Glob、正規表現、否定パターンは、`allowed_tools` / `deny` のツールルール照合へ追加済みである。

影響:

- `Bash(git (status|log|diff))` のような正規表現ルールを扱える。
- `!Read(.env*)` や `!Read(public/**)` のような否定ルールを、許可リストの除外または拒否リストの例外として扱える。

### フック機構

要求で定義されている `agentSpawn`, `userPromptSubmit`, `preToolUse`, `postToolUse` の設定構造は読み込み可能である。`preToolUse` / `postToolUse` は `ToolExecutor` の実行前後に統合済みで、matcher、環境変数、stdin入力、timeout、`on_error` を扱える。

影響:

- Write後の自動フォーマットやツール実行前の検査を設定から実行できる。
- 専用監査ログ形式は `[security] audit_log` として実装済みであり、hooks はツール実行前後の自動化に集中できる。

### システムプロンプトとメモリ

要求定義が前提にしている `AGENT.md` と、既存実装で使われてきた `.tengu/TENGU.md` の両方を階層的に読み込める。新規 `/init` は `.tengu/AGENT.md` を作成し、既存の `.tengu/TENGU.md` がある場合は互換ファイルとして扱う。

構造化 project memory は `.tengu/memory.json` に保存され、CLI / TUI から add / list / search / remove できる。最新メモリは system prompt に挿入される。

影響:

- Claude Code基準の `AGENT.md` と既存利用者向けの `TENGU.md` を併用できる。
- セッションをまたぐプロジェクト知識をローカルに保持できる。

### セッション操作

セッション永続化、一覧、保存、読み込み、分岐、TUI復元は実装済みである。CLIの `resume --last` と `resume <session-id>` は選択したセッションをTUIへ復元し、`sessions list` と引数なし `resume` は再開対象を選びやすい一覧と次のコマンドを表示する。

長時間セッション相当の大きな conversation / log / queue / usage / recent files の roundtrip テストも追加済みである。

影響:

- CLIとTUIの再開導線は要求の主要操作を満たす。
- 大きめのセッション JSON の保存・復元退行を検出できる。

### ファイル参照と補完

TUI の `@` 補完、fuzzy 検索、最近使用ファイル履歴、`.gitignore` 考慮は実装済みである。画像添付は headless `--image` と TUI `/image` / dropped path に対応する。

影響:

- Claude Code基準の対話的なファイル指定体験に必要な最小導線を満たす。
- recent files をセッションに保存し、復元後も参照候補として利用できる。

### Git/レビューとホスティング連携

`/diff`, `/commit`, `/pr`, review コマンドは存在する。Git差分表示、ローカルcommit確認アクション、PR作成確認アクション、review prompt生成は一時Gitリポジトリを使う統合寄りのテストで検証済みである。

GitHub/GitLab は `gh` / `glab` を利用する wrapper として、issue / PR / MR / label / comment / review の CLI/TUI 導線を持つ。

影響:

- Git/レビュー主要導線の回帰はユニットテストより広い範囲で検出できる。
- live `gh` / `glab` 実行結果の E2E は環境依存のため、opt-in integration profile として分離する余地がある。

### ドキュメント

要求の必須ドキュメントである `USAGE.md`, `CONFIGURATION.md`, `MCP_GUIDE.md` は作成済みである。README には各文書への導線を追加済みである。

ALT-036 で [PARITY_REPORT.md](PARITY_REPORT.md) を追加し、Phase H の最終判定を記録した。

影響:

- 利用者向けのコマンドリファレンス、設定仕様、MCP運用手順、最終 parity 判定を個別文書で参照できる。

### テストとカバレッジ

ユニットテストは存在し、`cargo test` は成功している。主要CLIは実バイナリを起動するE2Eハーネスで、auth、sessions、agent、MCP、tools、perf、大規模リポジトリ相当の file tools を検証している。

性能計測は `tengu perf` で起動経路、軽量コマンド、1MBファイル読み込み、RSSメモリを測定し、要求基準値と比較できる。カバレッジ計測は `scripts/coverage.sh` で `cargo llvm-cov` を優先し、`cargo tarpaulin` にフォールバックする導線を追加した。固定toolchainまたはmatching LLVM toolsでは、対話TUI描画と外部network transport adapterを除いた core coverage が 81.12% である。

影響:

- 主要CLIのE2E証跡、long-session regression、大規模リポジトリ regression、core coverage 80% 達成証跡は追加済みである。
- full coverage 80% と完全な対話TUI E2E証跡は継続改善対象である。

### セキュリティと監査

APIキーは環境変数または暗号化 token store で扱う。`.env` / `.env.*` はデフォルトでファイル系ツールからブロックし、`[security] blocked_paths` で追加の機密パスを拒否できる。`[security] audit_log` が指定された場合、ツール実行の成功・失敗・拒否をJSON Linesで記録し、Write内容やBashコマンドなどの機密化しやすい入力は監査ログ上でredactする。

影響:

- `.env` の既定拒否、監査ログ、暗号化 token store の実装証跡は追加済みである。
- OAuth フロー自体は未実装だが、ALT-032 の要求は「OAuthまたは暗号化トークン保存」であり、暗号化保存で満たした。

## 残る課題

1. 擬似端末ベースの完全な TUI E2E
2. live LLM API / `gh` / `glab` / 実 MCP サーバーの opt-in integration tests
3. interactive TUI と network adapter を含む full coverage 80%
4. PDF indexing や embedding 検索を含む semantic knowledge base

## 検証結果

```text
cargo test
322 unit tests passed
5 e2e tests passed

cargo clippy -- -D warnings
passed

markdownlint-cli2 README.md TASK.md docs/TASK.md docs/REQUIREMENTS_MATRIX.md docs/REQUIREMENTS_GAP_ANALYSIS.md docs/PARITY_REPORT.md
0 errors

scripts/coverage.sh
core coverage: 81.12% lines with matching LLVM tools
full coverage: 53.25% lines; interactive TUI and network adapters are the main remaining gaps
```

## 判断記録

- 判断: Phase H の parity closure は、ローカル CLI エージェントとしての主要ワークフローにおいて達成済みとする。
- 理由: TUI、Headless、LLMバックエンド、MCP、カスタムエージェント、レビュー、ファイル参照補完、ホスティング補助、認証保護、checkpoint、project memory、回帰証跡が揃った。
- 影響: 今後の作業は新規中核機能よりも、対話端末 E2E、live integration、full coverage、optional semantic KB の品質強化が中心になる。
