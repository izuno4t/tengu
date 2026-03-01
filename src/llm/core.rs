use anyhow::{anyhow, Result};
use futures_util::stream::BoxStream;

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

#[derive(Debug, Clone)]
pub struct LlmImage {
    pub media_type: String,
    pub data_base64: String,
}

#[derive(Debug, Clone)]
pub struct LlmRequest {
    pub prompt: String,
    pub images: Vec<LlmImage>,
}

impl LlmRequest {
    pub fn text(prompt: impl Into<String>) -> Self {
        Self {
            prompt: prompt.into(),
            images: Vec::new(),
        }
    }
}

pub type LlmStream = BoxStream<'static, Result<String>>;

pub struct LlmClient {
    backend: Box<dyn LlmBackend + Send + Sync>,
}

impl LlmClient {
    pub fn new(backend: Box<dyn LlmBackend + Send + Sync>) -> Self {
        Self { backend }
    }

    #[allow(dead_code)]
    pub fn provider(&self) -> LlmProvider {
        self.backend.provider()
    }

    pub async fn generate(&self, model: &str, request: &LlmRequest) -> Result<LlmResponse> {
        self.backend.generate(model, request).await
    }

    pub async fn generate_stream(&self, model: &str, request: &LlmRequest) -> Result<LlmStream> {
        self.backend.generate_stream(model, request).await
    }
}

#[async_trait::async_trait]
pub trait LlmBackend {
    #[allow(dead_code)]
    fn provider(&self) -> LlmProvider;
    async fn generate(&self, model: &str, request: &LlmRequest) -> Result<LlmResponse>;
    async fn generate_stream(&self, model: &str, request: &LlmRequest) -> Result<LlmStream>;
}
