# MCP Protocol Notes (Task-032)

このメモは、MCP 統合（TASK-014）に向けた最小限の仕様整理です。実装時は最新版と版間差分に注意します。

## 1. 基本: JSON-RPC 2.0 必須

- MCP のメッセージは JSON-RPC 2.0 に厳密準拠。
- JSON-RPC メッセージは UTF-8 でエンコードする。
- Request の `id` は `null` 不可、同一セッション内の再利用不可。
- Notification は `id` を持たない。

## 2. ライフサイクル（初期化必須）

- 通信開始は `initialize` リクエスト → サーバー応答 → `notifications/initialized` の順。
- 初期化前に通常リクエストを送らない。
- バージョン交渉は `initialize` の `protocolVersion` で行う。

## 3. ツール検出と呼び出し

- ツール一覧は `tools/list`、実行は `tools/call`。
- ツールは `name`/`description`/`inputSchema` を持つ（JSON Schema）。
- **ツール名は `@server/tool` の名前空間で区別**する（Tengu 方針）。

## 4. トランスポート

### 4.1 stdio

- サーバーは subprocess として起動。
- stdin/stdout に JSON-RPC メッセージを改行区切りで送受信。
- メッセージ内に改行を含めない（改行は区切りとしてのみ使用）。
- stdout は MCP メッセージのみ、stderr はログ用途。

### 4.2 Streamable HTTP（最新版）

- HTTP GET/POST を同一エンドポイントで提供。
- `MCP-Protocol-Version` ヘッダ必須（例: `2025-11-25`）。
- ヘッダが無い場合は `2025-03-26` を仮定する互換規定がある。
- セッション ID は `MCP-Session-Id`。
- 初期化時にセッション ID が返る場合は以降リクエストで必須。

### 4.3 旧 HTTP+SSE との互換

- 2025-11-25 版で Streamable HTTP に置換。
- 旧 HTTP+SSE との互換ガイドがあるため、必要ならフォールバックを検討。

## 5. SSE（Server-Sent Events）仕様要点

- Content-Type は `text/event-stream`、UTF-8。
- `event:`/`data:`/`id:`/`retry:` フィールド、空行でイベント区切り。
- `Last-Event-ID` による再開をサポート。

## 6. バージョニング注意点

- バージョンは `YYYY-MM-DD` 形式。
- 現行バージョンは **2025-11-25**（2026-01-24 時点）。
- 実装では **サーバーと交渉したバージョン**を採用し、互換性を確保する。

---

## 出典（一次情報）

```text
https://modelcontextprotocol.io/specification/2025-11-25/basic
https://modelcontextprotocol.io/specification/2025-11-25/basic/lifecycle
https://modelcontextprotocol.io/specification/2025-11-25/basic/transports
https://modelcontextprotocol.io/specification/2025-11-25/server/tools
https://modelcontextprotocol.io/specification/versioning
https://html.spec.whatwg.org/dev/server-sent-events.html
```
