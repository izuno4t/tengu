# コントリビューションガイド

Tenguへのコントリビューションを歓迎します！ 👺

## 開発環境のセットアップ

### 必要なもの

- Rust 1.75以上
- Cargo

### セットアップ

```bash
# リポジトリをクローン
git clone https://github.com/yourusername/tengu.git
cd tengu

# ビルド
cargo build

# テスト実行
cargo test

# 実行
cargo run
```

## コーディング規約

### Rust

- Rust 2021 edition使用
- `cargo clippy` のすべての推奨に従う
- `cargo fmt` でフォーマット
- パブリック関数には必ずドキュメントコメントを追加

### コミットメッセージ

Conventional Commits形式を推奨：

```
feat: 新機能追加
fix: バグ修正
docs: ドキュメント更新
refactor: リファクタリング
test: テスト追加・修正
chore: その他の変更
```

例：
```
feat: MCPサーバー自動検出機能を追加
fix: セッション保存時のクラッシュを修正
docs: README.mdにインストール手順を追加
```

## プルリクエストのプロセス

1. **Issue作成**
   - 新機能や大きな変更の場合、まずIssueを作成して議論

2. **ブランチ作成**
   ```bash
   git checkout -b feature/awesome-feature
   ```

3. **開発**
   - テストを書く
   - ドキュメントを更新
   - `cargo clippy` でチェック
   - `cargo fmt` でフォーマット

4. **コミット**
   ```bash
   git commit -m "feat: Add awesome feature"
   ```

5. **プッシュ**
   ```bash
   git push origin feature/awesome-feature
   ```

6. **プルリクエスト作成**
   - 変更内容を明確に記述
   - 関連するIssueをリンク

## テスト

### ユニットテスト

```bash
cargo test
```

### 特定のテストのみ実行

```bash
cargo test test_name
```

### カバレッジ

```bash
cargo tarpaulin --out Html
```

## ドキュメント

### APIドキュメント生成

```bash
cargo doc --open
```

### README更新

新機能を追加した場合、README.mdも更新してください。

## リリースプロセス

1. バージョンを`Cargo.toml`で更新
2. CHANGELOGを更新
3. タグを作成
   ```bash
   git tag -a v0.2.0 -m "Release v0.2.0"
   git push origin v0.2.0
   ```

## 質問・サポート

- GitHub Issues: バグ報告・機能要望
- GitHub Discussions: 質問・議論

## 行動規範

- 建設的なフィードバックを心がける
- 他のコントリビューターを尊重する
- オープンで歓迎的なコミュニティを維持

## ライセンス

コントリビューションはMITライセンスの下で提供されます。
