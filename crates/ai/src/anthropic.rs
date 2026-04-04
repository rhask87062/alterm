use crate::{ChatMessage, Provider, ProviderConfig, StreamEvent};
use tokio::sync::mpsc;

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

impl Provider for AnthropicProvider {
    async fn stream_chat(
        &self,
        _config: &ProviderConfig,
        _messages: &[ChatMessage],
        _tx: mpsc::Sender<StreamEvent>,
    ) {
        todo!()
    }

    fn name(&self) -> &'static str {
        "anthropic"
    }
}
