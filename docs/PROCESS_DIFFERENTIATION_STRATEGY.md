# Process Differentiation Strategy

作成日: 2026-05-04

## 目的

Tengu を既存のコーディングエージェント製品へ追従するだけの CLI ではなく、自社の開発スキーム、品質基準、承認プロセス、リリース手順に従って実装できる開発実行基盤へ拡張するための方針を整理する。

本書は、既存製品との比較で見えた一般機能を踏まえたうえで、Tengu が差別化すべき機能領域を定義する。

## 前提

Tengu は、ローカルで動作するコーディングエージェント CLI として次の中核機能をすでに持つ。

- CLI / TUI の対話実行と headless 実行
- Anthropic / OpenAI / Google / Ollama の LLM バックエンド
- Read / Edit / Write / Bash / Grep / Glob / ListFiles / WebFetch / WebSearch
- MCP、hooks、permission、sandbox、approval
- session resume、checkpoint / rollback、project memory
- review、GitHub / GitLab wrapper、custom command、custom agent

そのため、今後の差別化では「より多くの一般機能を追加する」よりも、「組織固有の開発プロセスを安全に再現できる」ことを重視する。

## 既存製品追従として有効な一般機能

以下は、Claude Code、Codex、Gemini CLI、Copilot coding agent、Cursor、Cline、aider、OpenCode などと比較して、製品水準を上げるために有効な一般機能である。ただし、これらは単体では差別化になりにくい。

| 領域 | 代表製品 | Tenguでの扱い |
| ---- | -------- | -------------- |
| クラウド/バックグラウンド実行 | Codex Cloud, Copilot coding agent, Cursor Background Agents | optional。ローカル実行前提と衝突しない範囲で後段 |
| コンテナサンドボックス | Codex, Gemini CLI | 高優先。安全性強化として有効 |
| managed policy | Claude Code, Gemini CLI | 高優先。組織適用の土台になる |
| 拡張パッケージ | Claude plugins/skills, Gemini extensions | 中優先。自社プロセス配布の器として有効 |
| Plan / Act 分離 | Cline, Gemini CLI | 高優先。プロセスゲートと相性が良い |
| repo map / symbol index | aider | 中優先。大規模リポジトリ対応として有効 |
| auto lint / test loop | aider, Codex | 高優先。品質ゲートと相性が良い |
| ブラウザ操作 | Cline | 中優先。Web UI検証が必要な組織では有効 |
| telemetry / 作業ログ | Gemini CLI, Codex Cloud | 高優先。監査・改善に必要 |
| LSP統合 | OpenCode | 高優先。診断・symbol検索・コード理解の土台になる |
| client/server API | OpenCode | 中優先。IDE、Web、外部自動化と接続しやすい |
| IDE連携 | OpenCode, Cursor, Cline | 中優先。TUI以外の利用導線として有効 |

## ベンチマーク対象に追加する実装

vibe-local / vibe-coder と OpenCode もベンチマーク対象に追加する。

vibe-local については、特に `paper/vibe-coder-technical-report.tex` を、実装方針、開発方法論、テスト戦略、CJK/IME対応、ローカルLLM運用を整理したレポートとして扱う。

vibe-local から参考にすべき観点は次のとおり。

- 教育・研究用途を強く意識した offline-first / no-account / no-cost の導線
- 単一ファイル、Python 標準ライブラリのみ、Ollama 直接接続という radical simplicity
- 低スペックから高スペックまでのモデル選択と sidecar model 利用
- XML tool-call fallback とローカルLLM向け tool-use 補正
- safe tool の並列実行と deterministic な出力順
- CJK display width、IME、検索ローカライズ、許可プロンプトの多言語対応
- PDF / Notebook / image を含むローカルファイル読み取り
- file watcher と auto-test loop
- DECSTBM による固定フッター、TUI debug log、scroll diagnostics、PTYテスト
- 初心者向けのインストーラ、トラブルシューティング、copy-paste可能な回復手順
- 並列AI監査を使った反復開発方法論

OpenCode から参考にすべき観点は次のとおり。

