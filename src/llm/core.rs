use anyhow::{anyhow, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LlmProvider {
    Anthropic,
    OpenAI,
    Google,
    Local,
}

impl LlmProvider {
    pub fn from_str(input: &str) -> Result<Self> {
        match input.trim().to_ascii_lowercase().as_str() {
            "anthropic" => Ok(Self::Anthropic),
            "openai" => Ok(Self::OpenAI),
            "google" | "gemini" => Ok(Self::Google),
            "local" | "ollama" | "lm-studio" | "lmstudio" => Ok(Self::Local),
            other => Err(anyhow!("unsupported provider: {}", other)),
        }
    }
}

#[derive(Debug, Clone)]
pub struct LlmResponse {
    pub content: String,
}

pub struct LlmClient {
    backend: Box<dyn LlmBackend + Send + Sync>,
}

impl LlmClient {
    pub fn new(backend: Box<dyn LlmBackend + Send + Sync>) -> Self {
        Self { backend }
    }

    pub fn provider(&self) -> LlmProvider {
        self.backend.provider()
    }

    pub async fn generate(&self, model: &str, prompt: &str) -> Result<LlmResponse> {
        self.backend.generate(model, prompt).await
    }
}

#[async_trait::async_trait]
pub trait LlmBackend {
    fn provider(&self) -> LlmProvider;
    async fn generate(&self, model: &str, prompt: &str) -> Result<LlmResponse>;
}
