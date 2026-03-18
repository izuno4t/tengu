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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_has_empty_servers() {
        let config = McpConfig::default();
        assert!(config.mcp_servers.is_empty());
    }

    #[test]
    fn load_nonexistent_returns_default() {
        let path = PathBuf::from("/nonexistent/mcp.toml");
        let config = McpStore::load(&path).unwrap();
        assert!(config.mcp_servers.is_empty());
    }

    #[test]
    fn save_and_load_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("mcp.toml");

        let mut config = McpConfig::default();
        let mut server = McpServerConfig::default();
        server.command = Some("node".to_string());
        server.args = Some(vec!["server.js".to_string()]);
        server.timeout_sec = Some(30);
        config.mcp_servers.insert("test-server".to_string(), server);

        McpStore::save(&path, &config).unwrap();
        let loaded = McpStore::load(&path).unwrap();
        assert_eq!(loaded.mcp_servers.len(), 1);
        let srv = loaded.mcp_servers.get("test-server").unwrap();
        assert_eq!(srv.command.as_deref(), Some("node"));
        assert_eq!(srv.args.as_ref().unwrap(), &vec!["server.js".to_string()]);
        assert_eq!(srv.timeout_sec, Some(30));
    }

    #[test]
    fn save_creates_parent_dirs() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sub").join("dir").join("mcp.toml");
        let config = McpConfig::default();
        McpStore::save(&path, &config).unwrap();
        assert!(path.exists());
    }

    #[test]
    fn server_config_with_http_fields() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("mcp.toml");

        let mut config = McpConfig::default();
        let mut server = McpServerConfig::default();
        server.url = Some("https://api.example.com".to_string());
        server.bearer_token_env_var = Some("API_TOKEN".to_string());
        let mut headers = BTreeMap::new();
        headers.insert("X-Custom".to_string(), "value".to_string());
        server.http_headers = Some(headers);
        config.mcp_servers.insert("http-server".to_string(), server);

        McpStore::save(&path, &config).unwrap();
        let loaded = McpStore::load(&path).unwrap();
        let srv = loaded.mcp_servers.get("http-server").unwrap();
        assert_eq!(srv.url.as_deref(), Some("https://api.example.com"));
        assert_eq!(srv.bearer_token_env_var.as_deref(), Some("API_TOKEN"));
        assert_eq!(
            srv.http_headers.as_ref().unwrap().get("X-Custom").unwrap(),
            "value"
        );
    }

    #[test]
    fn server_config_with_env_vars() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("mcp.toml");

        let mut config = McpConfig::default();
        let mut server = McpServerConfig::default();
        server.command = Some("python".to_string());
        let mut env = BTreeMap::new();
        env.insert("DB_HOST".to_string(), "localhost".to_string());
        env.insert("DB_PORT".to_string(), "5432".to_string());
        server.env = Some(env);
        config.mcp_servers.insert("db-server".to_string(), server);

        McpStore::save(&path, &config).unwrap();
        let loaded = McpStore::load(&path).unwrap();
        let srv = loaded.mcp_servers.get("db-server").unwrap();
        let env = srv.env.as_ref().unwrap();
        assert_eq!(env.get("DB_HOST").unwrap(), "localhost");
        assert_eq!(env.get("DB_PORT").unwrap(), "5432");
    }

    #[test]
    fn multiple_servers() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("mcp.toml");

        let mut config = McpConfig::default();
        let mut s1 = McpServerConfig::default();
        s1.command = Some("server1".to_string());
        let mut s2 = McpServerConfig::default();
        s2.command = Some("server2".to_string());
        config.mcp_servers.insert("s1".to_string(), s1);
        config.mcp_servers.insert("s2".to_string(), s2);

        McpStore::save(&path, &config).unwrap();
        let loaded = McpStore::load(&path).unwrap();
        assert_eq!(loaded.mcp_servers.len(), 2);
    }
}
