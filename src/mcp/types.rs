use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpTool {
    pub name: String,
    pub title: Option<String>,
    pub description: Option<String>,
    #[serde(rename = "inputSchema")]
    pub input_schema: Option<Value>,
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
}
