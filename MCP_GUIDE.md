# Tengu MCP Guide

この文書は Tengu で MCP サーバーを登録し、ツールとして利用するための運用手順をまとめる。MCP プロトコルの実装メモは [docs/MCP_PROTOCOL_NOTES.md](docs/MCP_PROTOCOL_NOTES.md) を参照する。

## 概要

MCP は外部ツールサーバーを JSON-RPC ベースで接続する仕組みである。Tengu は現在、STDIO と HTTP/SSE 系のサーバー設定を扱い、検出したツールを `@server/tool` 形式で参照する。

## サーバー追加

```bash
# stdio サーバーを追加
tengu mcp add filesystem -- npx @modelcontextprotocol/server-filesystem .

# PostgreSQL サーバー例
tengu mcp add postgres -- npx @modelcontextprotocol/server-postgres postgresql://localhost/mydb
```

登録内容は Tengu の MCP store に保存される。サーバー名はツール名前空間に使われるため、短く安定した名前にする。

## 一覧と削除

```bash
# サーバー一覧
tengu mcp list

# サーバー削除
tengu mcp remove filesystem
```

## ツール検出

```bash
# すべてのサーバーからツール一覧を取得
tengu mcp tools

# 特定サーバーのツール一覧を取得
tengu mcp tools filesystem
```

ツールは `@filesystem/read_file` のような `@server/tool` 形式で表示される。

## 利用例

```bash
tengu -p "Use the filesystem MCP tools to inspect package files"
```

LLM が MCP ツールを選択できる状態では、Tengu の通常ツールと同じ会話内で外部ツールを呼び出す。

## HTTPサーバー設定

HTTP/SSE 系サーバーは設定ファイル側で管理する。例を示す。

```toml
[mcp.servers.docs]
transport = "http"
url = "https://example.com/mcp"
bearer_token_env_var = "DOCS_MCP_TOKEN"
```

`bearer_token_env_var` を使うと、トークン値を設定ファイルに直接保存せず環境変数から渡せる。

## 環境変数

stdio サーバーへ環境変数を渡す場合は、サーバー設定の `env` を使う。

```toml
[mcp.servers.github]
command = "npx"
args = ["@modelcontextprotocol/server-github"]
env = { GITHUB_TOKEN = "${GITHUB_TOKEN}" }
```

機密値は環境変数参照にし、TOML に平文で保存しない。

## 権限と安全性

MCP ツールも通常ツールと同じく、許可ルールと承認ポリシーの対象として扱う。許可する場合は `@server/tool` の利用範囲を明確にする。

```toml
[permissions]
approval_policy = "on-request"
allowed_tools = [
  "Read",
  "Bash(git *)",
  "@filesystem/read_file",
]
```

外部サーバーは Tengu プロセス外で動作する。サーバー自体のアクセス権、ネットワーク到達範囲、渡す環境変数を必ず確認する。

## トラブルシューティング

### サーバーが一覧に出ない

```bash
tengu mcp list
```

登録がない場合は `tengu mcp add` で追加する。登録済みでもツール一覧が空の場合、サーバーコマンドが起動できているか確認する。

### ツール検出に失敗する

```bash
tengu mcp tools <server>
```

stdio サーバーでは stdout が JSON-RPC 専用である必要がある。ログは stderr に出すようサーバー側を調整する。

### 認証に失敗する

`bearer_token_env_var` や `env` で参照している環境変数が設定されているか確認する。

```bash
env | grep MCP
```

値そのものをログや issue に貼らない。

## 運用上の注意

- 信頼できる MCP サーバーだけを登録する。
- DB やクラウド操作を行うサーバーは read-only 権限から始める。
- CI では必要なサーバーだけを登録する。
- トークンは短命なもの、または最小権限のものを使う。
