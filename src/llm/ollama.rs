use anyhow::{anyhow, Result};
use futures_util::stream::{self, BoxStream, StreamExt};
use bytes::Bytes;
use serde::{Deserialize, Serialize};

use crate::llm::{LlmBackend, LlmProvider, LlmResponse, LlmStream};

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

#[derive(Debug, Deserialize)]
struct GenerateStreamResponse {
    response: String,
    done: bool,
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
        let response = client
            .post(self.generate_url())
            .json(&payload)
            .send()
            .await?;
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

    async fn generate_stream(&self, model: &str, prompt: &str) -> Result<LlmStream> {
        let client = reqwest::Client::new();
        let payload = GenerateRequest {
            model: model.to_string(),
            prompt: prompt.to_string(),
            stream: true,
        };
        let response = client
            .post(self.generate_url())
            .json(&payload)
            .send()
            .await?;
        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(anyhow!("ollama error: {} {}", status, body.trim()));
        }

        struct StreamState {
            stream: BoxStream<'static, Result<Bytes, reqwest::Error>>,
            buffer: String,
            done: bool,
        }

        let state = StreamState {
            stream: Box::pin(response.bytes_stream()),
            buffer: String::new(),
            done: false,
        };

        let output = stream::unfold(state, |mut state| async move {
            if state.done {
                return None;
            }
            loop {
                if let Some(idx) = state.buffer.find('\n') {
                    let line = state.buffer[..idx].to_string();
                    state.buffer = state.buffer[idx + 1..].to_string();
                    let line = line.trim();
                    if line.is_empty() {
                        continue;
                    }
                    match serde_json::from_str::<GenerateStreamResponse>(line) {
                        Ok(msg) => {
                            if msg.done {
                                if msg.response.is_empty() {
                                    return None;
                                }
                                state.done = true;
                            }
                            return Some((Ok(msg.response), state));
                        }
                        Err(err) => {
                            state.done = true;
                            return Some((Err(anyhow!("ollama stream parse error: {}", err)), state));
                        }
                    }
                }

                match state.stream.next().await {
                    Some(Ok(chunk)) => {
                        state.buffer.push_str(&String::from_utf8_lossy(&chunk));
                    }
                    Some(Err(err)) => {
                        state.done = true;
                        return Some((Err(anyhow::Error::new(err)), state));
                    }
                    None => {
                        if state.buffer.trim().is_empty() {
                            return None;
                        }
                        let line = state.buffer.trim().to_string();
                        state.buffer.clear();
                        match serde_json::from_str::<GenerateStreamResponse>(line.trim()) {
                            Ok(msg) => {
                                state.done = true;
                                return Some((Ok(msg.response), state));
                            }
                            Err(err) => {
                                state.done = true;
                                return Some((Err(anyhow!("ollama stream parse error: {}", err)), state));
                            }
                        }
                    }
                }
            }
        });

        Ok(Box::pin(output) as BoxStream<'static, Result<String>>)
    }
}
