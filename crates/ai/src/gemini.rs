use crate::{ChatMessage, Provider, ProviderConfig, StreamEvent};
use tokio::sync::mpsc;

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

impl Provider for GeminiProvider {
    async fn stream_chat(
        &self,
        _config: &ProviderConfig,
        _messages: &[ChatMessage],
        _tx: mpsc::Sender<StreamEvent>,
    ) {
        todo!()
    }

    fn name(&self) -> &'static str {
        "gemini"
    }
}
