use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct McpConfig {
    #[serde(default)]
    pub mcp_servers: BTreeMap<String, McpServerConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct McpServerConfig {
    pub command: Option<String>,
    #[serde(default)]
    pub args: Option<Vec<String>>,
    #[serde(default)]
    pub env: Option<BTreeMap<String, String>>,
    pub url: Option<String>,
    pub bearer_token_env_var: Option<String>,
    #[serde(default)]
    pub http_headers: Option<BTreeMap<String, String>>,
    pub timeout_sec: Option<u64>,
}

pub struct McpStore;

impl McpStore {
    pub fn default_path() -> PathBuf {
        PathBuf::from(".").join(".tengu").join("mcp.toml")
    }

    pub fn load(path: &Path) -> anyhow::Result<McpConfig> {
        if !path.exists() {
            return Ok(McpConfig::default());
        }
        let content = std::fs::read_to_string(path)?;
        let config: McpConfig = toml::from_str(&content)?;
        Ok(config)
    }

    pub fn save(path: &Path, config: &McpConfig) -> anyhow::Result<()> {
        if let Some(parent) = path.parent() {
            if !parent.exists() {
                std::fs::create_dir_all(parent)?;
            }
        }
        let content = toml::to_string_pretty(config)?;
        std::fs::write(path, content)?;
        Ok(())
    }
}
