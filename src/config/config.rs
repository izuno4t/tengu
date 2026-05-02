// Config module
// 設定ファイル管理

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Default, Deserialize, Serialize)]
pub struct Config {
    #[serde(default)]
    pub model: ModelConfig,
    #[serde(default)]
    pub permissions: Option<PermissionsConfig>,
    #[serde(default)]
    pub sandbox: Option<SandboxConfig>,
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

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct PermissionsConfig {
    pub approval_policy: Option<String>,
    pub allowed_tools: Option<Vec<String>>,
    pub deny: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct SandboxConfig {
    pub mode: Option<String>,
    pub allowed_paths: Option<Vec<String>>,
    pub blocked_paths: Option<Vec<String>>,
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
        if let Some(backend) = &self.model.backend {
            self.model.backend = Some(expand_env_vars_in_string(backend));
        }
        if let Some(name) = &self.model.name {
            self.model.name = Some(expand_env_vars_in_string(name));
        }
        if let Some(backend_url) = &self.model.backend_url {
            self.model.backend_url = Some(expand_env_vars_in_string(backend_url));
        }
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
        if let Some(sandbox) = &mut self.sandbox {
            if let Some(mode) = &sandbox.mode {
                sandbox.mode = Some(expand_env_vars_in_string(mode));
            }
            if let Some(allowed_paths) = &mut sandbox.allowed_paths {
                for item in allowed_paths.iter_mut() {
                    *item = expand_env_vars_in_string(item);
                }
            }
            if let Some(blocked_paths) = &mut sandbox.blocked_paths {
                for item in blocked_paths.iter_mut() {
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn default_model_config_values() {
        let mc = ModelConfig::default();
        assert_eq!(mc.provider, "anthropic");
        assert_eq!(mc.default, "claude-sonnet-4-20250514");
        assert_eq!(mc.max_tokens, Some(8192));
        assert!(mc.backend.is_none());
        assert!(mc.name.is_none());
        assert!(mc.backend_url.is_none());
    }

    #[test]
    fn default_config_has_default_model() {
        let cfg = Config::default();
        assert_eq!(cfg.model.provider, "anthropic");
        assert!(cfg.permissions.is_none());
        assert!(cfg.sandbox.is_none());
    }

    #[test]
    fn loads_config_from_toml_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("tengu.toml");
        let mut f = std::fs::File::create(&path).unwrap();
        writeln!(
            f,
            r#"
[model]
provider = "openai"
default = "gpt-4o"
max_tokens = 4096
"#
        )
        .unwrap();

        let cfg = Config::load(&path).unwrap();
        assert_eq!(cfg.model.provider, "openai");
        assert_eq!(cfg.model.default, "gpt-4o");
        assert_eq!(cfg.model.max_tokens, Some(4096));
    }

    #[test]
    fn loads_config_with_permissions() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("tengu.toml");
        let mut f = std::fs::File::create(&path).unwrap();
        writeln!(
            f,
            r#"
[model]
provider = "anthropic"
default = "claude-sonnet-4-20250514"

[permissions]
approval_policy = "auto"
allowed_tools = ["Read", "Write"]
deny = ["Bash"]
"#
        )
        .unwrap();

        let cfg = Config::load(&path).unwrap();
        let perms = cfg.permissions.unwrap();
        assert_eq!(perms.approval_policy.unwrap(), "auto");
        assert_eq!(perms.allowed_tools.unwrap(), vec!["Read", "Write"]);
        assert_eq!(perms.deny.unwrap(), vec!["Bash"]);
    }

    #[test]
    fn loads_config_with_sandbox() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("tengu.toml");
        let mut f = std::fs::File::create(&path).unwrap();
        writeln!(
            f,
            r#"
[model]
provider = "anthropic"
default = "claude-sonnet-4-20250514"

[sandbox]
mode = "strict"
allowed_paths = ["/tmp", "/home"]
blocked_paths = ["/etc"]
"#
        )
        .unwrap();

        let cfg = Config::load(&path).unwrap();
        let sb = cfg.sandbox.unwrap();
        assert_eq!(sb.mode.unwrap(), "strict");
        assert_eq!(sb.allowed_paths.unwrap(), vec!["/tmp", "/home"]);
        assert_eq!(sb.blocked_paths.unwrap(), vec!["/etc"]);
    }

    #[test]
    fn load_nonexistent_file_returns_error() {
        let result = Config::load(&PathBuf::from("/nonexistent/tengu.toml"));
        assert!(result.is_err());
    }

    #[test]
    fn load_invalid_toml_returns_error() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bad.toml");
        std::fs::write(&path, "not a [valid toml {{{").unwrap();
        let result = Config::load(&path);
        assert!(result.is_err());
    }

    #[test]
    fn expands_env_vars_in_provider() {
        std::env::set_var("TENGU_TEST_PROVIDER", "google");
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("tengu.toml");
        let mut f = std::fs::File::create(&path).unwrap();
        writeln!(
            f,
            r#"
[model]
provider = "$TENGU_TEST_PROVIDER"
default = "gemini-pro"
"#
        )
        .unwrap();

        let cfg = Config::load(&path).unwrap();
        assert_eq!(cfg.model.provider, "google");
        std::env::remove_var("TENGU_TEST_PROVIDER");
    }

    #[test]
    fn expands_env_vars_braces_syntax() {
        std::env::set_var("TENGU_TEST_MODEL", "my-model");
        let result = expand_env_vars_in_string("${TENGU_TEST_MODEL}");
        assert_eq!(result, "my-model");
        std::env::remove_var("TENGU_TEST_MODEL");
    }

    #[test]
    fn preserves_unset_env_vars() {
        let result = expand_env_vars_in_string("$NONEXISTENT_TENGU_VAR_XYZ");
        assert_eq!(result, "$NONEXISTENT_TENGU_VAR_XYZ");
    }

    #[test]
    fn preserves_unset_braces_env_vars() {
        let result = expand_env_vars_in_string("${NONEXISTENT_TENGU_VAR_XYZ}");
        assert_eq!(result, "${NONEXISTENT_TENGU_VAR_XYZ}");
    }

    #[test]
    fn expand_empty_braces() {
        let result = expand_env_vars_in_string("${}");
        assert_eq!(result, "${}");
    }

    #[test]
    fn expand_dollar_at_end() {
        let result = expand_env_vars_in_string("hello$");
        assert_eq!(result, "hello$");
    }

    #[test]
    fn is_env_var_char_accepts_alphanumeric_and_underscore() {
        assert!(is_env_var_char('A'));
        assert!(is_env_var_char('z'));
        assert!(is_env_var_char('0'));
        assert!(is_env_var_char('_'));
        assert!(!is_env_var_char('-'));
        assert!(!is_env_var_char('.'));
    }

    #[test]
    fn minimal_config_uses_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("tengu.toml");
        std::fs::write(&path, "").unwrap();
        let cfg = Config::load(&path).unwrap();
        assert_eq!(cfg.model.provider, "anthropic");
        assert_eq!(cfg.model.default, "claude-sonnet-4-20250514");
    }
}
