// Config module
// 設定ファイル管理

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Default, Deserialize, Serialize)]
pub struct Config {
    #[serde(default)]
    pub model: ModelConfig,
    #[serde(default)]
    pub permissions: Option<PermissionsConfig>,
    #[serde(default)]
    pub sandbox: Option<SandboxConfig>,
    #[serde(default)]
    pub hooks: Option<HooksConfig>,
    #[serde(default)]
    pub auth: Option<AuthConfig>,
    #[serde(default)]
    pub security: Option<SecurityConfig>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ModelConfig {
    #[serde(default)]
    pub provider: String,
    #[serde(default)]
    pub default: String,
    pub max_tokens: Option<u32>,
    pub temperature: Option<f32>,
    pub reasoning_effort: Option<String>,
    pub cache_prompts: Option<bool>,
    pub backend: Option<String>,
    pub name: Option<String>,
    pub backend_url: Option<String>,
    pub parameters: Option<ModelParametersConfig>,
    pub anthropic: Option<ModelProviderConfig>,
    pub openai: Option<ModelProviderConfig>,
    pub google: Option<ModelProviderConfig>,
    pub local: Option<ModelProviderConfig>,
    #[serde(default)]
    pub providers: HashMap<String, ModelProviderConfig>,
}

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct ModelParametersConfig {
    pub max_tokens: Option<u32>,
    pub temperature: Option<f32>,
    pub reasoning_effort: Option<String>,
    pub cache_prompts: Option<bool>,
}

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct ModelProviderConfig {
    pub api_key_env: Option<String>,
    pub api_key: Option<String>,
    pub base_url: Option<String>,
    pub max_tokens: Option<u32>,
    pub organization: Option<String>,
    pub project: Option<String>,
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

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct SecurityConfig {
    pub audit_log: Option<String>,
    pub audit_enabled: Option<bool>,
    pub allow_env_files: Option<bool>,
    pub blocked_paths: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct HooksConfig {
    #[serde(default)]
    pub agent_spawn: Vec<HookConfig>,
    #[serde(default)]
    pub user_prompt_submit: Vec<HookConfig>,
    #[serde(default)]
    pub pre_tool_use: Vec<HookConfig>,
    #[serde(default)]
    pub post_tool_use: Vec<HookConfig>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct HookConfig {
    pub command: String,
    pub matcher: Option<String>,
    pub timeout_ms: Option<u64>,
    pub cache_ttl_seconds: Option<u64>,
    pub on_error: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct AuthConfig {
    pub anthropic_api_key_env: Option<String>,
    pub openai_api_key_env: Option<String>,
    pub google_api_key_env: Option<String>,
    pub token_store: Option<String>,
    pub session_path: Option<String>,
    pub oauth_enabled: Option<bool>,
}

impl Default for ModelConfig {
    fn default() -> Self {
        Self {
            provider: "anthropic".to_string(),
            default: "claude-sonnet-4-20250514".to_string(),
            max_tokens: Some(8192),
            temperature: None,
            reasoning_effort: None,
            cache_prompts: None,
            backend: None,
            name: None,
            backend_url: None,
            parameters: None,
            anthropic: None,
            openai: None,
            google: None,
            local: None,
            providers: HashMap::new(),
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
        self.model.expand_env_vars();
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
        if let Some(hooks) = &mut self.hooks {
            hooks.expand_env_vars();
        }
        if let Some(auth) = &mut self.auth {
            auth.expand_env_vars();
        }
        if let Some(security) = &mut self.security {
            security.expand_env_vars();
        }
    }
}

impl ModelConfig {
    pub fn effective_max_tokens(&self) -> Option<u32> {
        self.max_tokens.or_else(|| {
            self.parameters
                .as_ref()
                .and_then(|params| params.max_tokens)
        })
    }

    fn expand_env_vars(&mut self) {
        if let Some(reasoning_effort) = &self.reasoning_effort {
            self.reasoning_effort = Some(expand_env_vars_in_string(reasoning_effort));
        }
        if let Some(parameters) = &mut self.parameters {
            parameters.expand_env_vars();
        }
        for provider in [
            &mut self.anthropic,
            &mut self.openai,
            &mut self.google,
            &mut self.local,
        ]
        .into_iter()
        .flatten()
        {
            provider.expand_env_vars();
        }
        for provider in self.providers.values_mut() {
            provider.expand_env_vars();
        }
    }
}

impl ModelParametersConfig {
    fn expand_env_vars(&mut self) {
        if let Some(reasoning_effort) = &self.reasoning_effort {
            self.reasoning_effort = Some(expand_env_vars_in_string(reasoning_effort));
        }
    }
}

impl ModelProviderConfig {
    fn expand_env_vars(&mut self) {
        if let Some(api_key_env) = &self.api_key_env {
            self.api_key_env = Some(expand_env_vars_in_string(api_key_env));
        }
        if let Some(api_key) = &self.api_key {
            self.api_key = Some(expand_env_vars_in_string(api_key));
        }
        if let Some(base_url) = &self.base_url {
            self.base_url = Some(expand_env_vars_in_string(base_url));
        }
        if let Some(organization) = &self.organization {
            self.organization = Some(expand_env_vars_in_string(organization));
        }
        if let Some(project) = &self.project {
            self.project = Some(expand_env_vars_in_string(project));
        }
    }
}

impl HooksConfig {
    fn expand_env_vars(&mut self) {
        for hook in self
            .agent_spawn
            .iter_mut()
            .chain(self.user_prompt_submit.iter_mut())
            .chain(self.pre_tool_use.iter_mut())
            .chain(self.post_tool_use.iter_mut())
        {
            hook.command = expand_env_vars_in_string(&hook.command);
            if let Some(matcher) = &hook.matcher {
                hook.matcher = Some(expand_env_vars_in_string(matcher));
            }
            if let Some(on_error) = &hook.on_error {
                hook.on_error = Some(expand_env_vars_in_string(on_error));
            }
        }
    }
}

impl AuthConfig {
    fn expand_env_vars(&mut self) {
        if let Some(value) = &self.anthropic_api_key_env {
            self.anthropic_api_key_env = Some(expand_env_vars_in_string(value));
        }
        if let Some(value) = &self.openai_api_key_env {
            self.openai_api_key_env = Some(expand_env_vars_in_string(value));
        }
        if let Some(value) = &self.google_api_key_env {
            self.google_api_key_env = Some(expand_env_vars_in_string(value));
        }
        if let Some(value) = &self.token_store {
            self.token_store = Some(expand_env_vars_in_string(value));
        }
        if let Some(value) = &self.session_path {
            self.session_path = Some(expand_env_vars_in_string(value));
        }
    }
}

impl SecurityConfig {
    fn expand_env_vars(&mut self) {
        if let Some(value) = &self.audit_log {
            self.audit_log = Some(expand_env_vars_in_string(value));
        }
        if let Some(blocked_paths) = &mut self.blocked_paths {
            for item in blocked_paths.iter_mut() {
                *item = expand_env_vars_in_string(item);
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
    use std::sync::{Mutex, MutexGuard, OnceLock};

    fn env_lock() -> MutexGuard<'static, ()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(())).lock().unwrap()
    }

    #[test]
    fn default_model_config_values() {
        let mc = ModelConfig::default();
        assert_eq!(mc.provider, "anthropic");
        assert_eq!(mc.default, "claude-sonnet-4-20250514");
        assert_eq!(mc.max_tokens, Some(8192));
        assert_eq!(mc.effective_max_tokens(), Some(8192));
        assert!(mc.temperature.is_none());
        assert!(mc.reasoning_effort.is_none());
        assert!(mc.cache_prompts.is_none());
        assert!(mc.backend.is_none());
        assert!(mc.name.is_none());
        assert!(mc.backend_url.is_none());
        assert!(mc.parameters.is_none());
        assert!(mc.anthropic.is_none());
        assert!(mc.openai.is_none());
        assert!(mc.google.is_none());
        assert!(mc.local.is_none());
        assert!(mc.providers.is_empty());
    }

    #[test]
    fn default_config_has_default_model() {
        let cfg = Config::default();
        assert_eq!(cfg.model.provider, "anthropic");
        assert!(cfg.permissions.is_none());
        assert!(cfg.sandbox.is_none());
        assert!(cfg.hooks.is_none());
        assert!(cfg.auth.is_none());
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
    fn loads_config_with_model_parameters_and_providers() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("tengu.toml");
        let mut f = std::fs::File::create(&path).unwrap();
        writeln!(
            f,
            r#"
[model]
provider = "anthropic"
default = "claude-sonnet-4-20250514"
temperature = 0.2

[model.parameters]
max_tokens = 16384
temperature = 0.7
reasoning_effort = "high"
cache_prompts = true

[model.anthropic]
api_key_env = "ANTHROPIC_API_KEY"
base_url = "https://api.anthropic.com"
max_tokens = 8192

[model.openai]
api_key_env = "OPENAI_API_KEY"
base_url = "https://api.openai.com/v1"
organization = "org_123"

[model.local]
base_url = "http://localhost:1234/v1"
api_key = "not-needed"

[model.providers.groq]
api_key_env = "GROQ_API_KEY"
base_url = "https://api.groq.com/openai/v1"
"#
        )
        .unwrap();

        let cfg = Config::load(&path).unwrap();
        assert_eq!(cfg.model.temperature, Some(0.2));
        assert_eq!(cfg.model.effective_max_tokens(), Some(16384));
        let params = cfg.model.parameters.unwrap();
        assert_eq!(params.temperature, Some(0.7));
        assert_eq!(params.reasoning_effort.as_deref(), Some("high"));
        assert_eq!(params.cache_prompts, Some(true));
        let anthropic = cfg.model.anthropic.unwrap();
        assert_eq!(anthropic.api_key_env.as_deref(), Some("ANTHROPIC_API_KEY"));
        assert_eq!(
            anthropic.base_url.as_deref(),
            Some("https://api.anthropic.com")
        );
        assert_eq!(anthropic.max_tokens, Some(8192));
        let openai = cfg.model.openai.unwrap();
        assert_eq!(openai.organization.as_deref(), Some("org_123"));
        let local = cfg.model.local.unwrap();
        assert_eq!(local.api_key.as_deref(), Some("not-needed"));
        assert_eq!(
            cfg.model
                .providers
                .get("groq")
                .and_then(|provider| provider.base_url.as_deref()),
            Some("https://api.groq.com/openai/v1")
        );
    }

    #[test]
    fn loads_config_with_auth() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("tengu.toml");
        let mut f = std::fs::File::create(&path).unwrap();
        writeln!(
            f,
            r#"
[auth]
anthropic_api_key_env = "ANTHROPIC_API_KEY"
openai_api_key_env = "OPENAI_API_KEY"
google_api_key_env = "GOOGLE_API_KEY"
token_store = "$HOME/.tengu/auth/tokens.json"
session_path = "$HOME/.tengu/auth/session.json"
oauth_enabled = false
"#
        )
        .unwrap();

        let cfg = Config::load(&path).unwrap();
        let auth = cfg.auth.unwrap();
        assert_eq!(
            auth.anthropic_api_key_env.as_deref(),
            Some("ANTHROPIC_API_KEY")
        );
        assert_eq!(auth.openai_api_key_env.as_deref(), Some("OPENAI_API_KEY"));
        assert_eq!(auth.google_api_key_env.as_deref(), Some("GOOGLE_API_KEY"));
        assert_eq!(auth.oauth_enabled, Some(false));
        assert!(auth
            .token_store
            .as_deref()
            .unwrap()
            .contains(".tengu/auth/tokens.json"));
        assert!(auth
            .session_path
            .as_deref()
            .unwrap()
            .contains(".tengu/auth/session.json"));
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
    fn loads_config_with_security() {
        let _lock = env_lock();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("tengu.toml");
        let mut f = std::fs::File::create(&path).unwrap();
        writeln!(
            f,
            r#"
[security]
audit_log = "$TENGU_TEST_AUDIT_LOG"
audit_enabled = true
allow_env_files = false
blocked_paths = ["secrets/**"]
"#
        )
        .unwrap();

        std::env::set_var("TENGU_TEST_AUDIT_LOG", "./audit.log");
        let cfg = Config::load(&path).unwrap();
        std::env::remove_var("TENGU_TEST_AUDIT_LOG");

        let security = cfg.security.unwrap();
        assert_eq!(security.audit_log.as_deref(), Some("./audit.log"));
        assert_eq!(security.audit_enabled, Some(true));
        assert_eq!(security.allow_env_files, Some(false));
        assert_eq!(security.blocked_paths.unwrap(), vec!["secrets/**"]);
    }

    #[test]
    fn loads_config_with_hooks() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("tengu.toml");
        let mut f = std::fs::File::create(&path).unwrap();
        writeln!(
            f,
            r#"
[[hooks.preToolUse]]
matcher = "Bash(git *)"
command = "echo $tool"
timeout_ms = 5000
on_error = "fail"

[[hooks.postToolUse]]
matcher = "Write(*.rs)"
command = "cargo fmt"
cache_ttl_seconds = 300
"#
        )
        .unwrap();

        let cfg = Config::load(&path).unwrap();
        let hooks = cfg.hooks.unwrap();
        assert_eq!(hooks.pre_tool_use.len(), 1);
        assert_eq!(
            hooks.pre_tool_use[0].matcher.as_deref(),
            Some("Bash(git *)")
        );
        assert_eq!(hooks.pre_tool_use[0].timeout_ms, Some(5000));
        assert_eq!(hooks.pre_tool_use[0].on_error.as_deref(), Some("fail"));
        assert_eq!(hooks.post_tool_use.len(), 1);
        assert_eq!(hooks.post_tool_use[0].cache_ttl_seconds, Some(300));
    }

    #[test]
    fn expands_env_vars_across_optional_sections() {
        let _lock = env_lock();
        std::env::set_var("TENGU_CFG_PROVIDER", "openai");
        std::env::set_var("TENGU_CFG_DEFAULT_MODEL", "gpt-test");
        std::env::set_var("TENGU_CFG_BACKEND", "responses");
        std::env::set_var("TENGU_CFG_MODEL_NAME", "custom-name");
        std::env::set_var("TENGU_CFG_BACKEND_URL", "http://backend.test");
        std::env::set_var("TENGU_CFG_REASONING", "medium");
        std::env::set_var("TENGU_CFG_PARAM_REASONING", "high");
        std::env::set_var("TENGU_CFG_API_KEY_ENV", "OPENAI_API_KEY");
        std::env::set_var("TENGU_CFG_API_KEY", "sk-test");
        std::env::set_var("TENGU_CFG_BASE_URL", "https://api.test/v1");
        std::env::set_var("TENGU_CFG_ORG", "org-test");
        std::env::set_var("TENGU_CFG_PROJECT", "proj-test");
        std::env::set_var("TENGU_CFG_APPROVAL", "on-request");
        std::env::set_var("TENGU_CFG_ALLOWED_TOOL", "Read");
        std::env::set_var("TENGU_CFG_DENY_TOOL", "Bash");
        std::env::set_var("TENGU_CFG_SANDBOX", "workspace-write");
        std::env::set_var("TENGU_CFG_ALLOWED_PATH", "/work");
        std::env::set_var("TENGU_CFG_BLOCKED_PATH", "/secret");
        std::env::set_var("TENGU_CFG_HOOK_CMD", "cargo test");
        std::env::set_var("TENGU_CFG_HOOK_MATCHER", "Bash(*)");
        std::env::set_var("TENGU_CFG_HOOK_ERROR", "warn");
        std::env::set_var("TENGU_CFG_ANTHROPIC_ENV", "ANTHROPIC_API_KEY");
        std::env::set_var("TENGU_CFG_OPENAI_ENV", "OPENAI_API_KEY");
        std::env::set_var("TENGU_CFG_GOOGLE_ENV", "GOOGLE_API_KEY");
        std::env::set_var("TENGU_CFG_TOKEN_STORE", "/tmp/tokens.json");
        std::env::set_var("TENGU_CFG_SESSION_PATH", "/tmp/session.json");
        std::env::set_var("TENGU_CFG_AUDIT_LOG", "/tmp/audit.log");
        std::env::set_var("TENGU_CFG_SECURITY_BLOCK", "/tmp/private");

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("tengu.toml");
        std::fs::write(
            &path,
            r#"
[model]
provider = "$TENGU_CFG_PROVIDER"
default = "${TENGU_CFG_DEFAULT_MODEL}"
backend = "$TENGU_CFG_BACKEND"
name = "$TENGU_CFG_MODEL_NAME"
backend_url = "$TENGU_CFG_BACKEND_URL"
reasoning_effort = "$TENGU_CFG_REASONING"

[model.parameters]
reasoning_effort = "$TENGU_CFG_PARAM_REASONING"

[model.openai]
api_key_env = "$TENGU_CFG_API_KEY_ENV"
api_key = "$TENGU_CFG_API_KEY"
base_url = "$TENGU_CFG_BASE_URL"
organization = "$TENGU_CFG_ORG"
project = "$TENGU_CFG_PROJECT"

[permissions]
approval_policy = "$TENGU_CFG_APPROVAL"
allowed_tools = ["$TENGU_CFG_ALLOWED_TOOL"]
deny = ["$TENGU_CFG_DENY_TOOL"]

[sandbox]
mode = "$TENGU_CFG_SANDBOX"
allowed_paths = ["$TENGU_CFG_ALLOWED_PATH"]
blocked_paths = ["$TENGU_CFG_BLOCKED_PATH"]

[[hooks.agentSpawn]]
command = "$TENGU_CFG_HOOK_CMD"
matcher = "$TENGU_CFG_HOOK_MATCHER"
on_error = "$TENGU_CFG_HOOK_ERROR"

[auth]
anthropic_api_key_env = "$TENGU_CFG_ANTHROPIC_ENV"
openai_api_key_env = "$TENGU_CFG_OPENAI_ENV"
google_api_key_env = "$TENGU_CFG_GOOGLE_ENV"
token_store = "$TENGU_CFG_TOKEN_STORE"
session_path = "$TENGU_CFG_SESSION_PATH"

[security]
audit_log = "$TENGU_CFG_AUDIT_LOG"
blocked_paths = ["$TENGU_CFG_SECURITY_BLOCK"]
"#,
        )
        .unwrap();

        let cfg = Config::load(&path).unwrap();

        assert_eq!(cfg.model.provider, "openai");
        assert_eq!(cfg.model.default, "gpt-test");
        assert_eq!(cfg.model.backend.as_deref(), Some("responses"));
        assert_eq!(cfg.model.name.as_deref(), Some("custom-name"));
        assert_eq!(
            cfg.model.backend_url.as_deref(),
            Some("http://backend.test")
        );
        assert_eq!(cfg.model.reasoning_effort.as_deref(), Some("medium"));
        assert_eq!(
            cfg.model
                .parameters
                .as_ref()
                .and_then(|p| p.reasoning_effort.as_deref()),
            Some("high")
        );
        let openai = cfg.model.openai.as_ref().unwrap();
        assert_eq!(openai.api_key_env.as_deref(), Some("OPENAI_API_KEY"));
        assert_eq!(openai.api_key.as_deref(), Some("sk-test"));
        assert_eq!(openai.base_url.as_deref(), Some("https://api.test/v1"));
        assert_eq!(openai.organization.as_deref(), Some("org-test"));
        assert_eq!(openai.project.as_deref(), Some("proj-test"));

        let permissions = cfg.permissions.as_ref().unwrap();
        assert_eq!(permissions.approval_policy.as_deref(), Some("on-request"));
        assert_eq!(
            permissions.allowed_tools.as_deref(),
            Some(&["Read".to_string()][..])
        );
        assert_eq!(permissions.deny.as_deref(), Some(&["Bash".to_string()][..]));

        let sandbox = cfg.sandbox.as_ref().unwrap();
        assert_eq!(sandbox.mode.as_deref(), Some("workspace-write"));
        assert_eq!(
            sandbox.allowed_paths.as_deref(),
            Some(&["/work".to_string()][..])
        );
        assert_eq!(
            sandbox.blocked_paths.as_deref(),
            Some(&["/secret".to_string()][..])
        );

        let hook = &cfg.hooks.as_ref().unwrap().agent_spawn[0];
        assert_eq!(hook.command, "cargo test");
        assert_eq!(hook.matcher.as_deref(), Some("Bash(*)"));
        assert_eq!(hook.on_error.as_deref(), Some("warn"));

        let auth = cfg.auth.as_ref().unwrap();
        assert_eq!(
            auth.anthropic_api_key_env.as_deref(),
            Some("ANTHROPIC_API_KEY")
        );
        assert_eq!(auth.openai_api_key_env.as_deref(), Some("OPENAI_API_KEY"));
        assert_eq!(auth.google_api_key_env.as_deref(), Some("GOOGLE_API_KEY"));
        assert_eq!(auth.token_store.as_deref(), Some("/tmp/tokens.json"));
        assert_eq!(auth.session_path.as_deref(), Some("/tmp/session.json"));

        let security = cfg.security.as_ref().unwrap();
        assert_eq!(security.audit_log.as_deref(), Some("/tmp/audit.log"));
        assert_eq!(
            security.blocked_paths.as_deref(),
            Some(&["/tmp/private".to_string()][..])
        );

        for key in [
            "TENGU_CFG_PROVIDER",
            "TENGU_CFG_DEFAULT_MODEL",
            "TENGU_CFG_BACKEND",
            "TENGU_CFG_MODEL_NAME",
            "TENGU_CFG_BACKEND_URL",
            "TENGU_CFG_REASONING",
            "TENGU_CFG_PARAM_REASONING",
            "TENGU_CFG_API_KEY_ENV",
            "TENGU_CFG_API_KEY",
            "TENGU_CFG_BASE_URL",
            "TENGU_CFG_ORG",
            "TENGU_CFG_PROJECT",
            "TENGU_CFG_APPROVAL",
            "TENGU_CFG_ALLOWED_TOOL",
            "TENGU_CFG_DENY_TOOL",
            "TENGU_CFG_SANDBOX",
            "TENGU_CFG_ALLOWED_PATH",
            "TENGU_CFG_BLOCKED_PATH",
            "TENGU_CFG_HOOK_CMD",
            "TENGU_CFG_HOOK_MATCHER",
            "TENGU_CFG_HOOK_ERROR",
            "TENGU_CFG_ANTHROPIC_ENV",
            "TENGU_CFG_OPENAI_ENV",
            "TENGU_CFG_GOOGLE_ENV",
            "TENGU_CFG_TOKEN_STORE",
            "TENGU_CFG_SESSION_PATH",
            "TENGU_CFG_AUDIT_LOG",
            "TENGU_CFG_SECURITY_BLOCK",
        ] {
            std::env::remove_var(key);
        }
    }

    #[test]
    fn expands_env_vars_handles_present_sections_with_omitted_optional_values() {
        let _lock = env_lock();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("tengu.toml");
        std::fs::write(
            &path,
            r#"
[permissions]

[sandbox]

[model.parameters]
max_tokens = 1024

[model.openai]
max_tokens = 2048

[[hooks.agentSpawn]]
command = "echo ready"

[auth]
oauth_enabled = true

[security]
audit_enabled = true
"#,
        )
        .unwrap();

        let cfg = Config::load(&path).unwrap();

        assert!(cfg.permissions.unwrap().approval_policy.is_none());
        assert!(cfg.sandbox.unwrap().mode.is_none());
        assert_eq!(cfg.model.parameters.unwrap().max_tokens, Some(1024));
        assert_eq!(cfg.model.openai.unwrap().max_tokens, Some(2048));
        assert_eq!(cfg.hooks.unwrap().agent_spawn[0].command, "echo ready");
        assert_eq!(cfg.auth.unwrap().oauth_enabled, Some(true));
        assert_eq!(cfg.security.unwrap().audit_enabled, Some(true));
    }

    #[test]
    fn expand_env_vars_handles_unclosed_braces_and_unbraced_suffix() {
        let _lock = env_lock();
        std::env::set_var("TENGU_TEST_SUFFIX", "value");

        assert_eq!(expand_env_vars_in_string("${TENGU_TEST_SUFFIX"), "value");
        assert_eq!(
            expand_env_vars_in_string("$TENGU_TEST_SUFFIX-rest"),
            "value-rest"
        );

        std::env::remove_var("TENGU_TEST_SUFFIX");
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
        let _lock = env_lock();
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
        let _lock = env_lock();
        std::env::set_var("TENGU_TEST_MODEL", "my-model");
        let result = expand_env_vars_in_string("${TENGU_TEST_MODEL}");
        assert_eq!(result, "my-model");
        std::env::remove_var("TENGU_TEST_MODEL");
    }

    #[test]
    fn preserves_unset_env_vars() {
        let _lock = env_lock();
        let result = expand_env_vars_in_string("$NONEXISTENT_TENGU_VAR_XYZ");
        assert_eq!(result, "$NONEXISTENT_TENGU_VAR_XYZ");
    }

    #[test]
    fn preserves_unset_braces_env_vars() {
        let _lock = env_lock();
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
