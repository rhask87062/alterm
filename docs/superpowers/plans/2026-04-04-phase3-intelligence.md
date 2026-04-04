# Phase 3: Intelligence — AI Chat, Config, Themes

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** AI chat block with multi-provider streaming support, terminal context awareness, TOML configuration with a GUI settings panel, and a theme engine — making Altermative an AI-native workspace.

**Architecture:** A new `ai` crate provides a `Provider` trait with implementations for OpenAI-compatible APIs (OpenAI, Grok, LM Studio, Ollama), Anthropic, and Google Gemini. A new `config` crate handles TOML loading/saving with hot-reload. The AI chat block lives in the workspace crate alongside the Terminal block. Streaming responses use `reqwest` with SSE parsing, bridged to iced via `Task::stream`.

**Tech Stack:** reqwest (streaming), eventsource-stream (SSE), serde/serde_json, toml, dirs (config paths), existing iced/workspace/terminal crates

---

## File Structure

```
altermative/
├── crates/
│   ├── ai/                          # NEW: AI provider abstraction
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs               # Provider trait, ChatMessage, StreamEvent
│   │       ├── openai.rs            # OpenAI-compatible provider (also Grok, LM Studio, Ollama)
│   │       ├── anthropic.rs         # Anthropic Claude provider
│   │       └── gemini.rs            # Google Gemini provider
│   ├── config/                      # NEW: configuration management
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs               # AppConfig struct, load/save, hot-reload
│   │       └── theme.rs             # Theme definitions (color schemes)
│   ├── workspace/src/
│   │   ├── block.rs                 # Add AIChat variant to Block enum
│   │   ├── ai_chat.rs              # NEW: AI chat block state and view
│   │   └── settings_panel.rs       # NEW: GUI settings panel
│   └── ...existing crates...
```

---

### Task 1: AI Crate Scaffold — Provider Trait and Types

**Files:**
- Create: `crates/ai/Cargo.toml`
- Create: `crates/ai/src/lib.rs`
- Modify: `Cargo.toml` (workspace)

- [ ] **Step 1: Create AI crate Cargo.toml**

```toml
[package]
name = "ai"
version.workspace = true
edition.workspace = true

[dependencies]
reqwest = { version = "0.12", features = ["json", "stream"] }
eventsource-stream = "0.2"
futures-util = "0.3"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tokio.workspace = true
log.workspace = true
async-trait = "0.1"
```

- [ ] **Step 2: Define core types and Provider trait**

```rust
// crates/ai/src/lib.rs
pub mod openai;
pub mod anthropic;
pub mod gemini;

use async_trait::async_trait;
use tokio::sync::mpsc;

/// A chat message in the conversation.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ChatMessage {
    pub role: Role,
    pub content: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    User,
    Assistant,
}

/// Events streamed from an AI provider during a response.
#[derive(Debug, Clone)]
pub enum StreamEvent {
    /// A token fragment of the response.
    Token(String),
    /// The response is complete.
    Done,
    /// An error occurred.
    Error(String),
}

/// Configuration for an AI provider.
#[derive(Debug, Clone)]
pub struct ProviderConfig {
    pub base_url: String,
    pub api_key: Option<String>,
    pub model: String,
    pub max_tokens: u32,
    pub temperature: f32,
    pub system_prompt: Option<String>,
}

/// Trait for AI providers. Each provider (OpenAI, Anthropic, Gemini)
/// implements this to handle API-specific request/response formats.
#[async_trait]
pub trait Provider: Send + Sync {
    /// Stream a chat completion response. Sends StreamEvents to the channel.
    async fn stream_chat(
        &self,
        config: &ProviderConfig,
        messages: &[ChatMessage],
        tx: mpsc::Sender<StreamEvent>,
    );

    /// Provider name for display.
    fn name(&self) -> &'static str;
}
```

- [ ] **Step 3: Add to workspace members and deps**

Add `"crates/ai"` to workspace members. Add workspace deps for reqwest, serde, serde_json, futures-util, eventsource-stream, async-trait.

- [ ] **Step 4: Verify and commit**

```bash
cargo check --workspace && git add -A && git commit -m "feat: ai crate scaffold with Provider trait and types"
```

---

### Task 2: OpenAI-Compatible Provider

**Files:**
- Create: `crates/ai/src/openai.rs`

Implements the Provider trait for all OpenAI-compatible APIs (OpenAI, Grok, LM Studio, Ollama). The only difference between them is the base_url and api_key.

- [ ] **Step 1: Implement OpenAI provider**