- build / plan の primary agent と general / explore / compaction / title / summary の system / subagent 構成
- agent ごとの model、temperature、step limit、permission、mode、prompt file
- Tab による primary agent 切替と `@agent` mention による subagent 呼び出し
- child session navigation による subagent 作業の追跡
- LSP diagnostics、symbol search、formatter status をエージェントが使える構成
- OpenAPI 3.1 を公開する headless server と SSE event stream
- TUI を client として扱い、IDE / Web / SDK / mobile など複数clientに拡張できる設計
- `/share` による会話共有と enterprise 向け disable / SSO / self-host 方針
- permission の object syntax、wildcard、external directory 制御
- 多言語README / docs と、VS Code / Cursor / Windsurf などIDE導線

## vibe-local比較で劣後している機能

Tengu はクラウドLLMを含む multi-provider、MCP、GitHub/GitLab wrapper、process-as-code 方向では強い。一方、vibe-local と比較すると、次の点は劣後または未整理である。

| 領域 | vibe-local の特徴 | Tengu の状態 | 追加候補 |
| ---- | ---------------- | ------------ | -------- |
| 完全オフライン導線 | no account / no cloud / no cost を前提化 | Ollama対応はあるが、製品導線はクラウドLLM寄り | offline-first profile と Ollama bootstrap を追加 |
| zero-dependency教材性 | 単一Pythonファイル、stdlibのみ | Rust binary と通常の依存関係 | single-binary 配布と educational mode を強化 |
| sidecar model | 軽量タスク用モデルを自動選択 | agent-specific model routing は構想段階 | sidecar model policy を実装候補に追加 |
| local LLM tool-use補正 | XML fallback、tool-call抽出、dedup | provider別 tool_use に依存しがち | XML / text tool-call fallback を追加 |
| safe tool並列実行 | Read / Glob / Grep 等を並列実行 | subagent/parallel agent はあるが tool実行並列化は限定的 | deterministic parallel safe tools を追加 |
| PDF / Notebook | ReadTool が PDF / ipynb に対応 | 画像対応はあるが PDF / Notebook は未整理 | document reader tools を追加 |
| file watcher | `/watch` で変更監視 | 変更監視は未整理 | watch-triggered review/test を追加 |
| auto-test loop | `/autotest` で自動テスト | done rules 構想はあるが常駐自動実行は未実装 | autotest daemon / TUI toggle を追加 |
| CJK/IME | IME、CJK幅、許可入力の多言語対応を重視 | CJK幅は改善済みだが IME/多言語許可は弱い | IME-aware input と localized approvals を追加 |
| TUI診断 | debug log、scroll diagnostics、PTYテスト | TUI単体テストはあるが実端末診断は弱い | `/debug-scroll`, `VIBE_DEBUG_TUI`相当を追加 |
| 初心者導線 | インストーラ、やさしい日本語、復旧手順 | READMEは製品利用者向け | beginner / workshop mode を追加 |
| AI監査方法論 | 3〜40 agent の並列監査を開発プロセス化 | subagentはあるが監査ラウンド機能は未整理 | audit tournament / review round を追加 |

## OpenCode比較で劣後している機能

Tengu は process-as-code とローカル実行の安全性では差別化余地がある。一方、OpenCode と比較すると、次の点は劣後または未整理である。

| 領域 | OpenCode の特徴 | Tengu の状態 | 追加候補 |
| ---- | --------------- | ------------ | -------- |
| primary agent 切替 | build / plan を Tab で切替 | `/plan` はあるが agent切替UIは弱い | primary agent mode と Tab切替を追加 |
| subagent追跡 | child session navigation | subagent結果はあるが子セッションUIは未整理 | child session tree / navigation を追加 |
| agent詳細設定 | model / temp / steps / mode / permission / prompt file | custom agent はあるが粒度が粗い | agent schema を拡張 |
| hidden system agents | compaction / title / summary | compaction等はあるがagent化されていない | system agent catalog を追加 |
| LSP統合 | diagnostics / symbols / formatter status | grep/glob中心 | LSP-aware context / diagnostics tool を追加 |
| client/server | OpenAPI server、SSE、TUI as client | TUI/CLI一体型 | local agent server と SDK を追加 |
| IDE連携 | VS Code/Cursor等の拡張、選択範囲共有 | 未実装 | IDE bridge / terminal extension を追加 |
| session sharing | share / unshare、enterprise制御 | local save/load中心 | private evidence sharing / self-host share を追加 |
| permission object syntax | wildcard / external_directory / tool別 action | 類似機能はあるがagent別統合は弱い | process profile と agent permission を統合 |
| 多言語導線 | README/docs 多言語 | README英語化済み、日本語docs中心 | docs i18n strategy を追加 |

