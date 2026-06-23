/// AI Chat state — manages a conversation with an AI provider inside a pane.
///
/// `AIChatState` holds the message history, input buffer, and streaming state
/// for a single AI chat session. It is embedded inside `Block::AIChat`.
use ai::{ChatMessage, Role};
use serde::{Serialize, Deserialize};

/// A single message in the chat display.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DisplayMessage {
    /// `"user"`, `"assistant"`, or `"error"`.
    pub role: String,
    /// The message content (plain text; may contain markdown in future).
    pub content: String,
    /// The model that generated this message (for assistant messages).
    pub model: Option<String>,
}

/// State for one AI chat session.
#[derive(Debug, Clone)]
pub struct AIChatState {
    /// Full conversation history (user + assistant messages).
    pub messages: Vec<DisplayMessage>,
    /// Current text in the input field.
    pub input: String,
    /// Whether we are currently receiving tokens from the provider.
    pub streaming: bool,
    /// Accumulates tokens during streaming; moved into `messages` on finish.
    pub current_response: String,
    /// Provider identifier, e.g. `"openai"`, `"anthropic"`, `"ollama"`.
    pub provider_name: String,
    /// Model identifier, e.g. `"gpt-4o"`, `"llama3.2"`.
    pub model_name: String,
    /// Last error message, if any.
    pub error: Option<String>,
    /// Model name captured when streaming started (for labeling the response).
    pub streaming_model: Option<String>,
    /// Scroll offset for the chat history (auto-scroll to bottom).
    pub scroll_to_bottom: bool,
    /// Available models fetched from the provider API (for the dropdown).
    pub available_models: Vec<String>,
    /// Whether we are currently fetching the model list.
    pub models_loading: bool,
    /// Human-readable reason the last model fetch failed. Only surfaced in the
    /// UI when there are no models to show.
    pub models_error: Option<String>,
    /// Whether the user opted into typing a custom model name.
    pub custom_model_entry: bool,
}

impl AIChatState {
    /// Create a new chat state for the given provider and model.
    pub fn new(provider_name: String, model_name: String) -> Self {
        Self {
            messages: vec![DisplayMessage {
                role: "assistant".to_string(),
                content: format!(
                    "Welcome to AI Chat! Provider: {}, Model: {}. Type a message below.",
                    provider_name, model_name
                ),
                model: Some(model_name.clone()),
            }],
            input: String::new(),
            streaming: false,
            current_response: String::new(),
            provider_name,
            model_name,
            error: None,
            streaming_model: None,
            scroll_to_bottom: true,
            available_models: Vec::new(),
            models_loading: false,
            models_error: None,
            custom_model_entry: false,
        }
    }

    /// Add a user message to the conversation history.
    pub fn add_user_message(&mut self, content: String) {
        self.messages.push(DisplayMessage {
            role: "user".to_string(),
            content,
            model: None,
        });
        self.scroll_to_bottom = true;
    }

    /// Begin receiving a streamed response. Captures the current model name
    /// so it's preserved even if the user switches models mid-conversation.
    pub fn start_streaming(&mut self) {
        self.streaming = true;
        self.current_response.clear();
        self.streaming_model = Some(self.model_name.clone());
        self.error = None;
    }

    /// Append a token to the in-progress response.
    pub fn append_token(&mut self, token: String) {
        self.current_response.push_str(&token);
        self.scroll_to_bottom = true;
    }

    /// Finish streaming: move the accumulated response into the conversation
    /// history as an assistant message.
    pub fn finish_streaming(&mut self) {
        self.streaming = false;
        if !self.current_response.is_empty() {
            self.messages.push(DisplayMessage {
                role: "assistant".to_string(),
                content: std::mem::take(&mut self.current_response),
                model: self.streaming_model.take(),
            });
        }
        self.scroll_to_bottom = true;
    }

    /// Record an error (also stops streaming).
    pub fn set_error(&mut self, msg: String) {
        self.streaming = false;
        self.current_response.clear();
        self.error = Some(msg.clone());
        self.messages.push(DisplayMessage {
            role: "error".to_string(),
            content: msg,
            model: None,
        });
        self.scroll_to_bottom = true;
    }

    /// Convert the display messages into the `ChatMessage` format expected by
    /// the AI provider trait. Error messages are excluded.
    pub fn chat_messages_for_api(&self) -> Vec<ChatMessage> {
        self.messages
            .iter()
            .filter_map(|dm| {
                let role = match dm.role.as_str() {
                    "user" => Role::User,
                    "assistant" => Role::Assistant,
                    _ => return None, // skip error messages
                };
                Some(ChatMessage {
                    role,
                    content: dm.content.clone(),
                })
            })
            .collect()
    }

    /// Decide how the model selector should render, given current state.
    /// A non-empty list always wins, so a background refresh never hides it.
    pub fn selector_mode(&self) -> SelectorMode {
        if self.custom_model_entry {
            SelectorMode::Custom
        } else if !self.available_models.is_empty() {
            SelectorMode::List
        } else if self.models_loading {
            SelectorMode::Loading
        } else if self.models_error.is_some() {
            SelectorMode::Error
        } else {
            SelectorMode::Empty
        }
    }
}

/// Which UI the model selector should render. A pure function of state, so the
/// branch logic is testable without rendering.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectorMode {
    /// Free-text custom model entry (user opted in).
    Custom,
    /// A dropdown of available models.
    List,
    /// A fetch is in flight and there's nothing cached to show yet.
    Loading,
    /// The last fetch failed and there's no cached list to fall back on.
    Error,
    /// Nothing available and nothing happening — caller should kick a fetch.
    Empty,
}

#[cfg(test)]
mod selector_tests {
    use super::*;

    fn state() -> AIChatState {
        AIChatState::new("openai".to_string(), "gpt-4o".to_string())
    }

    #[test]
    fn defaults_to_empty() {
        assert_eq!(state().selector_mode(), SelectorMode::Empty);
    }

    #[test]
    fn list_when_models_present() {
        let mut s = state();
        s.available_models = vec!["gpt-4o".into()];
        assert_eq!(s.selector_mode(), SelectorMode::List);
    }

    #[test]
    fn loading_when_empty_and_loading() {
        let mut s = state();
        s.models_loading = true;
        assert_eq!(s.selector_mode(), SelectorMode::Loading);
    }

    #[test]
    fn error_when_empty_and_failed() {
        let mut s = state();
        s.models_error = Some("No API key".into());
        assert_eq!(s.selector_mode(), SelectorMode::Error);
    }

    #[test]
    fn custom_overrides_everything() {
        let mut s = state();
        s.available_models = vec!["gpt-4o".into()];
        s.custom_model_entry = true;
        assert_eq!(s.selector_mode(), SelectorMode::Custom);
    }

    #[test]
    fn list_wins_over_loading_when_cached() {
        let mut s = state();
        s.available_models = vec!["gpt-4o".into()];
        s.models_loading = true; // background refresh in flight
        assert_eq!(s.selector_mode(), SelectorMode::List);
    }
}
