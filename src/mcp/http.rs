use anyhow::{anyhow, Result};
use futures_util::StreamExt;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue, CONTENT_TYPE};
use reqwest::{Client, Response};
use serde::Serialize;
use serde_json::Value;

use crate::mcp::{McpServerConfig, McpTool, ToolsListResult};

const PROTOCOL_VERSION: &str = "2025-11-25";

#[derive(Debug, Clone, Serialize)]
struct JsonRpcRequest<'a> {
    jsonrpc: &'static str,
    id: u64,
    method: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    params: Option<Value>,
}

#[derive(Debug, Clone, Serialize)]
struct JsonRpcNotification<'a> {
    jsonrpc: &'static str,
    method: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    params: Option<Value>,
}

pub async fn list_tools_http(server: &McpServerConfig) -> Result<Vec<McpTool>> {
    let url = server
        .url
        .as_ref()
        .ok_or_else(|| anyhow!("mcp server url is required for http"))?;
    let client = build_client(server)?;

    let mut headers = build_headers(server)?;
    let mut next_id = 1u64;

    let init_request = JsonRpcRequest {
        jsonrpc: "2.0",
        id: next_id,
        method: "initialize",
        params: Some(serde_json::json!({
            "protocolVersion": PROTOCOL_VERSION,
            "capabilities": {},
            "clientInfo": {
                "name": "tengu",
                "version": env!("CARGO_PKG_VERSION")
            }
        })),
    };
    let (init_result, session_id) = send_request(&client, url, &headers, &init_request, next_id)
        .await?;
    let _ = init_result;
    if let Some(session_id) = session_id {
        headers.insert(
            HeaderName::from_static("mcp-session-id"),
            HeaderValue::from_str(&session_id)?,
        );
    }
    next_id += 1;

    let init_notification = JsonRpcNotification {
        jsonrpc: "2.0",
        method: "notifications/initialized",
        params: None,
    };
    let _ = send_notification(&client, url, &headers, &init_notification).await;

    let mut tools = Vec::new();
    let mut cursor: Option<String> = None;
    loop {
        let id = next_id;
        next_id += 1;
        let params = match cursor.as_deref() {
            Some(cursor) => serde_json::json!({ "cursor": cursor }),
            None => serde_json::json!({}),
        };
        let request = JsonRpcRequest {
            jsonrpc: "2.0",
            id,
            method: "tools/list",
            params: Some(params),
        };
        let (value, _) = send_request(&client, url, &headers, &request, id).await?;
        let list: ToolsListResult = serde_json::from_value(value)?;
        tools.extend(list.tools);
        cursor = list.next_cursor;
        if cursor.is_none() {
            break;
        }
    }

    Ok(tools)
}

fn build_client(server: &McpServerConfig) -> Result<Client> {
    let mut builder = Client::builder();
    if let Some(timeout_sec) = server.timeout_sec {
        builder = builder.timeout(std::time::Duration::from_secs(timeout_sec));
    }
    Ok(builder.build()?)
}

fn build_headers(server: &McpServerConfig) -> Result<HeaderMap> {
    let mut headers = HeaderMap::new();
    headers.insert(
        HeaderName::from_static("mcp-protocol-version"),
        HeaderValue::from_static(PROTOCOL_VERSION),
    );
    if let Some(env_var) = &server.bearer_token_env_var {
        if let Ok(token) = std::env::var(env_var) {
            let value = format!("Bearer {}", token);
            headers.insert(reqwest::header::AUTHORIZATION, HeaderValue::from_str(&value)?);
        }
    }
    if let Some(extra) = &server.http_headers {
        for (key, value) in extra {
            let name = HeaderName::from_bytes(key.as_bytes())?;
            let value = HeaderValue::from_str(value)?;
            headers.insert(name, value);
        }
    }
    Ok(headers)
}

async fn send_notification(
    client: &Client,
    url: &str,
    headers: &HeaderMap,
    notification: &JsonRpcNotification<'_>,
) -> Result<()> {
    let resp = client.post(url).headers(headers.clone()).json(notification).send().await?;
    if !resp.status().is_success() {
        return Err(anyhow!("mcp notification failed: {}", resp.status()));
    }
    Ok(())
}

async fn send_request(
    client: &Client,
    url: &str,
    headers: &HeaderMap,
    request: &JsonRpcRequest<'_>,
    id: u64,
) -> Result<(Value, Option<String>)> {
    let resp = client.post(url).headers(headers.clone()).json(request).send().await?;
    let session_id = extract_session_id(&resp);
    let value = parse_response(resp, id).await?;
    Ok((value, session_id))
}

fn extract_session_id(resp: &Response) -> Option<String> {
    resp.headers()
        .get("MCP-Session-Id")
        .or_else(|| resp.headers().get("Mcp-Session-Id"))
        .and_then(|v| v.to_str().ok())
        .map(|v| v.to_string())
}

async fn parse_response(resp: Response, id: u64) -> Result<Value> {
    let is_sse = resp
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .map(|v| v.starts_with("text/event-stream"))
        .unwrap_or(false);
    if is_sse {
        parse_sse_response(resp, id).await
    } else {
        let value: Value = resp.json().await?;
        extract_result_by_id(&value, id).ok_or_else(|| anyhow!("missing result for id {}", id))?
    }
}

async fn parse_sse_response(resp: Response, id: u64) -> Result<Value> {
    let mut buffer = String::new();
    let mut data_lines: Vec<String> = Vec::new();
    let mut stream = resp.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        buffer.push_str(&String::from_utf8_lossy(&chunk));
        while let Some(pos) = buffer.find('\n') {
            let mut line = buffer[..pos].to_string();
            if line.ends_with('\r') {
                line.pop();
            }
            buffer = buffer[pos + 1..].to_string();
            if line.is_empty() {
                if !data_lines.is_empty() {
                    let data = data_lines.join("\n");
                    data_lines.clear();
                    if data == "[DONE]" {
                        continue;
                    }
                    if let Ok(value) = serde_json::from_str::<Value>(&data) {
                        if let Some(result) = extract_result_by_id(&value, id) {
                            return result;
                        }
                    }
                }
                continue;
            }
            if let Some(rest) = line.strip_prefix("data:") {
                data_lines.push(rest.trim_start().to_string());
            }
        }
    }
    Err(anyhow!("missing sse response for id {}", id))
}

fn extract_result_by_id(value: &Value, id: u64) -> Option<Result<Value>> {
    match value {
        Value::Array(items) => {
            for item in items {
                if let Some(result) = extract_result_by_id(item, id) {
                    return Some(result);
                }
            }
            None
        }
        Value::Object(map) => {
            let message_id = map.get("id")?.as_u64()?;
            if message_id != id {
                return None;
            }
            if let Some(error) = map.get("error") {
                return Some(Err(anyhow!("mcp error: {}", error)));
            }
            let result = map.get("result")?.clone();
            Some(Ok(result))
        }
        _ => None,
    }
}
