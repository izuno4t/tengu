// Model routing module
// 3-tier model routing with provider health tracking

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use super::LlmProvider;

/// Model complexity tier for routing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ModelTier {
    /// Complex tasks: architecture, multi-file refactoring, security review
    Strong,
    /// Normal tasks: code generation, bug fixes, explanations
    Balanced,
    /// Simple tasks: formatting, renaming, simple lookups
    Fast,
}

/// Classify prompt complexity into a tier.
pub fn classify_complexity(prompt: &str) -> ModelTier {
    let lower = prompt.to_lowercase();
    let strong_keywords = [
        "architect",
        "refactor",
        "security",
        "review",
        "design",
        "plan",
        "migrate",
        "complex",
        "multi-file",
        "system",
        // Japanese keywords
        "設計",
        "リファクタリング",
        "セキュリティ",
        "レビュー",
        "移行",
        "アーキテクチャ",
    ];
    let fast_keywords = [
        "format",
        "rename",
        "typo",
        "comment",
        "lint",
        "import",
        "simple",
        // Japanese keywords
        "フォーマット",
        "リネーム",
        "タイポ",
        "コメント",
        "インポート",
    ];

    for kw in &strong_keywords {
        if lower.contains(kw) {
            return ModelTier::Strong;
        }
    }
    for kw in &fast_keywords {
        if lower.contains(kw) {
            return ModelTier::Fast;
        }
    }

    // Default based on length heuristic
    if prompt.len() > 500 {
        ModelTier::Strong
    } else {
        ModelTier::Balanced
    }
}

/// Returns the preferred model name for a given tier and provider.
pub fn tier_models(tier: ModelTier, provider: LlmProvider) -> &'static str {
    match (tier, provider) {
        (ModelTier::Strong, LlmProvider::Anthropic) => "claude-sonnet-4-20250514",
        (ModelTier::Balanced, LlmProvider::Anthropic) => "claude-sonnet-4-20250514",
        (ModelTier::Fast, LlmProvider::Anthropic) => "claude-haiku-3-5-20241022",

        (ModelTier::Strong, LlmProvider::OpenAI) => "gpt-4o",
        (ModelTier::Balanced, LlmProvider::OpenAI) => "gpt-4o-mini",
        (ModelTier::Fast, LlmProvider::OpenAI) => "gpt-4o-mini",

        (ModelTier::Strong, LlmProvider::Google) => "gemini-2.5-pro",
        (ModelTier::Balanced, LlmProvider::Google) => "gemini-2.5-flash",
        (ModelTier::Fast, LlmProvider::Google) => "gemini-2.0-flash",

        (ModelTier::Strong, LlmProvider::Local) => "llama3.1:70b",
        (ModelTier::Balanced, LlmProvider::Local) => "llama3.1:8b",
        (ModelTier::Fast, LlmProvider::Local) => "llama3.1:8b",
    }
}

const HEALTH_COOLDOWN_SECS: u64 = 60;

/// Tracks health status for a provider.
#[derive(Debug, Clone)]
pub struct ProviderHealth {
    pub failure_count: u32,
    pub last_failure: Option<Instant>,
}

impl ProviderHealth {
    pub fn new() -> Self {
        Self {
            failure_count: 0,
            last_failure: None,
        }
    }

    pub fn record_failure(&mut self) {
        self.failure_count += 1;
        self.last_failure = Some(Instant::now());
    }

    pub fn record_success(&mut self) {
        self.failure_count = 0;
        self.last_failure = None;
    }

    pub fn is_healthy(&self) -> bool {
        match self.last_failure {
            None => true,
            Some(last) => last.elapsed() > Duration::from_secs(HEALTH_COOLDOWN_SECS),
        }
    }
}

impl Default for ProviderHealth {
    fn default() -> Self {
        Self::new()
    }
}

/// Routes model selection based on tier, provider health, and fallback logic.
pub struct ModelRouter {
    health: Arc<Mutex<HashMap<LlmProvider, ProviderHealth>>>,
    primary_provider: LlmProvider,
}

impl ModelRouter {
    pub fn new(primary_provider: LlmProvider) -> Self {
        Self {
            health: Arc::new(Mutex::new(HashMap::new())),
            primary_provider,
        }
    }

    /// Select the best model for a given prompt, considering tier and health.
    pub fn select_model(&self, prompt: &str) -> (String, LlmProvider) {
        let tier = classify_complexity(prompt);
        self.select_model_for_tier(tier)
    }

    /// Select a model for a specific tier.
    pub fn select_model_for_tier(&self, tier: ModelTier) -> (String, LlmProvider) {
        let health = self.health.lock().unwrap_or_else(|e| e.into_inner());

        // Try primary provider first
        let primary_health = health.get(&self.primary_provider);
        if primary_health.map_or(true, |h| h.is_healthy()) {
            return (
                tier_models(tier, self.primary_provider).to_string(),
                self.primary_provider,
            );
        }

        // Fallback: try other providers
        let fallback_order = [
            LlmProvider::Anthropic,
            LlmProvider::OpenAI,
            LlmProvider::Google,
            LlmProvider::Local,
        ];
        for &provider in &fallback_order {
            if provider == self.primary_provider {
                continue;
            }
            let provider_health = health.get(&provider);
            if provider_health.map_or(true, |h| h.is_healthy()) {
                return (tier_models(tier, provider).to_string(), provider);
            }
        }

        // All unhealthy: fall back to primary anyway
        (
            tier_models(tier, self.primary_provider).to_string(),
            self.primary_provider,
        )
    }

