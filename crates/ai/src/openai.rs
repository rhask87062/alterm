use crate::{ChatMessage, Provider, ProviderConfig, Role, StreamEvent};
use eventsource_stream::Eventsource;
use futures_util::StreamExt;
use serde_json::{json, Value};
use tokio::sync::mpsc;

/// OpenAI-compatible provider.
///
/// Works with any API that follows the OpenAI chat completions format:
/// - OpenAI (`https://api.openai.com/v1`)
/// - xAI Grok (`https://api.x.ai/v1`)
/// - LM Studio (`http://localhost:1234/v1`)
/// - Ollama (`http://localhost:11434/v1`)
pub struct OpenAIProvider {
    client: reqwest::Client,
}

impl OpenAIProvider {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
        }
    }
}

impl Default for OpenAIProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl Provider for OpenAIProvider {
    async fn stream_chat(
        &self,
        config: &ProviderConfig,
        messages: &[ChatMessage],
        tx: mpsc::Sender<StreamEvent>,
    ) {
        let url = format!("{}/chat/completions", config.base_url);

        // Build messages array — prepend system prompt if present
        let mut msgs: Vec<Value> = Vec::new();
        if let Some(ref system) = config.system_prompt {
            msgs.push(json!({
                "role": "system",
                "content": system
            }));
        }
        for msg in messages {
            let role = match msg.role {
                Role::System => "system",
                Role::User => "user",
                Role::Assistant => "assistant",
            };
            msgs.push(json!({
                "role": role,
                "content": &msg.content
            }));
        }

        // OpenAI/xAI: newer models (GPT-5.x, o-series) reject "max_tokens"
        // and require "max_completion_tokens". Local APIs (Ollama, LM Studio)
        // still expect "max_tokens".
        let is_cloud_openai = config.base_url.contains("api.openai.com")
            || config.base_url.contains("api.x.ai");

        let mut body = json!({
            "model": &config.model,
            "messages": msgs,
            "stream": true
        });

        if is_cloud_openai {
            body["max_completion_tokens"] = json!(config.max_tokens);
        } else {
            body["max_tokens"] = json!(config.max_tokens);
        }

        // Temperature is rejected by reasoning models (o1/o3) — only set if non-zero
        if config.temperature > 0.0 {
            body["temperature"] = json!(config.temperature);
        }

        let mut request = self.client.post(&url).json(&body);

        if let Some(ref key) = config.api_key {
            request = request.header("Authorization", format!("Bearer {key}"));
        }

        let response = match request.send().await {
            Ok(resp) => {
                if !resp.status().is_success() {
                    let status = resp.status();
                    let body = resp.text().await.unwrap_or_default();
                    let _ = tx
                        .send(StreamEvent::Error(format!(
                            "OpenAI API error {status}: {body}"
                        )))
                        .await;
                    return;
                }
                resp
            }
            Err(e) => {
                let _ = tx
                    .send(StreamEvent::Error(format!("Request failed: {e}")))
                    .await;
                return;
            }
        };

        let mut stream = response.bytes_stream().eventsource();

        while let Some(event) = stream.next().await {
            match event {
                Ok(ev) => {
                    let data = ev.data.trim().to_string();

                    // OpenAI signals end-of-stream with [DONE]
                    if data == "[DONE]" {
                        break;
                    }

                    // Parse the SSE data as JSON
                    if let Ok(parsed) = serde_json::from_str::<Value>(&data) {
                        if let Some(content) = parsed
                            .get("choices")
                            .and_then(|c| c.get(0))
                            .and_then(|c| c.get("delta"))
                            .and_then(|d| d.get("content"))
                            .and_then(|v| v.as_str())
                        {
                            if !content.is_empty() {
                                if tx.send(StreamEvent::Token(content.to_string())).await.is_err()
                                {
                                    // Receiver dropped
                                    return;
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    let _ = tx
                        .send(StreamEvent::Error(format!("SSE parse error: {e}")))
                        .await;
                    break;
                }
            }
        }

        let _ = tx.send(StreamEvent::Done).await;
    }

    fn name(&self) -> &'static str {
        "openai"
    }
}
