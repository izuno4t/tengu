# Git Naming & Workflow Rules for Coding Agents

このドキュメントは、AIコーディングエージェントが本リポジトリで作業する際に遵守すべきGit操作の標準ルールを定義したものです。

---

## 1. ブランチ命名規則 (Branch Naming)

すべての作業は、目的に応じたプレフィックス（接頭辞）を持つフィーチャーブランチで行ってください。

### 命名フォーマット
`タイプ/簡潔な説明` または `タイプ/チケット番号-簡潔な説明`

| プレフィックス | 使用ケース | 例 |
| :--- | :--- | :--- |
| feat/ | 新機能の開発 | feat/add-user-auth |
| fix/ | バグ修正 | fix/login-error-on-ios |
| refactor/ | 機能変更を伴わないコードのリファクタリング | refactor/cleanup-api-client |
| docs/ | ドキュメントのみの修正・追加 | docs/update-readme |
| chore/ | ビルドプロセスやライブラリの更新など | chore/update-deps |
| test/ | テストコードの追加・修正 | test/add-unit-tests-for-parser |

### 基本原則
- **小文字のみ**を使用し、単語間は**ハイフン (-)** で繋いでください。
- 意味のない名前（update, work, ai-patch など）は避けてください。

---

## 2. コミットメッセージ規約 (Commit Messages)

Conventional Commits 仕様に準拠してください。

### フォーマット
\`\`\`text
<type>(<scope>): <description>

[optional body]

[optional footer(s)]
\`\`\`

### 必須ルール
1. **Type**: feat, fix, docs, style, refactor, perf, test, build, ci, chore, revert のいずれかを使用すること。
2. **Subject**: 命令形（例: "add ..."）で記述し、文末にピリオドを打たないこと。
3. **Scope (任意)**: 影響範囲（例: auth, ui, api）を記述。

---

## 3. ワークフロー (Workflow)

原則として **GitHub Flow** に基づき動作してください。

1. **常に最新を維持**: 作業開始前にベースブランチ（main 等）から最新を取得すること。
2. **作業用ブランチの作成**: 命名規則に従い、main から分岐すること。
3. **アトミックコミット**: 1つのコミットには1つの論理的な変更のみを含めること。
4. **プッシュとPR作成**: 作業完了後、リモートへプッシュし PR を作成すること。

---

## 4. エージェントへの特記事項 (Special Instructions for AI)

- **セキュリティ**: .env や秘密鍵を絶対にコミットしないこと。
- **破壊的変更**: 既存の API 等を壊す場合は、type に ! を付けるか BREAKING CHANGE を明記すること。
- **透明性**: 複雑な Git 操作を行う前には、必ずユーザーに確認を得ること。
