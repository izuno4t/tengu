use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};

use crate::llm::{LlmBackend, LlmProvider, LlmResponse};

#[derive(Debug, Clone)]
pub struct OllamaBackend {
    pub base_url: String,
}

#[derive(Debug, Serialize)]
struct GenerateRequest {
    model: String,
    prompt: String,
    stream: bool,
}

#[derive(Debug, Deserialize)]
struct GenerateResponse {
    response: String,
}

impl OllamaBackend {
    pub fn new(base_url: String) -> Self {
        Self { base_url }
    }

    fn generate_url(&self) -> String {
        let base = self.base_url.trim_end_matches('/');
        if base.ends_with("/api") {
            format!("{}/generate", base)
        } else {
            format!("{}/api/generate", base)
        }
    }
}

#[async_trait::async_trait]
impl LlmBackend for OllamaBackend {
    fn provider(&self) -> LlmProvider {
        LlmProvider::Local
    }

    async fn generate(&self, model: &str, prompt: &str) -> Result<LlmResponse> {
        let client = reqwest::Client::new();
        let payload = GenerateRequest {
            model: model.to_string(),
            prompt: prompt.to_string(),
            stream: false,
        };
        let response = client.post(self.generate_url()).json(&payload).send().await?;
        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(anyhow!("ollama error: {} {}", status, body.trim()));
        }
        let body: GenerateResponse = response.json().await?;
        Ok(LlmResponse {
            content: body.response,
        })
    }
}
