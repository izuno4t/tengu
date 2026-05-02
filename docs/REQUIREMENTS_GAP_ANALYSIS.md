# REQUIREMENTS Gap Analysis

作成日: 2026-05-02

## 目的

`docs/REQUIREMENTS.md` に対する現行実装の不足度を整理し、要求を満たすために必要な実装タスクの根拠を記録する。

## 対象

- 要求定義: `docs/REQUIREMENTS.md`
- 現行タスク: `TASK.md`
- 主な実装: `src/cli.rs`, `src/tools/tools.rs`, `src/config/config.rs`, `src/tui/controller.rs`
- 検証: `cargo test`

## 総合評価

| 観点 | 達成度の目安 | 状態 |
| ---- | ------------ | ---- |
| Claude Code 基準の中核機能 | 75-85% | 主要導線は成立している |
| `docs/REQUIREMENTS.md` 全体 | 60-70% | 拡張・品質要件に不足がある |
| 製品完了度 | 50-60% | 文書、性能、セキュリティ、検証証跡が不足している |

## 成立している主要導線

- TUI 起動、対話、ストリーミング表示、基本スラッシュコマンドは実装済み。
- Headless `-p` と `json` / `stream-json` 出力は実装済み。
- Anthropic / OpenAI / Google / Ollama の LLM バックエンド抽象は存在する。
- Read / Write / Shell / Grep / Glob / WebFetch / WebSearch のツールは実装済み。
- MCP の STDIO / HTTP ツール検出導線は存在する。
- カスタムコマンド、カスタムエージェント、レビュー導線、画像入力の基本は実装済み。
- `cargo test` は 44 件成功している。

## 主な不足

### 必須ツール

`docs/REQUIREMENTS.md` が必須ツールとしている `WebFetch` と `WebSearch` は、`Tool` / `ToolInput`、CLI `tool` サブコマンド、LLM向けツール定義、JSON実行経路へ追加済みである。

影響:

- URL取得とWeb検索をツール実行基盤で扱える。
- WebFetch は SSRF 対策として private / local アドレスを拒否する。

### CLI引数の未接続

`--allowed-tools`, `--cwd`, `--add-dir` はCLI引数として定義されているが、主要実行経路で設定やツールポリシーへ反映されていない。

影響:

- READMEや要求定義に近い起動時制御が実際には効かない。
- CI/CDやスクリプト利用時の安全なツール制限が不十分になる。

### パーミッション仕様

要求で求められている Glob、正規表現、否定パターンは、`allowed_tools` / `deny` のツールルール照合へ追加済みである。

影響:

- `Bash(git (status|log|diff))` のような正規表現ルールを扱える。
- `!Read(.env*)` や `!Read(public/**)` のような否定ルールを、許可リストの除外または拒否リストの例外として扱える。

### フック機構

要求で定義されている `agentSpawn`, `userPromptSubmit`, `preToolUse`, `postToolUse` の設定構造は読み込み可能である。`preToolUse` / `postToolUse` は `ToolExecutor` の実行前後に統合済みで、matcher、環境変数、stdin入力、timeout、`on_error` を扱える。

影響:

- Write後の自動フォーマットやツール実行前の検査を設定から実行できる。
- 監査ログ要件は `postToolUse` で出力先を設定できるが、専用監査ログ形式は後続のセキュリティ既定拒否タスクで扱う。

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
- 端末上のピッカーUIとE2E検証は今後の強化余地として残る。

### ファイル参照と補完

`@` 記法や画像添付の基本導線はあるが、TAB補完、Fuzzyマッチング、最近使用ファイル履歴、`.gitignore` 考慮の証跡が不足している。

影響:

- Claude Code基準の対話的なファイル指定体験としては未達。

### ドキュメント

要求の必須ドキュメントのうち `USAGE.md`, `CONFIGURATION.md`, `MCP_GUIDE.md` が存在しない。

影響:

- 利用者向けのコマンドリファレンス、設定仕様、MCP運用手順がREADMEに偏る。
- 実装完了判定の証跡が不足する。

### テストとカバレッジ

ユニットテストは存在し、`cargo test` は成功している。ただし要求の 80% カバレッジ目標、統合テスト、E2Eテスト、性能計測の証跡は不足している。

影響:

- 要求定義の品質要件を完了扱いにできない。
- 主要導線の回帰リスクを検出しにくい。

### セキュリティと監査

APIキーは環境変数ベースで扱うが、暗号化保存、`.env` 読み取り拒否のデフォルト、ログからの機密情報除外、監査ログの実装証跡が不足している。

影響:

- セキュリティ要件を満たしたとは言えない。
- Shell / Write / Web系ツールを拡張する前に安全基盤を強化する必要がある。

## 実行計画

1. まず要求差分マトリクスを現行コード基準で更新し、完了扱いと未達扱いを明確にする。
2. CLI引数、パーミッション、設定スキーマ、システムプロンプト互換を先に整え、既存導線の仕様乖離を減らす。
3. セッション再開、ファイル参照補完、Git/レビューE2Eを補強する。
4. セキュリティ、監査ログ、性能計測、カバレッジを整備する。
5. 必須ドキュメントを作成し、`README.md`, `TASK.md`, `docs/REQUIREMENTS.md` の完了表現を再整合する。

## 検証結果

```text
cargo test
44 passed; 0 failed; 0 ignored
```

## 判断記録

- 判断: 現行実装は破棄せず、継続実装で不足を埋める。
- 理由: TUI、Headless、LLMバックエンド、MCP、カスタムエージェント、レビューなどの中核導線は再利用できる。
- 影響: 今後の作業は新規作成よりも、仕様乖離の是正、品質証跡の追加、文書整合が中心になる。