```rust
// crates/ai/src/openai.rs
// Handles: OpenAI, xAI Grok, LM Studio, Ollama (all OpenAI-compatible)

use crate::{ChatMessage, Provider, ProviderConfig, Role, StreamEvent};
use async_trait::async_trait;
use eventsource_stream::Eventsource;
use futures_util::StreamExt;
use reqwest::Client;
use tokio::sync::mpsc;

pub struct OpenAIProvider {
    client: Client,
}

impl OpenAIProvider {
    pub fn new() -> Self {
        Self { client: Client::new() }
    }
}

#[async_trait]
impl Provider for OpenAIProvider {
    async fn stream_chat(
        &self,
        config: &ProviderConfig,
        messages: &[ChatMessage],
        tx: mpsc::Sender<StreamEvent>,
    ) {
        let url = format!("{}/chat/completions", config.base_url.trim_end_matches('/'));

        // Build messages array with system prompt
        let mut api_messages = Vec::new();
        if let Some(ref system) = config.system_prompt {
            api_messages.push(serde_json::json!({"role": "system", "content": system}));
        }
        for msg in messages {
            let role = match msg.role {
                Role::User => "user",
                Role::Assistant => "assistant",
                Role::System => "system",
            };
            api_messages.push(serde_json::json!({"role": role, "content": &msg.content}));
        }

        let body = serde_json::json!({
            "model": &config.model,
            "messages": api_messages,
            "stream": true,
            "max_tokens": config.max_tokens,
            "temperature": config.temperature,
        });

        let mut request = self.client.post(&url)
            .header("Content-Type", "application/json")
            .json(&body);

        if let Some(ref key) = config.api_key {
            request = request.header("Authorization", format!("Bearer {}", key));
        }

        let response = match request.send().await {
            Ok(r) => r,
            Err(e) => { let _ = tx.send(StreamEvent::Error(e.to_string())).await; return; }
        };

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            let _ = tx.send(StreamEvent::Error(format!("{}: {}", status, body))).await;
            return;
        }

        let mut stream = response.bytes_stream().eventsource();
        while let Some(event) = stream.next().await {
            match event {
                Ok(ev) => {
                    if ev.data == "[DONE]" { break; }
                    if let Ok(chunk) = serde_json::from_str::<serde_json::Value>(&ev.data) {
                        if let Some(content) = chunk["choices"][0]["delta"]["content"].as_str() {
                            if !content.is_empty() {
                                if tx.send(StreamEvent::Token(content.to_string())).await.is_err() {
                                    return;
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    let _ = tx.send(StreamEvent::Error(e.to_string())).await;
                    break;
                }
            }
        }
        let _ = tx.send(StreamEvent::Done).await;
    }

    fn name(&self) -> &'static str { "OpenAI-Compatible" }
}
```

- [ ] **Step 2: Verify and commit**

```bash
cargo check -p ai && git add -A && git commit -m "feat: OpenAI-compatible provider (OpenAI, Grok, LM Studio, Ollama)"
```

---

### Task 3: Anthropic Provider

**Files:**
- Create: `crates/ai/src/anthropic.rs`

Anthropic uses different auth headers (`x-api-key`, `anthropic-version`), a top-level `system` field, and typed SSE events.

- [ ] **Step 1: Implement Anthropic provider**

Key differences from OpenAI:
- Auth: `x-api-key` header (not `Authorization: Bearer`)
- Required: `anthropic-version: 2023-06-01` header
- System prompt: top-level `system` field, NOT a message
- SSE events use typed `event:` field — extract text from `content_block_delta` events
- Stream ends at `message_stop` event (no `[DONE]`)

- [ ] **Step 2: Verify and commit**

```bash
cargo check -p ai && git add -A && git commit -m "feat: Anthropic Claude provider"
```

---

### Task 4: Google Gemini Provider

**Files:**
- Create: `crates/ai/src/gemini.rs`

Gemini uses different endpoint format, API key as query param, and different message structure.

- [ ] **Step 1: Implement Gemini provider**

Key differences:
- Endpoint: `{base_url}/models/{model}:streamGenerateContent?alt=sse&key={api_key}`
- Messages use `contents` with `parts` array, roles are `user`/`model`
- System prompt goes in `systemInstruction`
- No `[DONE]` terminator — stream ends when connection closes
- Content at `candidates[0].content.parts[0].text`

- [ ] **Step 2: Verify and commit**

```bash
cargo check -p ai && git add -A && git commit -m "feat: Google Gemini provider"
```