## 差別化の基本方針

Tengu の差別化軸は、**Process-as-Code for Coding Agents** とする。

これは、各社・各チームの開発プロセスを Markdown や TOML で記述し、エージェントが実装前、実装中、実装後にそのプロセスを強制または支援する仕組みである。

目指す状態は次のとおり。

- エージェントが「一般的な良いコード」に加えて「自社の標準に合うコード」を書く
- 実装前に、自社の設計・承認・リスク分類ルールに従って作業を分岐する
- 実装中に、変更範囲、禁止事項、必要な検証を自動で判断する
- 実装後に、チーム標準の品質ゲート、証跡、レビュー観点を自動で満たす
- 例外判断を記録し、あとから監査・改善できる

## 追加すべき差別化機能

### 1. Process Profile

プロジェクトまたは組織単位で、開発プロセスをプロファイルとして定義する。

例:

```toml
[process]
name = "company-standard"
mode = "strict"

[process.risk]
security_files = ["src/auth/**", "infra/**", ".github/workflows/**"]
public_api_files = ["openapi/**", "src/api/**"]
data_format_files = ["schema/**", "migrations/**"]

[process.gates]
require_plan_before_edit = true
require_tests_for_code_changes = true
require_review_note_for_security_changes = true
require_docs_for_public_api_changes = true
```

必要な機能:

- `.tengu/process.toml` と `~/.tengu/process.toml` の読み込み
- 組織、プロジェクト、ローカルの優先順位
- strict / advisory / off の運用モード
- 既存 permission / hooks / task / memory との統合

差別化ポイント:

- Claude Code の `CLAUDE.md` や Gemini の `GEMINI.md` は主に文脈であり、プロセス制御は設定やhooksに分散する
- Tengu は「プロセスそのもの」を第一級の設定として扱う

### 2. Work Type Classifier

作業依頼を受けた時点で、変更種別とリスクを分類する。

分類例:

- bugfix
- refactor
- feature
- test-only
- docs-only
- security-sensitive
- public-api-change
- data-format-change
- ci-cd-change
- migration

分類結果に応じて、必要なゲートを切り替える。

例:

| 分類 | 必須ゲート |
| ---- | ---------- |
| docs-only | markdownlint、用語統一 |
| test-only | 対象テスト、既存テスト影響確認 |
| feature | plan、実装、テスト、README/usage更新判定 |
| public-api-change | API互換性確認、docs更新、明示承認 |
| security-sensitive | threat note、追加レビュー、監査ログ |
| migration | rollback plan、データ影響記録 |

必要な機能:

- LLMによる初期分類
- ファイルパターンによる deterministic な補正
- 分類結果のセッション保存
- 誤分類時の手動上書き

差別化ポイント:

- 一般的な agent は「ユーザー指示を実行する」ことに寄りやすい
- Tengu は「作業種別に応じてプロセスを切り替える」

### 3. Policy-Driven Plan Gate

実装前の計画を、プロセスプロファイルに基づいて検証する。

計画に含める項目:

- 目的
- 変更範囲
- 影響するモジュール
- リスク分類
- 変更しない範囲
- 検証方法
- ドキュメント更新要否
- rollback / revert 方針
- 例外判断

ゲート例:

- security-sensitive なのに検証方法がない場合は実装不可
- public-api-change なのに docs 更新判定がない場合は確認要求
- migration なのに rollback 方針がない場合は実装不可
- refactor なのに外部挙動変更を含む場合は分類を変更

必要な機能:

- plan schema
- plan validation
- `/plan-check`
- plan approved 状態の保存
- plan と実際の diff の乖離検出

差別化ポイント:

- Plan / Act 分離を単なるUIモードではなく、組織プロセスのゲートにする

### 4. Definition of Done Engine

作業完了条件をコード化し、完了前に自動チェックする。

例:

```toml
[[done_rules]]
when = "work_type == 'feature'"
required = [
  "tests_passed",
  "docs_reviewed",
  "self_review_done",
  "no_untracked_risk"
]

[[done_rules]]
when = "changed('src/**/*.rs')"
commands = [
  "cargo fmt --check",
  "cargo test",
  "cargo clippy -- -D warnings"
]
```

必要な機能:

- 変更ファイルから done rule を選択
- コマンド実行結果の保存
- 未完了条件の一覧表示
- 完了宣言前のブロックまたは警告

差別化ポイント:

- 「テストを実行する」ではなく「この組織で完了とみなす条件を満たす」ことを保証する

### 5. Change Impact Analyzer

diff を見て、影響範囲と必要な追加作業を推定する。

分析観点:

- public API 変更
- DB schema / migration 変更
- 設定値 / 環境変数変更
- CI/CD 変更
- security boundary 変更
- user-facing behavior 変更
- generated file / lockfile 変更
- docs-only かどうか

出力例:

```text
Impact:
- public-api-change: yes, src/api/routes.rs
- docs-required: yes, README.md or docs/API.md
- tests-required: integration
- reviewer-required: platform-team
```

必要な機能:

- パターンベースの一次判定
- LLMによる意味的判定
- CODEOWNERS / team mapping との連携
- PR本文やレビュー依頼への反映

差別化ポイント:

- レビュー観点を人間が毎回思い出すのではなく、プロセスに従って自動生成する

### 6. Process-Aware Memory

現在の project memory は汎用メモリである。これを、プロセス判断に使える構造化メモリへ拡張する。

記録対象:

- 過去に採用した設計判断
- 禁止された実装パターン
- よく使う検証コマンド
- チーム固有のレビュー観点
- 例外が認められた理由
- incident / postmortem から得た注意点

必要な機能:

- memory entry に type / scope / applies_to を付与
- work type に応じた memory retrieval
- 期限切れ・無効化
- ルール化候補の提案

差別化ポイント:

- 「覚えている」ではなく「次回の計画・実装・検証に効く」

### 7. Evidence Pack

作業完了時に、第三者が追跡できる証跡を自動生成する。

含める内容:

- 入力依頼
- 作業分類
- 採用した計画
- 変更ファイル
- 実行コマンド
- テスト結果
- 例外判断
- 未確認事項
- レビュー観点
- rollback 方針

出力先:

- `.tengu/evidence/<session-id>.md`
- PR本文
- `docs/decisions/`
- CI artifact

差別化ポイント:

- 人間がレビューしやすいだけでなく、監査や再現にも使える

### 8. Team Review Routing

変更内容に応じて、必要なレビュー者やチームを推定する。

入力:

- CODEOWNERS
- `.tengu/process.toml`
- 変更ファイル
- work type
- risk level

出力:

- required reviewers
- optional reviewers
- review checklist
- PR labels
- blocked reason

差別化ポイント:

- GitHub/GitLab wrapper を「PRを作る」だけでなく「正しいレビュー導線へ流す」機能に拡張する

### 9. Process Simulation / Dry Run

実装前に、現在の依頼がどのプロセスを通るかだけを表示する。

例:

```bash
tengu process check "認証フローを変更して"
```

出力例:

```text
Work type: security-sensitive, feature
Required gates:
- plan approval
- cargo test
- security review note
- auth-team reviewer
- README update check
Blocked until:
- implementation plan includes rollback strategy
```

差別化ポイント:

- エージェントに実装させる前に、組織プロセス上の重さを見積もれる

### 10. Organization Process Pack

Process Profile、custom command、agent、hooks、review checklist、done rules をまとめた配布単位を作る。

構成例:

```text
.tengu-pack/
├── process.toml
├── commands/
├── agents/
├── hooks/
├── review-checklists/
├── done-rules.toml
└── README.md
```

