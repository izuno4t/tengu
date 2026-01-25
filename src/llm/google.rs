use anyhow::{anyhow, Result};

use crate::llm::{LlmBackend, LlmProvider, LlmResponse};

#[derive(Debug, Clone)]
pub struct GoogleBackend;

#[async_trait::async_trait]
impl LlmBackend for GoogleBackend {
    fn provider(&self) -> LlmProvider {
        LlmProvider::Google
    }

    async fn generate(&self, _model: &str, _prompt: &str) -> Result<LlmResponse> {
        Err(anyhow!("google backend not implemented"))
    }

    async fn generate_stream(&self, _model: &str, _prompt: &str) -> Result<crate::llm::LlmStream> {
        Err(anyhow!("google backend streaming not implemented"))
    }
}