---

### Task 5: Config Crate — TOML Configuration

**Files:**
- Create: `crates/config/Cargo.toml`
- Create: `crates/config/src/lib.rs`
- Create: `crates/config/src/theme.rs`
- Create: `config/default.toml`

- [ ] **Step 1: Define config structs**

```rust
// crates/config/src/lib.rs
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AppConfig {
    pub general: GeneralConfig,
    pub ai: AIConfig,
    pub appearance: AppearanceConfig,
    pub terminal: TerminalConfig,
    pub keybindings: KeybindingsConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct GeneralConfig {
    pub default_shell: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AIConfig {
    pub default_provider: String,          // "openai", "anthropic", "gemini", "grok", "lmstudio", "ollama"
    pub max_tokens: u32,
    pub temperature: f32,
    pub system_prompt: String,
    pub providers: AIProviders,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct AIProviders {
    pub openai: Option<ProviderEntry>,
    pub anthropic: Option<ProviderEntry>,
    pub gemini: Option<ProviderEntry>,
    pub grok: Option<ProviderEntry>,
    pub lmstudio: Option<ProviderEntry>,
    pub ollama: Option<ProviderEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderEntry {
    pub api_key: Option<String>,
    pub base_url: Option<String>,
    pub model: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AppearanceConfig {
    pub font_size: f32,
    pub font_family: String,
    pub theme: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct TerminalConfig {
    pub scrollback_lines: usize,
    pub cursor_style: String,       // "block", "underline", "bar"
    pub cursor_blink: bool,
    pub copy_on_select: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct KeybindingsConfig {
    // Custom keybinding overrides — Phase 5
}

impl AppConfig {
    pub fn load(path: &Path) -> Result<Self, Box<dyn std::error::Error>> { ... }
    pub fn save(&self, path: &Path) -> Result<(), Box<dyn std::error::Error>> { ... }
    pub fn config_dir() -> PathBuf { ... }  // ~/.config/altermative/
    pub fn config_path() -> PathBuf { ... } // ~/.config/altermative/config.toml
}
```

Provide sensible defaults for all fields:
- default_provider: "ollama"
- max_tokens: 4096
- temperature: 0.7
- system_prompt: "You are a helpful terminal assistant."
- font_size: 14.0
- theme: "dark"
- scrollback_lines: 10000
- cursor_blink: true

- [ ] **Step 2: Create default.toml**

A well-commented default config file.

- [ ] **Step 3: Add theme module**

```rust
// crates/config/src/theme.rs
pub struct Theme {
    pub name: String,
    pub bg: (u8, u8, u8),
    pub fg: (u8, u8, u8),
    pub cursor: (u8, u8, u8),
    pub selection_bg: (u8, u8, u8),
    pub ansi_colors: [(u8, u8, u8); 16],
}

impl Theme {
    pub fn dark() -> Self { ... }    // Current dark theme
    pub fn light() -> Self { ... }   // Light variant
    pub fn builtin_themes() -> Vec<Self> { ... }
}
```

- [ ] **Step 4: Verify and commit**

```bash
cargo check --workspace && git add -A && git commit -m "feat: config crate with TOML loading, themes"
```

---

### Task 6: AI Chat Block

**Files:**
- Create: `crates/workspace/src/ai_chat.rs`
- Modify: `crates/workspace/src/block.rs`
- Modify: `crates/workspace/src/lib.rs`
- Modify: `alterm/src/main.rs`

- [ ] **Step 1: Create AI chat state**

```rust
// crates/workspace/src/ai_chat.rs
pub struct AIChatState {
    pub messages: Vec<DisplayMessage>,
    pub input: String,
    pub streaming: bool,        // true while receiving tokens
    pub current_response: String, // accumulates tokens during streaming
    pub provider_name: String,
    pub error: Option<String>,
}

pub struct DisplayMessage {
    pub role: String,    // "user" or "assistant"
    pub content: String,
}
```

- [ ] **Step 2: Add AIChat variant to Block enum**

```rust
pub enum Block {
    Terminal { ... },
    AIChat {
        state: AIChatState,
    },
}
```

Add methods: `new_ai_chat()`, handle `tick()`, `title()`, `render_grid()` for AIChat (returns empty grid — AI chat renders differently).

- [ ] **Step 3: Create AI chat view in main.rs**

The AI chat block renders as a scrollable chat area + input field, not as a terminal canvas. In the `view()` function, when rendering a pane's block:
- If Terminal → render via TerminalView (existing)
- If AIChat → render chat messages + text_input

