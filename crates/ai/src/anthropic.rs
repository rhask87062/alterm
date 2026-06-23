use crate::{ChatMessage, Provider, ProviderConfig, Role, StreamEvent};
use eventsource_stream::Eventsource;
use futures_util::StreamExt;
use serde_json::{json, Value};
use tokio::sync::mpsc;

/// Anthropic models that removed sampling parameters (`temperature`, `top_p`,
/// `top_k`). Sending `temperature` to these returns HTTP 400. Match by
/// substring so suffixed/aliased ids (e.g. dated snapshots) are covered.
const NO_TEMPERATURE_MODELS: &[&str] = &["opus-4-7", "opus-4-8"];

/// Whether the given Anthropic model accepts a `temperature` parameter.
/// Opus 4.7+ removed it; everything else (Opus 4.6 and earlier, Sonnet, Haiku)
/// still accepts it.
fn supports_temperature(model: &str) -> bool {
    !NO_TEMPERATURE_MODELS.iter().any(|m| model.contains(m))
}

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
            "stream": true
        });

        // `temperature` is only sent to models that accept it. Newer Anthropic
        // models (Opus 4.7+) removed sampling parameters and return HTTP 400
        // ("temperature is deprecated for this model") if it's present.
        if supports_temperature(&config.model) {
            body.as_object_mut()
                .unwrap()
                .insert("temperature".to_string(), json!(config.temperature));
        }

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

#[cfg(test)]
mod tests {
    use super::supports_temperature;

    #[test]
    fn opus_4_7_and_4_8_reject_temperature() {
        assert!(!supports_temperature("claude-opus-4-8"));
        assert!(!supports_temperature("claude-opus-4-7"));
    }

    #[test]
    fn other_models_accept_temperature() {
        for m in [
            "claude-opus-4-6",
            "claude-opus-4-5",
            "claude-sonnet-4-6",
            "claude-haiku-4-5",
            "claude-3-5-sonnet-20241022",
        ] {
            assert!(supports_temperature(m), "{m} should accept temperature");
        }
    }
}
