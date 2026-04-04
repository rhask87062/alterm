use crate::{ChatMessage, Provider, ProviderConfig, Role, StreamEvent};
use eventsource_stream::Eventsource;
use futures_util::StreamExt;
use serde_json::{json, Value};
use tokio::sync::mpsc;

/// Google Gemini provider.
///
/// Key differences from OpenAI/Anthropic:
/// - API key is passed as a query parameter, not a header
/// - Uses `contents` array with `parts`, role `model` instead of `assistant`
/// - System prompt goes in `systemInstruction.parts[0].text`
/// - SSE stream has no `[DONE]` marker — check `finishReason: "STOP"`
/// - Default base URL: `https://generativelanguage.googleapis.com/v1beta`
pub struct GeminiProvider {
    client: reqwest::Client,
}

impl GeminiProvider {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
        }
    }
}

impl Default for GeminiProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl Provider for GeminiProvider {
    async fn stream_chat(
        &self,
        config: &ProviderConfig,
        messages: &[ChatMessage],
        tx: mpsc::Sender<StreamEvent>,
    ) {
        // Build URL: {base_url}/models/{model}:streamGenerateContent?alt=sse&key={api_key}
        let mut url = format!(
            "{}/models/{}:streamGenerateContent?alt=sse",
            config.base_url, config.model
        );
        if let Some(ref key) = config.api_key {
            url.push_str(&format!("&key={key}"));
        }

        // Build contents array — Gemini uses "user" and "model" roles
        let contents: Vec<Value> = messages
            .iter()
            .filter(|m| m.role != Role::System)
            .map(|m| {
                let role = match m.role {
                    Role::User | Role::System => "user",
                    Role::Assistant => "model",
                };
                json!({
                    "role": role,
                    "parts": [{ "text": &m.content }]
                })
            })
            .collect();

        let mut body = json!({
            "contents": contents,
            "generationConfig": {
                "temperature": config.temperature,
                "maxOutputTokens": config.max_tokens
            }
        });

        // System prompt goes in systemInstruction
        if let Some(ref system) = config.system_prompt {
            body.as_object_mut().unwrap().insert(
                "systemInstruction".to_string(),
                json!({
                    "parts": [{ "text": system }]
                }),
            );
        }

        let request = self
            .client
            .post(&url)
            .header("content-type", "application/json")
            .json(&body);

        let response = match request.send().await {
            Ok(resp) => {
                if !resp.status().is_success() {
                    let status = resp.status();
                    let body = resp.text().await.unwrap_or_default();
                    let _ = tx
                        .send(StreamEvent::Error(format!(
                            "Gemini API error {status}: {body}"
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

                    if let Ok(parsed) = serde_json::from_str::<Value>(&data) {
                        // Check for finish signal
                        if let Some(reason) = parsed
                            .get("candidates")
                            .and_then(|c| c.get(0))
                            .and_then(|c| c.get("finishReason"))
                            .and_then(|v| v.as_str())
                        {
                            if reason == "STOP" {
                                // Extract any final text before breaking
                                if let Some(text) = extract_gemini_text(&parsed) {
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
                                break;
                            }
                        }

                        // Extract content: candidates[0].content.parts[0].text
                        if let Some(text) = extract_gemini_text(&parsed) {
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
        "gemini"
    }
}

/// Extract text from a Gemini SSE chunk: `candidates[0].content.parts[0].text`
fn extract_gemini_text(value: &Value) -> Option<&str> {
    value
        .get("candidates")?
        .get(0)?
        .get("content")?
        .get("parts")?
        .get(0)?
        .get("text")?
        .as_str()
}
