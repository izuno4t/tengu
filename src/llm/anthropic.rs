use anyhow::{anyhow, Result};

use crate::llm::{LlmBackend, LlmProvider, LlmResponse};

#[derive(Debug, Clone)]
pub struct AnthropicBackend;

#[async_trait::async_trait]
impl LlmBackend for AnthropicBackend {
    fn provider(&self) -> LlmProvider {
        LlmProvider::Anthropic
    }

    async fn generate(&self, _model: &str, _prompt: &str) -> Result<LlmResponse> {
        Err(anyhow!("anthropic backend not implemented"))
    }
}
