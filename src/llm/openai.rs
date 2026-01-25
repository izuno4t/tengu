use anyhow::{anyhow, Result};

use crate::llm::{LlmBackend, LlmProvider, LlmResponse};

#[derive(Debug, Clone)]
pub struct OpenAiBackend;

#[async_trait::async_trait]
impl LlmBackend for OpenAiBackend {
    fn provider(&self) -> LlmProvider {
        LlmProvider::OpenAI
    }

    async fn generate(&self, _model: &str, _prompt: &str) -> Result<LlmResponse> {
        Err(anyhow!("openai backend not implemented"))
    }

    async fn generate_stream(&self, _model: &str, _prompt: &str) -> Result<crate::llm::LlmStream> {
        Err(anyhow!("openai backend streaming not implemented"))
    }
}