必要な機能:

- `tengu pack install <path-or-url>`
- `tengu pack list`
- `tengu pack update`
- version pinning
- allowlist / signature check

差別化ポイント:

- 一般的な extension ではなく、組織の開発標準を配布するための pack として位置づける

### 11. Preset Agent System

一般的なコーディングエージェントとして必要な役割をプリセットで提供しつつ、組織やプロジェクトごとにカスタマイズまたはフルリビルドできるようにする。

プリセット例:

- planner: 要件整理、設計案作成、リスク分類
- implementer: 実装、リファクタリング、局所修正
- reviewer: 差分レビュー、セキュリティ/性能/保守性観点の指摘
- tester: テスト計画、テスト追加、失敗原因分析
- documenter: README、設計書、変更履歴、PR説明の整備
- release-manager: 変更影響、リリースノート、rollback 方針の整理
- process-auditor: process profile と done rules の遵守確認

必要な機能:

- `tengu agent presets list`
- `tengu agent presets show <name>`
- `tengu agent customize <preset> --as <name>`
- `tengu agent rebuild <name>`
- preset の version pinning
- preset agent ごとの model / temperature / max_turns / allowed_tools 設定
- 組織 process pack による preset 上書き

設計方針:

- デフォルトでは一般的な coding agent として使える
- 自社標準に合わせる場合は preset を fork して差分だけ管理する
- 完全に別プロセスを持つ組織では preset をフルリビルドできる
- agent 定義はプロンプトだけでなく、権限、検証コマンド、参照すべきメモリ、使用モデルを含める

差別化ポイント:

- 単なる custom agent ではなく、標準役割を持つ agent catalog として提供する
- 導入直後はすぐ使え、成熟した組織では自社プロセスに合わせて置き換えられる

### 12. Agent-Specific Model Routing

エージェントごとに利用する LLM を切り替える。すべてのタスクを同じモデルで処理するのではなく、役割に応じて速度、コスト、推論品質、コンテキスト長を最適化する。

例:

| Agent | 推奨モデル特性 | 理由 |
| ----- | -------------- | ---- |
| planner | 高推論・長文脈 | 要件、制約、設計案の比較が必要 |
| implementer | コード生成品質・ツール追従 | 既存コードへの適合と編集精度が必要 |
| reviewer | 高精度・批判的推論 | バグ、リスク、抜け漏れ検出が必要 |
| tester | 中〜高推論・実行ログ理解 | 失敗原因の切り分けが必要 |
| documenter | 低コスト・文書生成 | 正確だが高速な整形が重要 |
| process-auditor | deterministic / 低温度 | プロセス遵守判定のぶれを抑える |

必要な機能:

- agent 定義内の `model`, `provider`, `temperature`, `reasoning_effort`
- fallback model 設定
- コスト上限と latency 上限
- セッション内での model routing の可視化
- agent ごとの usage 集計

差別化ポイント:

- multi-LLM 対応を「モデルを選べる」だけで終わらせず、開発プロセス上の役割に結びつける

### 13. Multi-LLM Design Tournament

設計時に複数の LLM または複数の planner agent に設計案を出させ、評価基準に基づいて最も良い案を選ぶ。

基本フロー:

1. planner A / B / C が独立に設計案を作る
2. evaluator agent が process profile と要件に基づいて採点する
3. 必要なら reviewer agent がリスクを指摘する
4. 最も評価の高い案、または複数案を統合した案を採用する
5. 採用理由と不採用理由を evidence pack に保存する

評価軸:

- 要件充足
- 既存設計との整合性
- 実装コスト
- テスト容易性
- 運用リスク
- セキュリティ影響
- rollback 容易性
- 自社プロセスとの適合

必要な機能:

- `tengu design tournament "<request>"`
- planner agent の並列実行
- evaluator rubric の設定
- 設計案の score / rationale 保存
- 採用案から plan gate への接続
- 同一モデルの複数温度実行、または異なるモデル間比較

差別化ポイント:

- 複数LLMを単なるバックエンド選択ではなく、設計品質を高めるための合議・評価機構として使う
- 採用されなかった案も証跡として残るため、設計判断の透明性が高い

### 14. Local-First Accessibility Profile

vibe-local の強みである offline-first / no-account / beginner-friendly を、Tengu でも明示的な profile として扱う。

目的:

- ネットワーク制限のある研修、学校、社内演習で使える
- APIキーや有料契約を持たない利用者でも練習できる
- 初心者がターミナル、権限確認、復旧手順を学べる
- ローカルLLMの制約を前提に、tool-use と context を調整する

必要な機能:

- `tengu profile local-workshop`
- Ollama 起動確認、モデル存在確認、推奨モデル提示
- sidecar model の自動選択
- local LLM 向け XML / text tool-call fallback
- 初心者向け permission prompt と localized approvals
- `--offline` 時の WebFetch / WebSearch 無効化
- PDF / Notebook / image を含むローカル教材読み取り
- copy-paste可能なエラー回復手順

差別化ポイント:

- Tengu の process-as-code を、企業開発だけでなく教育・研修・研究ベースラインにも適用できる
- クラウドLLMが使えない場面でも、自社または教育現場の標準手順に従って作業できる

### 15. Audit Tournament

vibe-local のレポートで示されている並列AI監査の方法論を、Tengu の開発プロセス機能として一般化する。

基本フロー:

1. security / reliability / UX / i18n / performance / tests などの監査agentを並列起動する
2. 各agentが severity 付き report を出す
3. evaluator が重複、誤検知、既修正を整理する
4. 修正バッチを作成し、テストと evidence pack に接続する
5. 次回監査で回帰がないか確認する

必要な機能:

- `tengu audit tournament`
- audit charter template
- severity / confidence / affected files の構造化
- false positive / accepted / fixed / deferred の状態管理
- audit report から TASK.md / issue / evidence pack への変換
- regression test required の自動判定

差別化ポイント:

- 複数agentを単なる作業分担ではなく、品質保証プロセスとして使う
- 人間のレビュー前に、観点別の網羅的監査を繰り返せる

### 16. LSP-Aware Process Context

OpenCode の LSP 統合を、単なる symbol search ではなく、プロセス判断に使う文脈として取り込む。

目的:

- 変更対象の診断、参照、定義、formatter状態を実装前後のゲートに使う
- grep / glob だけでは拾いにくい public API 変更や破壊的変更を検出する
- エージェントが「コンパイル前に見えている問題」と「テストで初めて分かる問題」を分けて扱えるようにする

必要な機能:

- LSP server discovery と workspace ごとの起動管理
- diagnostics / symbols / references / definitions の tool 化
- done rules から LSP diagnostics を参照する仕組み
- change impact analyzer への symbol graph 入力
- formatter / code action の提案を自動編集前に提示する UI

差別化ポイント:

- LSPを「コード理解の補助」ではなく、process profile の証跡と完了判定に接続する
- 組織標準で禁止された API や層越え依存を symbol graph から検出できる

### 17. Local Agent Runtime Server

OpenCode の client/server 構成を参考にしつつ、Tengu ではローカル優先の agent runtime として提供する。

目的:

- TUI、IDE拡張、Web UI、CI補助、SDK が同じ session / permission / evidence を共有できる
- headless 実行を単なる CLI 実行ではなく、外部ツールから制御可能な local API にする
- 組織 process pack を適用した実行環境を、複数clientから一貫して使う

必要な機能:

- local-only を既定にした HTTP / SSE server
- session、task、permission、agent、evidence の API
- TUI を server client として動かす分離
- IDE bridge から selection / diagnostics / command を送る API
- process profile に基づく API permission と audit log

差別化ポイント:

- client/server化を利便性だけでなく、プロセス統制と監査の共通基盤にする
- enterprise / self-host では外部共有よりも、社内標準プロセスの強制と証跡収集を重視する

## 推奨ロードマップ

### Phase A: プロセスを読めるようにする

- `Process Profile`
- `Work Type Classifier`
- `Process Simulation / Dry Run`
- `Preset Agent System`
- `Local-First Accessibility Profile`

目的:

