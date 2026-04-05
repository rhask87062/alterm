/// Settings panel — a pane-based UI for editing application configuration.
///
/// `SettingsState` holds a working copy of `AppConfig` that the user edits
/// in-place. Changes are persisted to disk when the Save button is pressed.
use altermative_config::AppConfig;

/// Which section of the settings panel is currently visible.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsSection {
    Appearance,
    AI,
    Terminal,
}

/// All editable fields that can change in the settings panel.
///
/// Each variant carries the new value typed/selected by the user.
#[derive(Debug, Clone)]
pub enum SettingsField {
    // Appearance
    FontSize(String),
    Theme(String),
    FontFamily(String),

    // AI
    DefaultProvider(String),
    AIModel(String),
    AIApiKey(String),
    Temperature(f32),
    MaxTokens(String),
    SystemPrompt(String),

    // Terminal
    ScrollbackLines(String),
    CursorBlink(bool),
    CopyOnSelect(bool),
}

/// Mutable state for a settings panel living inside a `Block::Settings` pane.
#[derive(Debug, Clone)]
pub struct SettingsState {
    /// Working copy of the application config — edits are made here until saved.
    pub config: AppConfig,
    /// `true` when the working copy differs from the last-saved state.
    pub dirty: bool,
    /// Which section tab is active in the settings sidebar.
    pub active_section: SettingsSection,

    // ── Scratchpad strings for text inputs ──
    // These hold the raw text typed by the user, which may temporarily be
    // invalid (e.g. an empty string when the user clears a numeric field).
    pub font_size_text: String,
    pub max_tokens_text: String,
    pub scrollback_text: String,
    pub api_key_text: String,
    pub model_text: String,
    pub system_prompt_text: String,
    pub font_family_text: String,
}

impl SettingsState {
    /// Create a new settings state from the application's current config.
    pub fn new(config: AppConfig) -> Self {
        let font_size_text = format!("{}", config.appearance.font_size);
        let max_tokens_text = format!("{}", config.ai.max_tokens);
        let scrollback_text = format!("{}", config.terminal.scrollback_lines);
        let api_key_text = {
            let provider = &config.ai.default_provider;
            Self::api_key_for_provider(&config, provider)
        };
        let model_text = {
            let provider = &config.ai.default_provider;
            Self::model_for_provider(&config, provider)
        };
        let system_prompt_text = config.ai.system_prompt.clone();
        let font_family_text = config.appearance.font_family.clone();

        Self {
            config,
            dirty: false,
            active_section: SettingsSection::Appearance,
            font_size_text,
            max_tokens_text,
            scrollback_text,
            api_key_text,
            model_text,
            system_prompt_text,
            font_family_text,
        }
    }

    /// Apply a single field change to the working config.
    pub fn apply_field(&mut self, field: SettingsField) {
        match field {
            // ── Appearance ──
            SettingsField::FontSize(val) => {
                self.font_size_text = val.clone();
                if let Ok(v) = val.parse::<f32>() {
                    if v >= 6.0 && v <= 72.0 {
                        self.config.appearance.font_size = v;
                    }
                }
            }
            SettingsField::Theme(val) => {
                self.config.appearance.theme = val;
            }
            SettingsField::FontFamily(val) => {
                self.font_family_text = val.clone();
                self.config.appearance.font_family = val;
            }

            // ── AI ──
            SettingsField::DefaultProvider(val) => {
                // When switching provider, update the model/key text fields.
                self.config.ai.default_provider = val.clone();
                self.model_text = Self::model_for_provider(&self.config, &val);
                self.api_key_text = Self::api_key_for_provider(&self.config, &val);
            }
            SettingsField::AIModel(val) => {
                self.model_text = val.clone();
                let provider = self.config.ai.default_provider.clone();
                Self::ensure_provider_entry(&mut self.config, &provider).model = val;
            }
            SettingsField::AIApiKey(val) => {
                self.api_key_text = val.clone();
                let key = if val.is_empty() { None } else { Some(val) };
                let provider = self.config.ai.default_provider.clone();
                Self::ensure_provider_entry(&mut self.config, &provider).api_key = key;
            }
            SettingsField::Temperature(val) => {
                self.config.ai.temperature = val;
            }
            SettingsField::MaxTokens(val) => {
                self.max_tokens_text = val.clone();
                if let Ok(v) = val.parse::<u32>() {
                    self.config.ai.max_tokens = v;
                }
            }
            SettingsField::SystemPrompt(val) => {
                self.system_prompt_text = val.clone();
                self.config.ai.system_prompt = val;
            }

            // ── Terminal ──
            SettingsField::ScrollbackLines(val) => {
                self.scrollback_text = val.clone();
                if let Ok(v) = val.parse::<usize>() {
                    self.config.terminal.scrollback_lines = v;
                }
            }
            SettingsField::CursorBlink(val) => {
                self.config.terminal.cursor_blink = val;
            }
            SettingsField::CopyOnSelect(val) => {
                self.config.terminal.copy_on_select = val;
            }
        }

        self.dirty = true;
    }

    /// Save the working config to disk.
    pub fn save(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        self.config.save(&AppConfig::config_path())?;
        self.dirty = false;
        Ok(())
    }

    // ── Helpers ──

    fn api_key_for_provider(config: &AppConfig, provider: &str) -> String {
        let entry = match provider {
            "openai" => config.ai.providers.openai.as_ref(),
            "anthropic" => config.ai.providers.anthropic.as_ref(),
            "gemini" => config.ai.providers.gemini.as_ref(),
            "xai" => config.ai.providers.xai.as_ref(),
            "lmstudio" => config.ai.providers.lmstudio.as_ref(),
            "ollama" => config.ai.providers.ollama.as_ref(),
            _ => None,
        };
        entry
            .and_then(|e| e.api_key.clone())
            .unwrap_or_default()
    }

    fn model_for_provider(config: &AppConfig, provider: &str) -> String {
        let entry = match provider {
            "openai" => config.ai.providers.openai.as_ref(),
            "anthropic" => config.ai.providers.anthropic.as_ref(),
            "gemini" => config.ai.providers.gemini.as_ref(),
            "xai" => config.ai.providers.xai.as_ref(),
            "lmstudio" => config.ai.providers.lmstudio.as_ref(),
            "ollama" => config.ai.providers.ollama.as_ref(),
            _ => None,
        };
        entry
            .map(|e| e.model.clone())
            .unwrap_or_default()
    }

    fn ensure_provider_entry<'a>(
        config: &'a mut AppConfig,
        provider: &str,
    ) -> &'a mut altermative_config::ProviderEntry {
        use altermative_config::ProviderEntry;
        let slot = match provider {
            "openai" => &mut config.ai.providers.openai,
            "anthropic" => &mut config.ai.providers.anthropic,
            "gemini" => &mut config.ai.providers.gemini,
            "xai" => &mut config.ai.providers.xai,
            "lmstudio" => &mut config.ai.providers.lmstudio,
            "ollama" => &mut config.ai.providers.ollama,
            _ => {
                // Unknown provider — park it in openai slot as a fallback.
                &mut config.ai.providers.openai
            }
        };
        slot.get_or_insert_with(|| ProviderEntry::for_provider(provider, ""))
    }
}
