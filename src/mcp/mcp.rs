// MCP module
// Model Context Protocol統合

pub struct McpServer {
    pub name: String,
    pub command: Vec<String>,
}

impl McpServer {
    pub fn new(name: String, command: Vec<String>) -> Self {
        Self { name, command }
    }
}
