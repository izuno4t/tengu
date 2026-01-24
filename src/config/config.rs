// Config module
// 設定ファイル管理

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Deserialize, Serialize)]
pub struct Config {
    #[serde(default)]
    pub model: ModelConfig,
    #[serde(default)]
    pub permissions: Option<PermissionsConfig>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ModelConfig {
    #[serde(default)]
    pub provider: String,
    #[serde(default)]
    pub default: String,
    pub max_tokens: Option<u32>,
    pub backend: Option<String>,
    pub name: Option<String>,
    pub backend_url: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct PermissionsConfig {
    pub approval_policy: Option<String>,
    pub allowed_tools: Option<Vec<String>>,
    pub deny: Option<Vec<String>>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            model: ModelConfig::default(),
            permissions: None,
        }
    }
}

impl Default for ModelConfig {
    fn default() -> Self {
        Self {
            provider: "anthropic".to_string(),
            default: "claude-sonnet-4-20250514".to_string(),
            max_tokens: Some(8192),
            backend: None,
            name: None,
            backend_url: None,
        }
    }
}

impl Config {
    pub fn load(path: &PathBuf) -> anyhow::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let mut config: Config = toml::from_str(&content)?;
        config.expand_env_vars();
        Ok(config)
    }

    fn expand_env_vars(&mut self) {
        self.model.provider = expand_env_vars_in_string(&self.model.provider);
        self.model.default = expand_env_vars_in_string(&self.model.default);
        if let Some(permissions) = &mut self.permissions {
            if let Some(approval_policy) = &permissions.approval_policy {
                permissions.approval_policy = Some(expand_env_vars_in_string(approval_policy));
            }
            if let Some(allowed_tools) = &mut permissions.allowed_tools {
                for item in allowed_tools.iter_mut() {
                    *item = expand_env_vars_in_string(item);
                }
            }
            if let Some(deny) = &mut permissions.deny {
                for item in deny.iter_mut() {
                    *item = expand_env_vars_in_string(item);
                }
            }
        }
    }
}

fn expand_env_vars_in_string(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch != '$' {
            output.push(ch);
            continue;
        }

        match chars.peek() {
            Some('{') => {
                chars.next();
                let mut name = String::new();
                while let Some(&next_ch) = chars.peek() {
                    if next_ch == '}' {
                        chars.next();
                        break;
                    }
                    name.push(next_ch);
                    chars.next();
                }

                if name.is_empty() {
                    output.push_str("${}");
                } else if let Ok(val) = std::env::var(&name) {
                    output.push_str(&val);
                } else {
                    output.push_str("${");
                    output.push_str(&name);
                    output.push('}');
                }
            }
            Some(next_ch) if is_env_var_char(*next_ch) => {
                let mut name = String::new();
                while let Some(&next_ch) = chars.peek() {
                    if !is_env_var_char(next_ch) {
                        break;
                    }
                    name.push(next_ch);
                    chars.next();
                }

                if let Ok(val) = std::env::var(&name) {
                    output.push_str(&val);
                } else {
                    output.push('$');
                    output.push_str(&name);
                }
            }
            _ => {
                output.push('$');
            }
        }
    }

    output
}

fn is_env_var_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '_'
}
