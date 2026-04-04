use crate::{ChatMessage, Provider, ProviderConfig, Role, StreamEvent};
use eventsource_stream::Eventsource;
use futures_util::StreamExt;
use serde_json::{json, Value};
use tokio::sync::mpsc;

/// Anthropic Claude provider.
///
/// Key differences from OpenAI:
/// - Auth via `x-api-key` header (not `Authorization: Bearer`)
/// - Required `anthropic-version` header
/// - System prompt is a top-level `system` field, not a message
/// - SSE events are typed via the `event:` field
pub struct AnthropicProvider {
    client: reqwest::Client,
}

impl AnthropicProvider {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
        }
    }
}

impl Default for AnthropicProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl Provider for AnthropicProvider {
    async fn stream_chat(
        &self,
        config: &ProviderConfig,
        messages: &[ChatMessage],
        tx: mpsc::Sender<StreamEvent>,
    ) {
        let url = format!("{}/messages", config.base_url);

        // Build messages — Anthropic does NOT support "system" role in messages.
        // Only "user" and "assistant" roles are allowed.
        let msgs: Vec<Value> = messages
            .iter()
            .filter(|m| m.role != Role::System)
            .map(|m| {
                let role = match m.role {
                    Role::User | Role::System => "user",
                    Role::Assistant => "assistant",
                };
                json!({
                    "role": role,
                    "content": &m.content
                })
            })
            .collect();

        let mut body = json!({
            "model": &config.model,
            "messages": msgs,
            "max_tokens": config.max_tokens,
            "temperature": config.temperature,
            "stream": true
        });

        // System prompt goes as a top-level field
        if let Some(ref system) = config.system_prompt {
            body.as_object_mut()
                .unwrap()
                .insert("system".to_string(), json!(system));
        }

        let mut request = self
            .client
            .post(&url)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&body);

        if let Some(ref key) = config.api_key {
            request = request.header("x-api-key", key);
        }

        let response = match request.send().await {
            Ok(resp) => {
                if !resp.status().is_success() {
                    let status = resp.status();
                    let body = resp.text().await.unwrap_or_default();
                    let _ = tx
                        .send(StreamEvent::Error(format!(
                            "Anthropic API error {status}: {body}"
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
                    // Anthropic uses typed SSE events
                    match ev.event.as_str() {
                        "content_block_delta" => {
                            if let Ok(parsed) = serde_json::from_str::<Value>(&ev.data) {
                                if let Some(text) = parsed
                                    .get("delta")
                                    .and_then(|d| d.get("text"))
                                    .and_then(|v| v.as_str())
                                {
                                    if !text.is_empty() {
                                        if tx
                                            .send(StreamEvent::Token(text.to_string()))
                                            .await
                                            .is_err()
                                        {
                                            return;
                                        }
                                    }
                                }
                            }
                        }
                        "message_stop" => {
                            break;
                        }
                        // Ignore other event types (message_start, content_block_start,
                        // content_block_stop, ping, etc.)
                        _ => {}
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
        "anthropic"
    }
}
