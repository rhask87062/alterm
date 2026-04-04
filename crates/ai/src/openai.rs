use crate::{ChatMessage, Provider, ProviderConfig, StreamEvent};
use tokio::sync::mpsc;

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

impl Provider for OpenAIProvider {
    async fn stream_chat(
        &self,
        _config: &ProviderConfig,
        _messages: &[ChatMessage],
        _tx: mpsc::Sender<StreamEvent>,
    ) {
        todo!()
    }

    fn name(&self) -> &'static str {
        "openai"
    }
}
