use anyhow::{anyhow, Result};
use bytes::Bytes;
use futures_util::stream::{self, BoxStream, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::llm::{
    LlmBackend, LlmProvider, LlmRequest, LlmResponse, LlmStream, LlmStreamEvent, LlmUsage,
};

const DEFAULT_GOOGLE_BASE_URL: &str = "https://generativelanguage.googleapis.com/v1beta";

#[derive(Debug, Clone)]
pub struct GoogleBackend {
    pub base_url: String,
}

#[derive(Debug, Serialize)]
struct GenerateContentRequest {
    contents: Vec<GoogleContent>,
}

#[derive(Debug, Serialize)]
struct GoogleContent {
    parts: Vec<GooglePart>,
}

#[derive(Debug, Serialize)]
struct GooglePart {
    #[serde(skip_serializing_if = "Option::is_none")]
    text: Option<String>,
    #[serde(rename = "inlineData", skip_serializing_if = "Option::is_none")]
    inline_data: Option<GoogleInlineData>,
}

#[derive(Debug, Serialize)]
struct GoogleInlineData {
    #[serde(rename = "mimeType")]
    mime_type: String,
    data: String,
}

#[derive(Debug, Deserialize)]
struct GenerateContentResponse {
    candidates: Option<Vec<GoogleCandidate>>,
    #[serde(rename = "usageMetadata", default)]
    usage_metadata: Option<GoogleUsageMetadata>,
}

#[derive(Debug, Deserialize, Clone)]
struct GoogleUsageMetadata {
    #[serde(rename = "promptTokenCount", default)]
    prompt_token_count: Option<u64>,
    #[serde(rename = "candidatesTokenCount", default)]
    candidates_token_count: Option<u64>,
    #[serde(rename = "totalTokenCount", default)]
    total_token_count: Option<u64>,
    #[serde(rename = "cachedContentTokenCount", default)]
    cached_content_token_count: Option<u64>,
    #[serde(rename = "thoughtsTokenCount", default)]
    thoughts_token_count: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct GoogleCandidate {
    content: Option<GoogleContentResponse>,
}

#[derive(Debug, Deserialize)]
struct GoogleContentResponse {
    parts: Option<Vec<GooglePartResponse>>,
}

#[derive(Debug, Deserialize)]
struct GooglePartResponse {
    text: Option<String>,
}

impl GoogleBackend {
    pub fn new(base_url: Option<String>) -> Self {
        Self {
            base_url: base_url.unwrap_or_else(|| DEFAULT_GOOGLE_BASE_URL.to_string()),
        }
    }

    fn api_key(&self) -> Result<String> {
        std::env::var("GOOGLE_API_KEY").map_err(|_| anyhow!("GOOGLE_API_KEY is not set"))
    }

    fn request_body(&self, request: &LlmRequest) -> GenerateContentRequest {
        let mut parts = vec![GooglePart {
            text: Some(request.prompt.clone()),
            inline_data: None,
        }];
        for image in &request.images {
            parts.push(GooglePart {
                text: None,
                inline_data: Some(GoogleInlineData {
                    mime_type: image.media_type.clone(),
                    data: image.data_base64.clone(),
                }),
            });
        }
        GenerateContentRequest {
            contents: vec![GoogleContent { parts }],
        }
    }

    fn generate_url(&self, model: &str, stream: bool, api_key: &str) -> String {
        let base = self.base_url.trim_end_matches('/');
        let method = if stream {
            "streamGenerateContent"
        } else {
            "generateContent"
        };
        let mut url = format!("{}/models/{}:{}", base, model, method);
        if stream {
            url.push_str("?alt=sse");
            url.push('&');
        } else {
            url.push('?');
        }
        url.push_str("key=");
        url.push_str(api_key);
        url
    }

    fn extract_text(value: &Value) -> String {
        value
            .get("candidates")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|candidate| candidate.get("content"))
            .filter_map(|content| content.get("parts"))
            .filter_map(Value::as_array)
            .flat_map(|parts| parts.iter())
            .filter_map(|part| part.get("text"))
            .filter_map(Value::as_str)
            .collect::<Vec<_>>()
            .join("")
    }

    fn normalize_usage(usage: GoogleUsageMetadata, raw: Option<Value>) -> LlmUsage {
        LlmUsage {
            provider: "google".to_string(),
            input_tokens: usage.prompt_token_count,
            output_tokens: usage.candidates_token_count,
            total_tokens: usage.total_token_count.or_else(|| {
                match (usage.prompt_token_count, usage.candidates_token_count) {
                    (Some(input), Some(output)) => Some(input + output),
                    _ => None,
                }
            }),
            cache_creation_input_tokens: None,
            cache_read_input_tokens: usage.cached_content_token_count,
            reasoning_tokens: usage.thoughts_token_count,
            raw,
        }
    }

    fn parse_stream_data(data: &str) -> Result<Option<LlmStreamEvent>> {
        let payload = data.trim();
        if payload.is_empty() || payload == "[DONE]" {
            return Ok(None);
        }
        let value: Value = serde_json::from_str(payload)?;
        if let Some(error) = value
            .get("error")
            .and_then(|err| err.get("message"))
            .and_then(Value::as_str)
        {
            return Err(anyhow!("google stream error: {}", error));
        }
        if let Some(usage_value) = value.get("usageMetadata").cloned() {
            if let Ok(usage) = serde_json::from_value::<GoogleUsageMetadata>(usage_value.clone()) {
                return Ok(Some(LlmStreamEvent::Usage(Self::normalize_usage(
                    usage,
                    Some(usage_value),
                ))));
            }
        }
        let text = Self::extract_text(&value);
        if text.is_empty() {
            Ok(None)
        } else {
            Ok(Some(LlmStreamEvent::Text(text)))
        }
    }
}

#[async_trait::async_trait]
impl LlmBackend for GoogleBackend {
    fn provider(&self) -> LlmProvider {
        LlmProvider::Google
    }

    async fn generate(&self, model: &str, request: &LlmRequest) -> Result<LlmResponse> {
        let api_key = self.api_key()?;
        let client = reqwest::Client::new();
        let response = client
            .post(self.generate_url(model, false, &api_key))
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .json(&self.request_body(request))
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(anyhow!("google error: {} {}", status, body.trim()));
        }

        let body: GenerateContentResponse = response.json().await?;
        let content = body
            .candidates
            .unwrap_or_default()
            .into_iter()
            .find_map(|candidate| candidate.content)
            .and_then(|content| content.parts)
            .unwrap_or_default()
            .into_iter()
            .filter_map(|part| part.text)
            .collect::<Vec<_>>()
            .join("");
        Ok(LlmResponse {
            content,
            usage: body
                .usage_metadata
                .map(|usage| Self::normalize_usage(usage, None)),
        })
    }

    async fn generate_stream(&self, model: &str, request: &LlmRequest) -> Result<LlmStream> {
        let api_key = self.api_key()?;
        let client = reqwest::Client::new();
        let response = client
            .post(self.generate_url(model, true, &api_key))
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .json(&self.request_body(request))
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(anyhow!("google error: {} {}", status, body.trim()));
        }

        struct StreamState {
            stream: BoxStream<'static, Result<Bytes, reqwest::Error>>,
            buffer: String,
            pending_data: Vec<String>,
            finished: bool,
        }

        fn take_message(state: &mut StreamState) -> Result<Option<LlmStreamEvent>> {
            while let Some(idx) = state.buffer.find('\n') {
                let mut line = state.buffer[..idx].to_string();
                state.buffer = state.buffer[idx + 1..].to_string();
                if line.ends_with('\r') {
                    line.pop();
                }

                if line.is_empty() {
                    if !state.pending_data.is_empty() {
                        let data = state.pending_data.join("\n");
                        state.pending_data.clear();
                        return GoogleBackend::parse_stream_data(&data);
                    }
                    continue;
                }

                if let Some(data) = line.strip_prefix("data:") {
                    state.pending_data.push(data.trim_start().to_string());
                }
            }

            Ok(None)
        }

        let state = StreamState {
            stream: Box::pin(response.bytes_stream()),
            buffer: String::new(),
            pending_data: Vec::new(),
            finished: false,
        };

        let output = stream::unfold(state, |mut state| async move {
            if state.finished {
                return None;
            }

            loop {
                match take_message(&mut state) {
                    Ok(Some(text)) => return Some((Ok(text), state)),
                    Ok(None) => {}
                    Err(err) => {
                        state.finished = true;
                        return Some((Err(err), state));
                    }
                }

                match state.stream.next().await {
                    Some(Ok(chunk)) => {
                        state.buffer.push_str(&String::from_utf8_lossy(&chunk));
                    }
                    Some(Err(err)) => {
                        state.finished = true;
                        return Some((Err(anyhow::Error::new(err)), state));
                    }
                    None => {
                        state.finished = true;
                        if !state.pending_data.is_empty() {
                            let data = state.pending_data.join("\n");
                            match GoogleBackend::parse_stream_data(&data) {
                                Ok(Some(text)) => return Some((Ok(text), state)),
                                Ok(None) => return None,
                                Err(err) => return Some((Err(err), state)),
                            }
                        }
                        return None;
                    }
                }
            }
        });

        Ok(Box::pin(output) as BoxStream<'static, Result<LlmStreamEvent>>)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_generate_url_for_streaming() {
        let backend = GoogleBackend::new(None);
        let url = backend.generate_url("gemini-2.0-flash", true, "test-key");
        assert_eq!(
            url,
            "https://generativelanguage.googleapis.com/v1beta/models/gemini-2.0-flash:streamGenerateContent?alt=sse&key=test-key"
        );
    }

    #[test]
    fn parses_text_from_stream_payload() {
        let payload = r#"{"candidates":[{"content":{"parts":[{"text":"hello"}]}}]}"#;
        let parsed = GoogleBackend::parse_stream_data(payload).unwrap();
        assert!(matches!(parsed, Some(LlmStreamEvent::Text(text)) if text == "hello"));
    }

    #[test]
    fn parses_usage_from_stream_payload() {
        let payload = r#"{"usageMetadata":{"promptTokenCount":21,"candidatesTokenCount":8,"totalTokenCount":29,"cachedContentTokenCount":5,"thoughtsTokenCount":3}}"#;
        let parsed = GoogleBackend::parse_stream_data(payload).unwrap();
        assert!(matches!(
            parsed,
            Some(LlmStreamEvent::Usage(usage))
                if usage.provider == "google"
                    && usage.input_tokens == Some(21)
                    && usage.output_tokens == Some(8)
                    && usage.total_tokens == Some(29)
                    && usage.cache_read_input_tokens == Some(5)
                    && usage.reasoning_tokens == Some(3)
        ));
    }
}