    /// Record a provider failure.
    pub fn record_failure(&self, provider: LlmProvider) {
        let mut health = self.health.lock().unwrap_or_else(|e| e.into_inner());
        health
            .entry(provider)
            .or_insert_with(ProviderHealth::new)
            .record_failure();
    }

    /// Record a provider success.
    pub fn record_success(&self, provider: LlmProvider) {
        let mut health = self.health.lock().unwrap_or_else(|e| e.into_inner());
        health
            .entry(provider)
            .or_insert_with(ProviderHealth::new)
            .record_success();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_complex_prompts() {
        assert_eq!(
            classify_complexity("Please review the security of this module"),
            ModelTier::Strong
        );
        assert_eq!(
            classify_complexity("Refactor the authentication flow"),
            ModelTier::Strong
        );
        assert_eq!(
            classify_complexity("Design a new API architecture"),
            ModelTier::Strong
        );
    }

    #[test]
    fn classify_simple_prompts() {
        assert_eq!(
            classify_complexity("Fix this typo in the README"),
            ModelTier::Fast
        );
        assert_eq!(classify_complexity("Rename this variable"), ModelTier::Fast);
        assert_eq!(classify_complexity("Format this code"), ModelTier::Fast);
    }

    #[test]
    fn classify_balanced_prompts() {
        assert_eq!(
            classify_complexity("Add a new endpoint for user login"),
            ModelTier::Balanced
        );
        assert_eq!(
            classify_complexity("Fix the bug in the parser"),
            ModelTier::Balanced
        );
    }

    #[test]
    fn classify_japanese_keywords() {
        assert_eq!(
            classify_complexity("このモジュールのセキュリティをレビューして"),
            ModelTier::Strong
        );
        assert_eq!(classify_complexity("コメントを追加して"), ModelTier::Fast);
    }

    #[test]
    fn classify_long_prompts_as_strong() {
        let long_prompt = "a".repeat(600);
        assert_eq!(classify_complexity(&long_prompt), ModelTier::Strong);
    }

    #[test]
    fn tier_models_returns_valid_names() {
        let model = tier_models(ModelTier::Strong, LlmProvider::Anthropic);
        assert!(!model.is_empty());

        let model = tier_models(ModelTier::Fast, LlmProvider::OpenAI);
        assert!(!model.is_empty());
    }

    #[test]
    fn provider_health_lifecycle() {
        let mut health = ProviderHealth::new();
        assert!(health.is_healthy());
        assert_eq!(health.failure_count, 0);

        health.record_failure();
        assert_eq!(health.failure_count, 1);
        assert!(!health.is_healthy()); // Just failed, within cooldown

        health.record_success();
        assert_eq!(health.failure_count, 0);
        assert!(health.is_healthy());
    }

    #[test]
    fn router_selects_primary_when_healthy() {
        let router = ModelRouter::new(LlmProvider::Anthropic);
        let (model, provider) = router.select_model("Fix a bug");
        assert_eq!(provider, LlmProvider::Anthropic);
        assert!(!model.is_empty());
    }

    #[test]
    fn router_falls_back_on_failure() {
        let router = ModelRouter::new(LlmProvider::Anthropic);
        router.record_failure(LlmProvider::Anthropic);

        let (_, provider) = router.select_model("Fix a bug");
        // Should fall back to another provider
        assert_ne!(provider, LlmProvider::Anthropic);
    }

    #[test]
    fn router_returns_primary_when_all_unhealthy() {
        let router = ModelRouter::new(LlmProvider::Anthropic);
        router.record_failure(LlmProvider::Anthropic);
        router.record_failure(LlmProvider::OpenAI);
        router.record_failure(LlmProvider::Google);
        router.record_failure(LlmProvider::Local);

        let (_, provider) = router.select_model("Fix a bug");
        assert_eq!(provider, LlmProvider::Anthropic);
    }

    #[test]
    fn router_select_model_for_tier() {
        let router = ModelRouter::new(LlmProvider::Anthropic);
        let (model, _) = router.select_model_for_tier(ModelTier::Strong);
        assert_eq!(model, "claude-sonnet-4-20250514");

        let (model, _) = router.select_model_for_tier(ModelTier::Fast);
        assert_eq!(model, "claude-haiku-3-5-20241022");
    }

    #[test]
    fn router_success_restores_health() {
        let router = ModelRouter::new(LlmProvider::Anthropic);
        router.record_failure(LlmProvider::Anthropic);

        let (_, provider) = router.select_model("test");
        assert_ne!(provider, LlmProvider::Anthropic);

        router.record_success(LlmProvider::Anthropic);
        let (_, provider) = router.select_model("test");
        assert_eq!(provider, LlmProvider::Anthropic);
    }
}
