# REQUIREMENTS Gap Analysis

作成日: 2026-05-02

## 目的

`docs/REQUIREMENTS.md` に対する現行実装の不足度を整理し、要求を満たすために必要な実装タスクの根拠を記録する。

## 対象

- 要求定義: `docs/REQUIREMENTS.md`
- 現行タスク: `TASK.md`
- 主な実装: `src/cli.rs`, `src/tools/tools.rs`, `src/config/config.rs`, `src/tui/controller.rs`
- 検証: `cargo test`, `cargo clippy`, `markdownlint-cli2`

## 総合評価

| 観点 | 達成度の目安 | 状態 |
| ---- | ------------ | ---- |
| Claude Code 基準の中核機能 | 85-90% | 主要導線と安全既定は成立している |
| `docs/REQUIREMENTS.md` 全体 | 75-85% | 拡張・品質要件に不足がある |
| 製品完了度 | 70-80% | 対話TUI/外部連携E2E、coverage実測、認証保護、任意高度機能に不足がある |

## 成立している主要導線

- TUI 起動、対話、ストリーミング表示、基本スラッシュコマンドは実装済み。
- Headless `-p` と `json` / `stream-json` 出力は実装済み。
- Anthropic / OpenAI / Google / Ollama の LLM バックエンド抽象は存在する。
- Read / Write / Shell / Grep / Glob / WebFetch / WebSearch のツールは実装済み。
- MCP の STDIO / HTTP ツール検出導線は存在する。
- カスタムコマンド、カスタムエージェント、レビュー導線、画像入力の基本は実装済み。
- `.env` 既定拒否、監査ログ、hooks、Git/レビュー統合寄りテスト、主要CLIのE2Eハーネス、性能計測、カバレッジ計測導線は追加済み。
- `cargo test` は 293 件成功している。

## 主な不足

### 必須ツール

`docs/REQUIREMENTS.md` が必須ツールとしている `WebFetch` と `WebSearch` は、`Tool` / `ToolInput`、CLI `tool` サブコマンド、LLM向けツール定義、JSON実行経路へ追加済みである。

影響:

- URL取得とWeb検索をツール実行基盤で扱える。
- WebFetch は SSRF 対策として private / local アドレスを拒否する。

### CLI引数の未接続

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

### 設定スキーマ

現行 `Config` は `model`, `permissions`, `sandbox`, `hooks`, `auth` を扱える。`model.parameters`、provider別設定、auth詳細はスキーマに追加済みである。

影響:

- 要求定義の主要なTOML設定例を読み込める。
- `model.parameters.max_tokens` と provider別 base URL は既存の実行経路へ反映できる。

### システムプロンプトファイル名

要求定義が前提にしている `AGENT.md` と、既存実装で使われてきた `.tengu/TENGU.md` の両方を階層的に読み込める。新規 `/init` は `.tengu/AGENT.md` を作成し、既存の `.tengu/TENGU.md` がある場合は互換ファイルとして扱う。

影響:

- Claude Code基準の `AGENT.md` と既存利用者向けの `TENGU.md` を併用できる。
- 明示CLI指定の `--system-prompt` / `--system-prompt-file` は引き続き最優先である。

### セッション操作

セッション永続化、一覧、保存、読み込み、分岐、TUI復元は実装済みである。CLIの `resume --last` と `resume <session-id>` は選択したセッションをTUIへ復元し、`sessions list` と引数なし `resume` は再開対象を選びやすい一覧と次のコマンドを表示する。

影響:

- CLIとTUIの再開導線は要求の主要操作を満たす。
- 端末上のピッカーUIと完全な対話TUI E2E検証は今後の強化余地として残る。

### ファイル参照と補完

`@` 記法や画像添付の基本導線はあるが、TAB補完、Fuzzyマッチング、最近使用ファイル履歴、`.gitignore` 考慮の証跡が不足している。

影響:

- Claude Code基準の対話的なファイル指定体験としては未達。

### Git/レビュー

`/diff`, `/commit`, `/pr`, review コマンドは存在する。Git差分表示、ローカルcommit確認アクション、PR作成確認アクション、review prompt生成は一時Gitリポジトリを使う統合寄りのテストで検証済みである。

影響:

- Git/レビュー主要導線の回帰はユニットテストより広い範囲で検出できる。
- GitHub/GitLab API操作、PRコメント編集、外部 `gh` 実行結果のE2E検証は今後の強化余地として残る。

### ドキュメント

要求の必須ドキュメントである `USAGE.md`, `CONFIGURATION.md`, `MCP_GUIDE.md` は作成済みである。README には各文書への導線を追加済みである。

影響:

- 利用者向けのコマンドリファレンス、設定仕様、MCP運用手順を個別文書で参照できる。
- 今後の機能追加時は README ではなく該当文書を主な更新先にする。

### テストとカバレッジ

ユニットテストは存在し、`cargo test` は成功している。主要CLIは実バイナリを起動するE2Eハーネスで、auth、sessions、agent、MCP、tools、perf の代表導線を検証している。性能計測は `tengu perf` で起動経路、軽量コマンド、1MBファイル読み込み、RSSメモリを測定し、要求基準値と比較できる。カバレッジ計測は `scripts/coverage.sh` で `cargo llvm-cov` を優先し、`cargo tarpaulin` にフォールバックする導線を追加した。ただし要求の 80% カバレッジ達成証跡、完全な対話TUI E2Eテストの証跡は不足している。

影響:

- 主要CLIのE2E証跡は追加済みだが、80% 達成の実測証跡と対話TUIの完全なE2E証跡は別途取得が必要である。
- LLM応答開始時間など外部API依存の性能証跡は今後の強化余地として残る。

### セキュリティと監査

APIキーは環境変数ベースで扱う。`.env` / `.env.*` はデフォルトでファイル系ツールからブロックし、`[security] blocked_paths` で追加の機密パスを拒否できる。`[security] audit_log` が指定された場合、ツール実行の成功・失敗・拒否をJSON Linesで記録し、Write内容やBashコマンドなどの機密化しやすい入力は監査ログ上でredactする。

影響:

- `.env` の既定拒否と監査ログの実装証跡は追加済みである。
- OAuth、暗号化トークン保存、より広い機密情報検出は今後の強化余地として残る。

## 実行計画

1. 要求差分マトリクス、TASK、READMEの完了扱いと未達扱いは現行実装基準で再整合済みである。
2. CLI引数、パーミッション、設定スキーマ、システムプロンプト互換は既存導線へ反映済みである。
3. セッション再開、主要CLI、Git/レビュー導線は統合寄りテストまたはE2Eテストを補強済みであり、ファイル参照補完は未達として管理する。
4. セキュリティ、監査ログ、性能計測、カバレッジ計測導線は整備済みであり、80% coverage達成証跡は後続で扱う。
5. 残作業は対話TUI/外部連携E2E証跡、coverage実測、OAuth/暗号化保存、任意高度機能に絞って管理する。

## 検証結果

```text
cargo test
297 passed; 0 failed; 0 ignored

scripts/coverage.sh
failed: active cargo-llvm-cov could not find LLVM tools matching the active rustc.
Set matching LLVM_COV and LLVM_PROFDATA, or use a rustup toolchain with llvm-tools-preview.
```

## 判断記録

- 判断: 現行実装は破棄せず、継続実装で不足を埋める。
- 理由: TUI、Headless、LLMバックエンド、MCP、カスタムエージェント、レビューなどの中核導線は再利用できる。
- 影響: 今後の作業は新規作成よりも、仕様乖離の是正、品質証跡の追加、文書整合が中心になる。
