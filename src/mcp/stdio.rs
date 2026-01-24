use anyhow::{anyhow, Result};
use serde::Serialize;
use serde_json::Value;
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

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

pub fn list_tools_stdio(server: &McpServerConfig) -> Result<Vec<McpTool>> {
    let command = server
        .command
        .as_ref()
        .ok_or_else(|| anyhow!("mcp server command is required for stdio"))?;
    let args = server.args.as_ref().cloned().unwrap_or_default();
    let mut child = spawn_stdio_server(command, &args, server.env.as_ref())?;
    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| anyhow!("failed to open stdin for mcp server"))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| anyhow!("failed to open stdout for mcp server"))?;
    let mut reader = BufReader::new(stdout);

    let mut next_id = 1u64;
    send_initialize(&mut stdin, next_id)?;
    read_response(&mut reader, next_id)?;
    next_id += 1;

    send_initialized(&mut stdin)?;

    let mut tools = Vec::new();
    let mut cursor: Option<String> = None;
    loop {
        let id = next_id;
        next_id += 1;
        send_tools_list(&mut stdin, id, cursor.as_deref())?;
        let result = read_response(&mut reader, id)?;
        let list: ToolsListResult = serde_json::from_value(result)?;
        tools.extend(list.tools);
        cursor = list.next_cursor;
        if cursor.is_none() {
            break;
        }
    }

    let _ = child.kill();
    let _ = child.wait();
    Ok(tools)
}

fn spawn_stdio_server(
    command: &str,
    args: &[String],
    env: Option<&std::collections::BTreeMap<String, String>>,
) -> Result<Child> {
    let mut cmd = Command::new(command);
    cmd.args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit());
    if let Some(env) = env {
        for (key, value) in env {
            cmd.env(key, value);
        }
    }
    Ok(cmd.spawn()?)
}

fn send_initialize(stdin: &mut ChildStdin, id: u64) -> Result<()> {
    let params = serde_json::json!({
        "protocolVersion": PROTOCOL_VERSION,
        "capabilities": {},
        "clientInfo": {
            "name": "tengu",
            "version": env!("CARGO_PKG_VERSION")
        }
    });
    let request = JsonRpcRequest {
        jsonrpc: "2.0",
        id,
        method: "initialize",
        params: Some(params),
    };
    send_message(stdin, &request)
}

fn send_initialized(stdin: &mut ChildStdin) -> Result<()> {
    let notification = JsonRpcNotification {
        jsonrpc: "2.0",
        method: "notifications/initialized",
        params: None,
    };
    send_message(stdin, &notification)
}

fn send_tools_list(stdin: &mut ChildStdin, id: u64, cursor: Option<&str>) -> Result<()> {
    let params = match cursor {
        Some(cursor) => serde_json::json!({ "cursor": cursor }),
        None => serde_json::json!({}),
    };
    let request = JsonRpcRequest {
        jsonrpc: "2.0",
        id,
        method: "tools/list",
        params: Some(params),
    };
    send_message(stdin, &request)
}

fn send_message<T: Serialize>(stdin: &mut ChildStdin, message: &T) -> Result<()> {
    let payload = serde_json::to_string(message)?;
    stdin.write_all(payload.as_bytes())?;
    stdin.write_all(b"\n")?;
    stdin.flush()?;
    Ok(())
}

fn read_response(reader: &mut BufReader<ChildStdout>, id: u64) -> Result<Value> {
    let mut line = String::new();
    loop {
        line.clear();
        if reader.read_line(&mut line)? == 0 {
            return Err(anyhow!("mcp server closed stdout"));
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let value: Value = serde_json::from_str(trimmed)?;
        if let Some(result) = extract_result_by_id(&value, id) {
            return result;
        }
    }
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
