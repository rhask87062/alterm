pub mod hooks;
pub mod theme;

use serde::{Deserialize, Serialize};
use std::error::Error;
use std::path::{Path, PathBuf};

// ── AppConfig ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AppConfig {
    pub general: GeneralConfig,
    pub ai: AIConfig,
    pub appearance: AppearanceConfig,
    pub terminal: TerminalConfig,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            general: GeneralConfig::default(),
            ai: AIConfig::default(),
            appearance: AppearanceConfig::default(),
            terminal: TerminalConfig::default(),
        }
    }
}

impl AppConfig {
    /// Load config from `path`. Returns default config if the file does not exist.
    pub fn load(path: &Path) -> Result<Self, Box<dyn Error>> {
        if !path.exists() {
            log::info!("Config file not found at {:?}, using defaults", path);
            return Ok(Self::default());
        }
        let contents = std::fs::read_to_string(path)?;
        let config: Self = toml::from_str(&contents)?;
        log::info!("Loaded config from {:?}", path);
        Ok(config)
    }

    /// Save config to `path` as pretty-printed TOML. Creates parent directories if needed.
    pub fn save(&self, path: &Path) -> Result<(), Box<dyn Error>> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let toml_str = toml::to_string_pretty(self)?;
        std::fs::write(path, toml_str)?;
        log::info!("Saved config to {:?}", path);
        Ok(())
    }

    /// Returns `$XDG_CONFIG_HOME/altermative/` (or `~/.config/altermative/`).
    pub fn config_dir() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| {
                dirs::home_dir()
                    .unwrap_or_else(|| PathBuf::from("."))
                    .join(".config")
            })
            .join("altermative")
    }

    /// Returns the default config file path: `config_dir()/config.toml`.
    pub fn config_path() -> PathBuf {
        Self::config_dir().join("config.toml")
    }

    /// Returns the Lua hooks file path: `config_dir()/hooks.lua`.
    pub fn hooks_path() -> PathBuf {
        Self::config_dir().join("hooks.lua")
    }
}

// ── GeneralConfig ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct GeneralConfig {
    /// Shell to use. `None` means inherit `$SHELL` from the environment.
    pub default_shell: Option<String>,
}

impl Default for GeneralConfig {
    fn default() -> Self {
        Self {
            default_shell: None,
        }
    }
}

// ── AIConfig ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AIConfig {
    /// Which provider to use when none is explicitly selected.
    pub default_provider: String,
    /// Maximum number of tokens to request from the model.
    pub max_tokens: u32,
    /// Sampling temperature (0.0–1.0).
    pub temperature: f32,
    /// System prompt sent to the AI on every conversation.
    pub system_prompt: String,
    /// Per-provider connection details.
    pub providers: AIProviders,
}

impl AIConfig {
    /// Get the model name for a given provider, falling back to a sensible default.
    pub fn provider_model(&self, provider: &str) -> String {
        self.providers.get(provider)
            .map(|e| e.model.clone())
            .filter(|m| !m.is_empty())
            .unwrap_or_else(|| match provider {
                "openai" => "gpt-4o".to_string(),
                "anthropic" => "claude-sonnet-4-20250514".to_string(),
                "google" => "gemini-2.0-flash".to_string(),
                "xai" => "grok-3".to_string(),
                "lmstudio" => "default".to_string(),
                "ollama" => "llama3.2".to_string(),
                _ => "default".to_string(),
            })
    }
}

impl Default for AIConfig {
    fn default() -> Self {
        Self {
            default_provider: "ollama".to_string(),
            max_tokens: 4096,
            temperature: 0.7,
            system_prompt: "You are a helpful terminal assistant. \
                You can see the user's terminal output for context."
                .to_string(),
            providers: AIProviders::default(),
        }
    }
}

// ── AIProviders ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AIProviders {
    pub openai: Option<ProviderEntry>,
    pub anthropic: Option<ProviderEntry>,
    pub google: Option<ProviderEntry>,
    pub xai: Option<ProviderEntry>,
    pub lmstudio: Option<ProviderEntry>,
    pub ollama: Option<ProviderEntry>,
}

impl AIProviders {
    /// Get the provider entry by name.
    pub fn get(&self, name: &str) -> Option<&ProviderEntry> {
        match name {
            "openai" => self.openai.as_ref(),
            "anthropic" => self.anthropic.as_ref(),
            "google" => self.google.as_ref(),
            "xai" => self.xai.as_ref(),
            "lmstudio" => self.lmstudio.as_ref(),
            "ollama" => self.ollama.as_ref(),
            _ => None,
        }
    }
}

