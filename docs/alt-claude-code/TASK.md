# Alt Claude Code Product Task List

## Decision

- 判断: 作り直しではなく `継続実装 + 乖離是正`
- 理由:
  - 既存の中核（TUI、ワンショット、LLMバックエンド、ローカルツール、承認、MCP、セッション）は再利用価値が高い
  - Claude Code 基準から外れているのは主に一部導線の不足と、誤って足した機能の方向性
  - 全面作り直しより、主要導線ごとの差分を埋める方が正確かつ速い

## Product Definition

実施に使える `alt Claude Code` の最小製品相当は、次の導線が成立していることとする。

1. 対話導線
   - TUI で会話、ストリーミング、承認付きツール実行ができる
2. ワンショット導線
   - `-p` で単発実行でき、`stream-json` が逐次出力される
3. 安全な編集導線
   - Read / Write / Shell / Grep / Glob が動き、差分提示と承認がある
4. レビュー導線
   - Git 差分を LLM に渡し、専用コマンドでレビューできる
5. 拡張導線
   - MCP、カスタムコマンド、カスタムエージェントの基本が成立する
6. 製品ハードニング
   - ドキュメント、テスト、実例が実装と整合する

## Gap Summary

| Flow | Current Status | Main Gap |
|----|----|----|
| 対話導線 | おおむね成立 | 一部スラッシュコマンドはスタブ |
| ワンショット導線 | 成立 | 追加の回帰テスト不足 |
| 安全な編集導線 | 成立 | 実運用向けのコマンド例の整理不足 |
| レビュー導線 | 部分実装 | README/要件/CLI導線の完成不足 |
| 拡張導線 | 部分実装 | agent/auth はまだスタブが多い |
| 製品ハードニング | 未完 | 統合テストと最終整合が不足 |

## Phases

### Phase A: Core Product

| ID | Status | Summary | DependsOn |
|----|--------|---------|-----------|
| ALT-001 | ✅ | 継続実装で進める判断と主要導線の定義を確定する | - |
| ALT-002 | ✅ | Claude Code 基準から外れた cloud 導線を除去する | ALT-001 |
| ALT-003 | completed | レビュー導線（CLI `review` / TUI `/review`）を製品相当に仕上げる | ALT-001 |

### Phase B: Feature Completion

| ID | Status | Summary | DependsOn |
|----|--------|---------|-----------|
| ALT-004 | completed | 画像入力導線を実装する | ALT-003 |
| ALT-005 | completed | エージェント管理コマンドを実体化する | ALT-003 |
| ALT-006 | completed | Auth コマンドを実体化する | ALT-003 |

### Phase C: Hardening

| ID | Status | Summary | DependsOn |
|----|--------|---------|-----------|
| ALT-007 | pending | 主要導線の統合テストを追加する | ALT-003,ALT-004,ALT-005,ALT-006 |
| ALT-008 | pending | README / REQUIREMENTS / TASK の整合を最終化する | ALT-007 |
