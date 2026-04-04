/// AI Chat state — manages a conversation with an AI provider inside a pane.
///
/// `AIChatState` holds the message history, input buffer, and streaming state
/// for a single AI chat session. It is embedded inside `Block::AIChat`.
use ai::{ChatMessage, Role};

/// A single message in the chat display.
#[derive(Debug, Clone)]
pub struct DisplayMessage {
    /// `"user"`, `"assistant"`, or `"error"`.
    pub role: String,
    /// The message content (plain text; may contain markdown in future).
    pub content: String,
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
    /// Scroll offset for the chat history (auto-scroll to bottom).
    pub scroll_to_bottom: bool,
}

impl AIChatState {
    /// Create a new chat state for the given provider and model.
    pub fn new(provider_name: String, model_name: String) -> Self {
        Self {
            messages: Vec::new(),
            input: String::new(),
            streaming: false,
            current_response: String::new(),
            provider_name,
            model_name,
            error: None,
            scroll_to_bottom: true,
        }
    }

    /// Add a user message to the conversation history.
    pub fn add_user_message(&mut self, content: String) {
        self.messages.push(DisplayMessage {
            role: "user".to_string(),
            content,
        });
        self.scroll_to_bottom = true;
    }

    /// Begin receiving a streamed response.
    pub fn start_streaming(&mut self) {
        self.streaming = true;
        self.current_response.clear();
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
}