Chat layout:
```
┌──────────────────────────┐
│  scrollable chat area    │
│  User: How do I list...  │
│  AI: You can use `ls`... │
│                          │
├──────────────────────────┤
│  [Type a message...  ⏎]  │
└──────────────────────────┘
```

- [ ] **Step 4: Wire AI streaming**

When user submits a message:
1. Add user message to chat state
2. Create a streaming task that calls the AI provider
3. As tokens arrive, append to `current_response`
4. On Done, move `current_response` to messages as assistant message
5. On Error, display error

Use `iced::Task::stream` to bridge the async stream into iced messages.

- [ ] **Step 5: Add terminal context**

When sending to AI, include the focused terminal's recent output:
- Get the last N lines from the active terminal pane
- Prepend as context: "Here is the recent terminal output:\n```\n{output}\n```"

- [ ] **Step 6: Wire sidebar AI button and Ctrl+Shift+A**

- Sidebar "AI" button creates a new AI chat pane (splits focused pane)
- `Ctrl+Shift+A` toggles/creates an AI chat pane
- Add to keybindings registry

- [ ] **Step 7: Verify and commit**

```bash
cargo check --workspace && git add -A && git commit -m "feat: AI chat block with streaming and terminal context"
```

---

### Task 7: Settings Panel

**Files:**
- Create: `crates/workspace/src/settings_panel.rs`
- Modify: `alterm/src/main.rs`

- [ ] **Step 1: Implement settings panel view**

A panel (rendered as a Block or overlay) with:
- **Appearance section:** font size slider, theme dropdown
- **AI section:** provider dropdown, model text input, API key (secure input), temperature slider
- **Terminal section:** scrollback lines, cursor style, cursor blink toggle
- Save button that writes to config.toml

Use iced widgets: `text_input`, `toggler`, `slider`, `pick_list`, `button`, `scrollable`.

- [ ] **Step 2: Wire settings to config**

- Load config on app start
- Settings panel reads/writes AppConfig
- Save button persists to TOML file
- Changes take effect immediately (no restart needed)

- [ ] **Step 3: Wire sidebar settings button and Ctrl+Shift+,**

- Sidebar gear button opens settings as a pane or overlay
- `Ctrl+Shift+,` shortcut

- [ ] **Step 4: Verify and commit**

```bash
cargo check --workspace && git add -A && git commit -m "feat: settings panel with config persistence"
```

---

### Task 8: Integration and Polish

**Files:**
- Modify: `alterm/src/main.rs`
- Modify: various crate files

- [ ] **Step 1: Load config on startup**

In `Altermative::new()`, load `~/.config/altermative/config.toml`. If it doesn't exist, create with defaults. Pass config to relevant components.

- [ ] **Step 2: Apply theme from config**

Use the theme's colors for the terminal palette, UI chrome colors, etc.

- [ ] **Step 3: Apply AI config**

Pass provider config to AI chat blocks when creating them.

- [ ] **Step 4: Context pinning UI**

In AI chat, show which terminal pane is providing context. Add a small label like "Context: Terminal (Tab 1, Pane 1)".

- [ ] **Step 5: Final verification**

Run and verify:
1. AI chat pane opens from sidebar or Ctrl+Shift+A
2. Can type messages and get streamed responses (requires API key in config)
3. AI can see terminal output context
4. Settings panel opens and changes persist
5. Theme changes apply
6. Works with at least one provider (test with Ollama/LM Studio if available locally)

- [ ] **Step 6: Commit**

```bash
git add -A && git commit -m "feat: Phase 3 complete — AI chat, config, themes"
```

---

## Phase 3 Completion Checklist

- [ ] AI provider trait with streaming support
- [ ] OpenAI-compatible provider (OpenAI, Grok, LM Studio, Ollama)
- [ ] Anthropic Claude provider
- [ ] Google Gemini provider
- [ ] TOML configuration at ~/.config/altermative/config.toml
- [ ] AI chat block with streaming response display
- [ ] Terminal context awareness (active pane output sent to AI)
- [ ] Settings panel GUI (appearance, AI, terminal settings)
- [ ] Theme engine (dark/light + custom)
- [ ] Ctrl+Shift+A for AI chat, gear button for settings
- [ ] Config changes persist and apply immediately

## Notes for Phase 4

- Browser block (wry webview) will be added as another Block variant
- File preview block similarly
- Lua hooks (mlua) for dynamic config
- Desktop integration (.desktop file, packaging)
