pub mod anthropic;
pub mod gemini;
pub mod openai;

use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

// ---------------------------------------------------------------------------
// Core types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    User,
    Assistant,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: Role,
    pub content: String,
}

#[derive(Debug, Clone)]
pub enum StreamEvent {
    Token(String),
    Done,
    Error(String),
}

#[derive(Debug, Clone)]
pub struct ProviderConfig {
    pub base_url: String,
    pub api_key: Option<String>,
    pub model: String,
    pub max_tokens: u32,
    pub temperature: f32,
    pub system_prompt: Option<String>,
}

// ---------------------------------------------------------------------------
// Provider trait  (native async fn — stable since Rust 1.75)
// ---------------------------------------------------------------------------

pub trait Provider: Send + Sync {
    fn stream_chat(
        &self,
        config: &ProviderConfig,
        messages: &[ChatMessage],
        tx: mpsc::Sender<StreamEvent>,
    ) -> impl std::future::Future<Output = ()> + Send;

    fn name(&self) -> &'static str;
}

// ---------------------------------------------------------------------------
// Model listing
// ---------------------------------------------------------------------------

/// Fetch available models from a provider's API.
///
/// - **OpenAI-compatible** (openai, grok, lmstudio, ollama): `GET {base_url}/models`
/// - **Anthropic**: No listing endpoint — returns a hardcoded set.
/// - **Gemini**: `GET {base_url}/models?key={key}`
pub async fn fetch_models(
    base_url: &str,
    api_key: Option<&str>,
    provider_type: &str,
) -> Vec<String> {
    match provider_type {
        "anthropic" => anthropic_hardcoded_models(),
        "gemini" => fetch_gemini_models(base_url, api_key).await,
        // openai, grok, lmstudio, ollama — all OpenAI-compatible
        _ => fetch_openai_compatible_models(base_url, api_key).await,
    }
}

fn anthropic_hardcoded_models() -> Vec<String> {
    vec![
        "claude-sonnet-4-20250514".to_string(),
        "claude-haiku-4-5-20251001".to_string(),
        "claude-opus-4-5-20250414".to_string(),
    ]
}

async fn fetch_openai_compatible_models(
    base_url: &str,
    api_key: Option<&str>,
) -> Vec<String> {
    let url = format!("{}/models", base_url);
    let client = reqwest::Client::new();
    let mut request = client.get(&url);
    if let Some(key) = api_key {
        request = request.header("Authorization", format!("Bearer {key}"));
    }

    let response = match request.send().await {
        Ok(r) if r.status().is_success() => r,
        _ => return Vec::new(),
    };

    let body: serde_json::Value = match response.json().await {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };

    let mut models: Vec<String> = body
        .get("data")
        .and_then(|d| d.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|item| item.get("id").and_then(|v| v.as_str()))
                .map(|s| s.to_string())
                .collect()
        })
        .unwrap_or_default();

    models.sort();
    models
}

async fn fetch_gemini_models(
    base_url: &str,
    api_key: Option<&str>,
) -> Vec<String> {
    let mut url = format!("{}/models", base_url);
    if let Some(key) = api_key {
        url.push_str(&format!("?key={key}"));
    }

    let client = reqwest::Client::new();
    let response = match client.get(&url).send().await {
        Ok(r) if r.status().is_success() => r,
        _ => return Vec::new(),
    };

    let body: serde_json::Value = match response.json().await {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };

    let mut models: Vec<String> = body
        .get("models")
        .and_then(|m| m.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|item| item.get("name").and_then(|v| v.as_str()))
                .map(|s| {
                    // Strip "models/" prefix: "models/gemini-2.0-flash" → "gemini-2.0-flash"
                    s.strip_prefix("models/").unwrap_or(s).to_string()
                })
                .collect()
        })
        .unwrap_or_default();

    models.sort();
    models
}
