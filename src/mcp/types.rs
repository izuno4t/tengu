use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::llm::ToolDefinition;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpTool {
    pub name: String,
    pub title: Option<String>,
    pub description: Option<String>,
    #[serde(rename = "inputSchema")]
    pub input_schema: Option<Value>,
}

impl McpTool {
    /// Convert to a ToolDefinition for the LLM, prefixed with server name.
    pub fn to_tool_definition(&self, server_name: &str) -> ToolDefinition {
        ToolDefinition {
            name: format!("mcp__{}_{}", server_name, self.name),
            description: self
                .description
                .clone()
                .unwrap_or_else(|| format!("MCP tool: {}", self.name)),
            input_schema: self.input_schema.clone().unwrap_or_else(|| {
                serde_json::json!({
                    "type": "object",
                    "properties": {}
                })
            }),
        }
    }
}

/// Extract text content from an MCP tool call result.
pub fn mcp_result_to_text(result: &Value) -> String {
    // MCP results typically have a "content" array with text items
    if let Some(content) = result.get("content").and_then(Value::as_array) {
        let texts: Vec<&str> = content
            .iter()
            .filter_map(|item| {
                if item.get("type").and_then(Value::as_str) == Some("text") {
                    item.get("text").and_then(Value::as_str)
                } else {
                    None
                }
            })
            .collect();
        if !texts.is_empty() {
            return texts.join("\n");
        }
    }
    // Fallback: serialize the result
    serde_json::to_string_pretty(result).unwrap_or_else(|_| result.to_string())
}

#[derive(Debug, Deserialize)]
pub struct ToolsListResult {
    pub tools: Vec<McpTool>,
    #[serde(rename = "nextCursor")]
    pub next_cursor: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mcp_tool_serialization_roundtrip() {
        let tool = McpTool {
            name: "fetch".to_string(),
            title: Some("Fetch URL".to_string()),
            description: Some("Fetch content from a URL".to_string()),
            input_schema: Some(serde_json::json!({
                "type": "object",
                "properties": {
                    "url": {"type": "string"}
                },
                "required": ["url"]
            })),
        };
        let json = serde_json::to_string(&tool).unwrap();
        let loaded: McpTool = serde_json::from_str(&json).unwrap();
        assert_eq!(loaded.name, "fetch");
        assert_eq!(loaded.title.unwrap(), "Fetch URL");
        assert!(loaded.input_schema.is_some());
    }

    #[test]
    fn mcp_tool_minimal() {
        let json = r#"{"name": "test"}"#;
        let tool: McpTool = serde_json::from_str(json).unwrap();
        assert_eq!(tool.name, "test");
        assert!(tool.title.is_none());
        assert!(tool.description.is_none());
        assert!(tool.input_schema.is_none());
    }

    #[test]
    fn tools_list_result_deserialization() {
        let json = r#"{
            "tools": [
                {"name": "tool1", "description": "first tool"},
                {"name": "tool2"}
            ],
            "nextCursor": "cursor123"
        }"#;
        let result: ToolsListResult = serde_json::from_str(json).unwrap();
        assert_eq!(result.tools.len(), 2);
        assert_eq!(result.tools[0].name, "tool1");
        assert_eq!(result.next_cursor.unwrap(), "cursor123");
    }

    #[test]
    fn tools_list_result_no_cursor() {
        let json = r#"{"tools": []}"#;
        let result: ToolsListResult = serde_json::from_str(json).unwrap();
        assert!(result.tools.is_empty());
        assert!(result.next_cursor.is_none());
    }

    #[test]
    fn mcp_tool_to_tool_definition() {
        let tool = McpTool {
            name: "search".to_string(),
            title: None,
            description: Some("Search docs".to_string()),
            input_schema: Some(serde_json::json!({
                "type": "object",
                "properties": {
                    "query": {"type": "string"}
                }
            })),
        };
        let def = tool.to_tool_definition("myserver");
        assert_eq!(def.name, "mcp__myserver_search");
        assert_eq!(def.description, "Search docs");
        assert!(def.input_schema.get("properties").is_some());
    }

    #[test]
    fn mcp_tool_to_tool_definition_no_schema() {
        let tool = McpTool {
            name: "ping".to_string(),
            title: None,
            description: None,
            input_schema: None,
        };
        let def = tool.to_tool_definition("srv");
        assert_eq!(def.name, "mcp__srv_ping");
        assert_eq!(def.description, "MCP tool: ping");
    }

    #[test]
    fn mcp_result_to_text_with_content_array() {
        let result = serde_json::json!({
            "content": [
                {"type": "text", "text": "Hello"},
                {"type": "text", "text": "World"}
            ]
        });
        assert_eq!(mcp_result_to_text(&result), "Hello\nWorld");
    }

    #[test]
    fn mcp_result_to_text_fallback() {
        let result = serde_json::json!({"status": "ok"});
        let text = mcp_result_to_text(&result);
        assert!(text.contains("status"));
        assert!(text.contains("ok"));
    }

    #[test]
    fn mcp_result_to_text_ignores_non_text_items_and_missing_text() {
        let result = serde_json::json!({
            "content": [
                {"type": "image", "data": "base64"},
                {"type": "text"},
                {"type": "text", "text": "Only text"}
            ]
        });

        assert_eq!(mcp_result_to_text(&result), "Only text");
    }

    #[test]
    fn mcp_result_to_text_falls_back_for_empty_or_non_array_content() {
        let empty = serde_json::json!({"content": []});
        let non_array = serde_json::json!({"content": "plain"});

        assert!(mcp_result_to_text(&empty).contains("content"));
        assert!(mcp_result_to_text(&non_array).contains("plain"));
    }
}