impl Default for AIProviders {
    fn default() -> Self {
        Self {
            openai: None,
            anthropic: None,
            google: None,
            xai: None,
            lmstudio: None,
            // Ship a default Ollama entry so users can start immediately.
            ollama: Some(ProviderEntry {
                api_key: None,
                base_url: Some("http://localhost:11434/v1".to_string()),
                model: "llama3.2".to_string(),
            }),
        }
    }
}

// ── ProviderEntry ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ProviderEntry {
    /// API key for this provider. May be left `None` for local providers.
    pub api_key: Option<String>,
    /// Override the default base URL for this provider.
    pub base_url: Option<String>,
    /// Model identifier to use (e.g. `"gpt-4o"`, `"claude-opus-4-5"`).
    pub model: String,
}

impl Default for ProviderEntry {
    fn default() -> Self {
        Self {
            api_key: None,
            base_url: None,
            model: String::new(),
        }
    }
}

impl ProviderEntry {
    /// Construct an entry for a well-known provider using its canonical base URL.
    pub fn for_provider(name: &str, model: impl Into<String>) -> Self {
        let base_url = default_base_url(name).map(str::to_string);
        Self {
            api_key: None,
            base_url,
            model: model.into(),
        }
    }

    /// Resolved base URL: user-supplied value, or the provider's canonical default.
    pub fn resolved_base_url(&self, provider_name: &str) -> String {
        self.base_url
            .clone()
            .unwrap_or_else(|| default_base_url(provider_name).unwrap_or("").to_string())
    }
}

/// Returns the canonical base URL for a built-in provider name, if known.
pub fn default_base_url(provider: &str) -> Option<&'static str> {
    match provider {
        "openai" => Some("https://api.openai.com/v1"),
        "anthropic" => Some("https://api.anthropic.com/v1"),
        "google" => Some("https://generativelanguage.googleapis.com/v1beta"),
        "xai" => Some("https://api.x.ai/v1"),
        "lmstudio" => Some("http://localhost:1234/v1"),
        "ollama" => Some("http://localhost:11434/v1"),
        _ => None,
    }
}

// ── AppearanceConfig ───────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AppearanceConfig {
    /// Terminal font size in points.
    pub font_size: f32,
    /// Font family name (CSS-style, e.g. `"monospace"` or `"JetBrains Mono"`).
    pub font_family: String,
    /// Name of the active color theme (`"dark"` or `"light"`).
    pub theme: String,
}

impl Default for AppearanceConfig {
    fn default() -> Self {
        Self {
            font_size: 14.0,
            font_family: "monospace".to_string(),
            theme: "dark".to_string(),
        }
    }
}

// ── TerminalConfig ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct TerminalConfig {
    /// Number of lines kept in the scrollback buffer.
    pub scrollback_lines: usize,
    /// Whether the cursor should blink.
    pub cursor_blink: bool,
    /// Copy selected text to the clipboard automatically.
    pub copy_on_select: bool,
}

impl Default for TerminalConfig {
    fn default() -> Self {
        Self {
            scrollback_lines: 10000,
            cursor_blink: true,
            copy_on_select: false,
        }
    }
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_roundtrips() {
        let config = AppConfig::default();
        let toml_str = toml::to_string_pretty(&config).expect("serialize");
        let restored: AppConfig = toml::from_str(&toml_str).expect("deserialize");
        assert_eq!(restored.ai.default_provider, "ollama");
        assert_eq!(restored.ai.max_tokens, 4096);
        assert!((restored.ai.temperature - 0.7).abs() < f32::EPSILON);
        assert_eq!(restored.appearance.theme, "dark");
        assert_eq!(restored.terminal.scrollback_lines, 10000);
    }

    #[test]
    fn empty_toml_yields_defaults() {
        let config: AppConfig = toml::from_str("").expect("empty toml");
        assert_eq!(config.ai.default_provider, "ollama");
        assert_eq!(config.appearance.font_size, 14.0);
    }

    #[test]
    fn partial_override_merges_with_defaults() {
        let toml_str = r#"
[ai]
default_provider = "openai"
max_tokens = 2048
"#;
        let config: AppConfig = toml::from_str(toml_str).expect("partial toml");
        assert_eq!(config.ai.default_provider, "openai");
        assert_eq!(config.ai.max_tokens, 2048);
        // Fields not set in TOML fall back to defaults.
        assert!((config.ai.temperature - 0.7).abs() < f32::EPSILON);
        assert_eq!(config.appearance.theme, "dark");
    }

    #[test]
    fn default_base_urls_are_correct() {
        assert_eq!(
            default_base_url("openai"),
            Some("https://api.openai.com/v1")
        );
        assert_eq!(
            default_base_url("ollama"),
            Some("http://localhost:11434/v1")
        );
        assert_eq!(default_base_url("unknown"), None);
    }
}
