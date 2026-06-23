pub mod anthropic;
pub mod gemini;
pub mod openai;
pub mod model_cache;

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

/// Why a model-list fetch failed. Drives the UI's empty-state message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModelFetchError {
    /// Provider requires an API key and none is configured.
    MissingApiKey,
    /// Server rejected the credentials (401 / 403).
    Unauthorized,
    /// Could not reach the provider (network/connection error).
    Unreachable,
    /// Reached the provider but the response was not understood.
    BadResponse,
}

impl ModelFetchError {
    /// A short, human-readable reason suitable for the model selector.
    pub fn user_message(&self) -> String {
        match self {
            ModelFetchError::MissingApiKey => "No API key — add one in Settings".to_string(),
            ModelFetchError::Unauthorized => "API key rejected — check it in Settings".to_string(),
            ModelFetchError::Unreachable => "Couldn't reach provider — is it online?".to_string(),
            ModelFetchError::BadResponse => "Unexpected response from provider".to_string(),
        }
    }
}

/// Whether a provider needs an API key to list models / chat.
pub fn provider_requires_key(provider: &str) -> bool {
    matches!(provider, "openai" | "anthropic" | "google" | "xai")
}

/// Substrings (lowercased compare) that mark a model id as NOT a text-chat
/// model — embeddings, speech, image/audio generation, moderation, etc.
/// Deliberately conservative: does NOT include `audio`/`vision`/`realtime`,
/// which are chat-capable multimodal models.
///
/// Note: some markers are short substrings (e.g. `clip`) and could in theory
/// match a future chat model id by coincidence; if a real chat model ever
/// disappears from the dropdown, narrow the offending marker here.
const NON_CHAT_MARKERS: &[&str] = &[
    "embed", "tts", "whisper", "dall-e", "dalle", "moderation",
    "rerank", "clip", "stable-diffusion", "sora", "image-", "-image",
];

/// True if `id` looks like a text-chat model (not embeddings/speech/image/etc.).
pub fn is_chat_model(id: &str) -> bool {
    let lower = id.to_lowercase();
    !NON_CHAT_MARKERS.iter().any(|m| lower.contains(m))
}

/// Drop non-chat models, preserving order.
pub fn filter_chat_models(models: Vec<String>) -> Vec<String> {
    models.into_iter().filter(|m| is_chat_model(m)).collect()
}

/// Fetch available models from a provider's API.
///
/// - Key-requiring providers with no key → `Err(MissingApiKey)` (no network call).
/// - **Anthropic**: no listing endpoint — returns a hardcoded set.
/// - **Gemini** (`google`): `GET {base_url}/models?key={key}`.
/// - **OpenAI-compatible** (openai, xai, lmstudio, ollama): `GET {base_url}/models`.
pub async fn fetch_models(
    base_url: &str,
    api_key: Option<&str>,
    provider_type: &str,
) -> Result<Vec<String>, ModelFetchError> {
    let key_missing = api_key.map_or(true, |k| k.trim().is_empty());
    if provider_requires_key(provider_type) && key_missing {
        return Err(ModelFetchError::MissingApiKey);
    }
    match provider_type {
        "anthropic" => Ok(anthropic_hardcoded_models()),
        "google" => fetch_gemini_models(base_url, api_key).await,
        _ => fetch_openai_compatible_models(base_url, api_key).await,
    }
}

fn anthropic_hardcoded_models() -> Vec<String> {
    vec![
        "claude-opus-4-5-20250414".to_string(),
        "claude-sonnet-4-20250514".to_string(),
        "claude-haiku-4-5-20251001".to_string(),
        "claude-3-5-sonnet-20241022".to_string(),
        "claude-3-5-haiku-20241022".to_string(),
        "claude-3-opus-20240229".to_string(),
        "claude-3-sonnet-20240229".to_string(),
        "claude-3-haiku-20240307".to_string(),
    ]
}

fn status_to_error(status: u16) -> ModelFetchError {
    match status {
        401 | 403 => ModelFetchError::Unauthorized,
        _ => ModelFetchError::BadResponse,
    }
}

async fn fetch_openai_compatible_models(
    base_url: &str,
    api_key: Option<&str>,
) -> Result<Vec<String>, ModelFetchError> {
    let url = format!("{}/models", base_url);
    let client = reqwest::Client::new();
    let mut request = client.get(&url);
    if let Some(key) = api_key {
        request = request.header("Authorization", format!("Bearer {key}"));
    }

    let response = request.send().await.map_err(|_| ModelFetchError::Unreachable)?;
    if !response.status().is_success() {
        return Err(status_to_error(response.status().as_u16()));
    }
    let body: serde_json::Value =
        response.json().await.map_err(|_| ModelFetchError::BadResponse)?;
    Ok(filter_chat_models(parse_openai_models(&body)))
}