- まだ実装を制御しない
- 現在の依頼がどのプロセスに該当するかを見える化する
- 一般的なプリセットエージェントを使える状態にし、組織別カスタマイズの入口を作る
- offline / workshop / beginner 用の実行プロファイルを選べるようにする

### Phase B: 実装前後のゲートに使う

- `Policy-Driven Plan Gate`
- `Definition of Done Engine`
- `Change Impact Analyzer`
- `Agent-Specific Model Routing`
- `LSP-Aware Process Context`

目的:

- 実装前に計画不足を検出する
- 完了前に検証不足を検出する
- PR/レビューに必要な情報を自動生成する
- agent の役割ごとに適切な LLM を使い分ける
- LSP diagnostics / symbol 情報を process gate と impact analyzer に接続する

### Phase C: 組織標準として運用する

- `Process-Aware Memory`
- `Evidence Pack`
- `Team Review Routing`
- `Organization Process Pack`
- `Multi-LLM Design Tournament`
- `Audit Tournament`
- `Local Agent Runtime Server`

目的:

- チームの開発標準を共有・更新・監査可能にする
- エージェントの判断を属人化させない
- 重要な設計判断では複数案を比較し、採用理由を残す
- 複数agentによる観点別監査を継続的な品質保証に組み込む
- TUI / IDE / API client が同じ process profile と evidence を共有する

## 最初に実装すべきMVP

最小差別化として、次の6機能を優先する。

1. `.tengu/process.toml` の読み込み
2. work type / risk の分類表示
3. preset agent catalog の提供
4. local-workshop profile の提供
5. done rules に基づく完了前チェック
6. evidence pack のMarkdown出力

このMVPにより、Tengu は単にコードを書くCLIではなく、**自社の開発プロセスに従って作業し、その証跡を残すエージェント** として位置づけられる。

## 既存機能との対応

| 既存機能 | 拡張後の役割 |
| -------- | ------------ |
| project memory | process-aware memory の保存先 |
| hooks | deterministic gate / automation の実行基盤 |
| permission | process profile による許可・拒否の強制 |
| checkpoint | policy gate 前後の復元点 |
| review | impact analyzer と review routing の出力先 |
| GitHub/GitLab wrapper | PR作成と reviewer / label / checklist 反映 |
| TUI plan | plan gate の操作UI |
| TASK.md | done rules と進捗管理の可視化先 |
| custom agent | preset agent の fork / customize / rebuild の土台 |
| multi-provider LLM | agent-specific model routing と design tournament の実行基盤 |
| Ollama backend | local-first profile と sidecar model routing の土台 |
| subagent / parallel agent | audit tournament の実行基盤 |
| tools | LSP-aware context と diagnostics collection の実行基盤 |
| headless execution | local agent runtime server の土台 |

## 参考情報

- [Claude Code slash commands](https://docs.claude.com/en/docs/claude-code/slash-commands)
- [Claude Code settings](https://docs.claude.com/en/docs/claude-code/settings)
- [Claude Code extensions overview](https://code.claude.com/docs/en/features-overview)
- [OpenAI Codex cloud](https://platform.openai.com/docs/codex)
- [OpenAI Codex CLI getting started](https://help.openai.com/en/articles/11096431-openai-codex-ci-getting-started)
- [Gemini CLI documentation](https://google-gemini.github.io/gemini-cli/)
- [GitHub Copilot coding agent](https://docs.github.com/copilot/concepts/about-assigning-tasks-to-copilot)
- [Cline overview](https://docs.cline.bot/introduction/overview)
- [aider repository map](https://aider.chat/docs/repomap.html)
- [vibe-local repository](https://github.com/ochyai/vibe-local)
- [vibe-coder technical report](https://github.com/ochyai/vibe-local/blob/main/paper/vibe-coder-technical-report.tex)
- [OpenCode repository](https://github.com/anomalyco/opencode)
- [OpenCode agents documentation](https://opencode.ai/docs/agents/)
- [OpenCode server documentation](https://opencode.ai/docs/server/)
- [OpenCode LSP documentation](https://opencode.ai/docs/lsp/)