/// Extract model ids from an OpenAI-style `{ "data": [ { "id": ... } ] }` body.
pub fn parse_openai_models(body: &serde_json::Value) -> Vec<String> {
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

/// Precondition: callers must pass a non-blank `api_key`. `fetch_models`
/// guarantees this for `google` via the `provider_requires_key` short-circuit;
/// a `None` key here would build a keyless URL the API rejects.
async fn fetch_gemini_models(
    base_url: &str,
    api_key: Option<&str>,
) -> Result<Vec<String>, ModelFetchError> {
    let mut url = format!("{}/models", base_url);
    if let Some(key) = api_key {
        url.push_str(&format!("?key={key}"));
    }
    let client = reqwest::Client::new();
    let response = client.get(&url).send().await.map_err(|_| ModelFetchError::Unreachable)?;
    if !response.status().is_success() {
        return Err(status_to_error(response.status().as_u16()));
    }
    let body: serde_json::Value =
        response.json().await.map_err(|_| ModelFetchError::BadResponse)?;
    Ok(filter_chat_models(parse_gemini_models(&body)))
}

/// Extract model names from a Gemini `{ "models": [ { "name": "models/..." } ] }`
/// body, stripping the `models/` prefix.
pub fn parse_gemini_models(body: &serde_json::Value) -> Vec<String> {
    let mut models: Vec<String> = body
        .get("models")
        .and_then(|m| m.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|item| item.get("name").and_then(|v| v.as_str()))
                .map(|s| s.strip_prefix("models/").unwrap_or(s).to_string())
                .collect()
        })
        .unwrap_or_default();
    models.sort();
    models
}

#[cfg(test)]
mod model_listing_tests {
    use super::*;

    #[test]
    fn requires_key_for_cloud_not_local() {
        for p in ["openai", "anthropic", "google", "xai"] {
            assert!(provider_requires_key(p), "{p} should require a key");
        }
        for p in ["ollama", "lmstudio"] {
            assert!(!provider_requires_key(p), "{p} should not require a key");
        }
    }

    #[test]
    fn error_messages_are_actionable() {
        assert!(ModelFetchError::MissingApiKey.user_message().to_lowercase().contains("api key"));
        assert!(ModelFetchError::Unauthorized.user_message().to_lowercase().contains("key"));
        assert!(ModelFetchError::Unreachable.user_message().to_lowercase().contains("reach"));
        assert!(ModelFetchError::BadResponse.user_message().to_lowercase().contains("unexpected"));
    }

    #[test]
    fn parses_openai_data_ids_sorted() {
        let body = serde_json::json!({
            "data": [ {"id": "gpt-4o"}, {"id": "gpt-3.5"}, {"id": "o1"} ]
        });
        assert_eq!(parse_openai_models(&body), vec!["gpt-3.5", "gpt-4o", "o1"]);
    }

    #[test]
    fn parses_openai_missing_data_is_empty() {
        let body = serde_json::json!({ "object": "list" });
        assert!(parse_openai_models(&body).is_empty());
    }

    #[test]
    fn parses_gemini_strips_models_prefix_sorted() {
        let body = serde_json::json!({
            "models": [ {"name": "models/gemini-2.0-flash"}, {"name": "models/gemini-1.5-pro"} ]
        });
        assert_eq!(parse_gemini_models(&body), vec!["gemini-1.5-pro", "gemini-2.0-flash"]);
    }

    #[test]
    fn parses_gemini_missing_models_is_empty() {
        let body = serde_json::json!({ "someOtherField": true });
        assert!(parse_gemini_models(&body).is_empty());
    }

    #[test]
    fn keeps_chat_models() {
        for id in [
            "gpt-4o", "o1", "o3-mini", "claude-sonnet-4-5-20250929",
            "llama3.2", "gemini-2.0-flash", "grok-2", "mixtral-8x7b",
            "gpt-4o-audio-preview", "gpt-4-vision-preview", "gpt-4o-realtime-preview",
        ] {
            assert!(is_chat_model(id), "{id} should be kept");
        }
    }

    #[test]
    fn drops_non_chat_models() {
        for id in [
            "text-embedding-3-small", "text-embedding-ada-002", "nomic-embed-text",
            "whisper-1", "tts-1", "tts-1-hd", "dall-e-3", "gpt-image-1", "image-gen-v2",
            "text-moderation-latest", "rerank-english-v3.0", "clip-vit-base",
            "stable-diffusion-xl", "sora",
        ] {
            assert!(!is_chat_model(id), "{id} should be dropped");
        }
    }

    #[test]
    fn filter_preserves_order_of_kept() {
        let input = vec![
            "gpt-4o".to_string(),
            "text-embedding-3-small".to_string(),
            "o1".to_string(),
            "whisper-1".to_string(),
        ];
        assert_eq!(filter_chat_models(input), vec!["gpt-4o".to_string(), "o1".to_string()]);
    }
}
