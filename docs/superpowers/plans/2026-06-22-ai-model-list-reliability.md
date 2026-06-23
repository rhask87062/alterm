# AI Model-List Reliability Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the AI chat model dropdown reliably populated — instant on session restore, offline-tolerant, and never silently collapsing to a free-text box.

**Architecture:** A new `ai::model_cache` module persists the last-known model list per provider to `~/.config/alterm/model-cache.json`. The flow is cache-first: panes are seeded from the cache instantly, and a network refresh fires only when the cache is missing or stale. `fetch_models` returns a typed `Result` so the UI shows a specific reason + Retry instead of a text box. All model fetch/apply is keyed by **provider string** and applied across all tabs' AI panes (sidesteps cross-tab pane-id ambiguity and dedupes fetches).

**Tech Stack:** Rust (edition 2021), iced 0.14 (Message/update/Task), serde + serde_json, reqwest, tokio.

## Global Constraints

- Cache file: `~/.config/alterm/model-cache.json`; freshness TTL `ai::model_cache::MODEL_CACHE_TTL_SECS = 86400` (24h).
- The `ai` crate must NOT depend on `alterm-config`. `model_cache` takes a directory `&Path` and current time `u64` as arguments; the app passes `AppConfig::config_dir()` and `now_secs()`.
- Model fetch results are keyed by **provider name** and applied to every `Block::AIChat` pane in every tab whose `provider_name` matches. Never address AI panes by `pane_grid::Pane` id across tabs (ids are not unique across tabs).
- A failed background refresh must NEVER wipe a non-empty `available_models`. `models_error` is only surfaced when `available_models` is empty.
- Use iced 0.14 patterns already in `alterm/src/main.rs` (`Task::done`, `Task::perform`, `Task::batch`, `self.update(...)`).
- Code exploration uses jCodemunch MCP tools, not raw file search.
- Out of scope (a later "Improvement B" spec): Anthropic live `/v1/models` fetch, filtering OpenAI's non-chat models, persisting the selected model to config.

---

### Task 1: `ai::model_cache` module (persisted cache + TTL lookup)

**Files:**
- Create: `crates/ai/src/model_cache.rs`
- Modify: `crates/ai/src/lib.rs` (add `pub mod model_cache;`)
- Test: inline `#[cfg(test)]` in `crates/ai/src/model_cache.rs`

**Interfaces:**
- Produces:
  - `pub const MODEL_CACHE_TTL_SECS: u64`
  - `pub struct CachedProvider { pub models: Vec<String>, pub fetched_at: u64 }`
  - `pub struct ModelCache { pub providers: HashMap<String, CachedProvider> }` (derives `Default`)
  - `pub struct CacheLookup { pub models: Vec<String>, pub needs_refresh: bool }`
  - `impl ModelCache { pub fn lookup(&self, provider: &str, now: u64, ttl: u64) -> CacheLookup; pub fn put(&mut self, provider: &str, models: Vec<String>, now: u64) }`
  - `pub fn load(dir: &Path) -> ModelCache`
  - `pub fn save(dir: &Path, cache: &ModelCache)`

- [ ] **Step 1: Write the module with its failing tests**

Create `crates/ai/src/model_cache.rs`:

```rust
//! Persisted per-provider model-list cache.
//!
//! Stores the last-known list of models for each AI provider on disk so the
//! model dropdown is populated instantly on launch / session restore and stays
//! usable when the provider is briefly unreachable. The caller supplies the
//! directory (so this module stays decoupled from the config crate) and the
//! current time (so staleness logic is deterministic in tests).

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// How long a cached model list is considered fresh (24 hours).
pub const MODEL_CACHE_TTL_SECS: u64 = 24 * 60 * 60;

const CACHE_FILE: &str = "model-cache.json";

/// One provider's cached model list and when it was fetched.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CachedProvider {
    pub models: Vec<String>,
    /// Unix timestamp (seconds) the list was fetched.
    pub fetched_at: u64,
}

/// The whole cache: provider name -> cached list.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ModelCache {
    pub providers: HashMap<String, CachedProvider>,
}

/// Result of a cache lookup: the models to show now, and whether the caller
/// should kick off a background refresh.
pub struct CacheLookup {
    pub models: Vec<String>,
    pub needs_refresh: bool,
}

impl ModelCache {
    /// Look up a provider. Returns its cached models (empty if none) and whether
    /// a refresh is due (missing entry, or older than `ttl` seconds).
    pub fn lookup(&self, provider: &str, now: u64, ttl: u64) -> CacheLookup {
        match self.providers.get(provider) {
            Some(entry) => {
                let stale = now.saturating_sub(entry.fetched_at) > ttl;
                CacheLookup { models: entry.models.clone(), needs_refresh: stale }
            }
            None => CacheLookup { models: Vec::new(), needs_refresh: true },
        }
    }

    /// Insert or replace a provider's model list, stamped at `now`.
    pub fn put(&mut self, provider: &str, models: Vec<String>, now: u64) {
        self.providers.insert(
            provider.to_string(),
            CachedProvider { models, fetched_at: now },
        );
    }
}

fn cache_path(dir: &Path) -> PathBuf {
    dir.join(CACHE_FILE)
}

/// Load the cache from `<dir>/model-cache.json`. A missing or corrupt file
/// yields an empty cache — never an error or panic.
pub fn load(dir: &Path) -> ModelCache {
    let path = cache_path(dir);
    let contents = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(_) => return ModelCache::default(),
    };
    match serde_json::from_str(&contents) {
        Ok(cache) => cache,
        Err(e) => {
            log::warn!("Ignoring corrupt model cache at {path:?}: {e}");
            ModelCache::default()
        }
    }
}

/// Write the cache to `<dir>/model-cache.json`. Best-effort: logs on failure,
/// never panics.
pub fn save(dir: &Path, cache: &ModelCache) {
    let path = cache_path(dir);
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    match serde_json::to_string_pretty(cache) {
        Ok(json) => {
            if let Err(e) = std::fs::write(&path, json) {
                log::warn!("Failed to write model cache to {path:?}: {e}");
            }
        }
        Err(e) => log::warn!("Failed to serialize model cache: {e}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("alterm-modelcache-{tag}"));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn lookup_missing_needs_refresh() {
        let cache = ModelCache::default();
        let r = cache.lookup("openai", 1000, MODEL_CACHE_TTL_SECS);
        assert!(r.models.is_empty());
        assert!(r.needs_refresh);
    }

    #[test]
    fn lookup_fresh_does_not_need_refresh() {
        let mut cache = ModelCache::default();
        cache.put("openai", vec!["gpt-4o".into()], 1000);
        let r = cache.lookup("openai", 1000 + 10, MODEL_CACHE_TTL_SECS);
        assert_eq!(r.models, vec!["gpt-4o".to_string()]);
        assert!(!r.needs_refresh);
    }

    #[test]
    fn lookup_stale_needs_refresh_but_returns_models() {
        let mut cache = ModelCache::default();
        cache.put("openai", vec!["gpt-4o".into()], 1000);
        let r = cache.lookup("openai", 1000 + MODEL_CACHE_TTL_SECS + 1, MODEL_CACHE_TTL_SECS);
        assert_eq!(r.models, vec!["gpt-4o".to_string()]);
        assert!(r.needs_refresh);
    }

    #[test]
    fn load_missing_file_is_empty() {
        let dir = temp_dir("missing");
        assert!(load(&dir).providers.is_empty());
    }

    #[test]
    fn load_corrupt_file_is_empty() {
        let dir = temp_dir("corrupt");
        std::fs::write(dir.join("model-cache.json"), "{ not json").unwrap();
        assert!(load(&dir).providers.is_empty());
    }

    #[test]
    fn save_then_load_roundtrip() {
        let dir = temp_dir("roundtrip");
        let mut cache = ModelCache::default();
        cache.put("anthropic", vec!["claude-x".into(), "claude-y".into()], 42);
        save(&dir, &cache);
        let loaded = load(&dir);
        let entry = loaded.providers.get("anthropic").unwrap();
        assert_eq!(entry.models, vec!["claude-x".to_string(), "claude-y".to_string()]);
        assert_eq!(entry.fetched_at, 42);
    }
}
```

In `crates/ai/src/lib.rs`, add the module declaration next to the existing ones (after `pub mod openai;`):

```rust
pub mod anthropic;
pub mod gemini;
pub mod openai;
pub mod model_cache;
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p ai model_cache`
Expected: compile error / FAIL — module is new and not yet compiled into a passing state (or fails until the file is saved correctly).

- [ ] **Step 3: Make tests pass**

The code in Step 1 is the implementation. Fix any compile errors until green.

Run: `cargo test -p ai model_cache`
Expected: PASS — 6 tests pass.

- [ ] **Step 4: Commit**

```bash
git add crates/ai/src/model_cache.rs crates/ai/src/lib.rs
git commit -m "feat(ai): persisted per-provider model-list cache"
```

---

### Task 2: Typed fetch errors + testable parsing in `ai`

**Files:**
- Modify: `crates/ai/src/lib.rs` (the "Model listing" section: `fetch_models`, `fetch_openai_compatible_models`, `fetch_gemini_models`; add `ModelFetchError`, `provider_requires_key`, `parse_openai_models`, `parse_gemini_models`)
- Test: inline `#[cfg(test)]` in `crates/ai/src/lib.rs`

**Interfaces:**
- Consumes: nothing from Task 1.
- Produces:
  - `pub enum ModelFetchError { MissingApiKey, Unauthorized, Unreachable, BadResponse }` with `pub fn user_message(&self) -> String`
  - `pub fn provider_requires_key(provider: &str) -> bool`
  - `pub async fn fetch_models(base_url: &str, api_key: Option<&str>, provider_type: &str) -> Result<Vec<String>, ModelFetchError>`
  - `pub fn parse_openai_models(body: &serde_json::Value) -> Vec<String>`
  - `pub fn parse_gemini_models(body: &serde_json::Value) -> Vec<String>`

- [ ] **Step 1: Add the error type, key helper, and pure parsers (with tests)**

In `crates/ai/src/lib.rs`, replace the entire `// Model listing` section (from the `/// Fetch available models...` doc comment down through the end of `fetch_gemini_models`) with:

```rust
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
    Ok(parse_openai_models(&body))
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
    Ok(parse_gemini_models(&body))
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
```

Then add a test module at the end of `crates/ai/src/lib.rs`:

```rust
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
        assert!(!ModelFetchError::Unreachable.user_message().is_empty());
        assert!(!ModelFetchError::BadResponse.user_message().is_empty());
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
}
```

> Note: `anthropic_hardcoded_models` is preserved verbatim (still out of scope to make it live — that's Improvement B). Its values are unchanged.

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p ai model_listing`
Expected: FAIL to compile first time if anything is mistyped; otherwise the 5 new tests run. (They will not exist/pass until the code is in.)

- [ ] **Step 3: Fix compile errors until green**

Run: `cargo test -p ai`
Expected: PASS — Task 1 (6) + Task 2 (5) tests pass.

- [ ] **Step 4: Commit**

```bash
git add crates/ai/src/lib.rs
git commit -m "feat(ai): typed ModelFetchError + testable model parsing"
```

---

### Task 3: `AIChatState` fields + `selector_mode` helper

**Files:**
- Modify: `crates/workspace/src/ai_chat.rs` (add fields, init, `SelectorMode`, `selector_mode()`, tests)
- Modify: `crates/workspace/src/lib.rs:17` (re-export `SelectorMode`)
- Test: inline `#[cfg(test)]` in `crates/workspace/src/ai_chat.rs`

**Interfaces:**
- Produces:
  - `AIChatState` gains `pub models_error: Option<String>` and `pub custom_model_entry: bool`
  - `pub enum SelectorMode { Custom, List, Loading, Error, Empty }`
  - `impl AIChatState { pub fn selector_mode(&self) -> SelectorMode }`
  - Re-exported as `workspace::SelectorMode`

- [ ] **Step 1: Add the two fields**

In `crates/workspace/src/ai_chat.rs`, in the `AIChatState` struct, after the `pub models_loading: bool,` field, add:

```rust
    /// Human-readable reason the last model fetch failed. Only surfaced in the
    /// UI when there are no models to show.
    pub models_error: Option<String>,
    /// Whether the user opted into typing a custom model name.
    pub custom_model_entry: bool,
```

In `AIChatState::new`, after `available_models: Vec::new(),` and `models_loading: false,`, add:

```rust
            models_error: None,
            custom_model_entry: false,
```

- [ ] **Step 2: Add `SelectorMode` + `selector_mode()` with failing tests**

In `crates/workspace/src/ai_chat.rs`, add after the `impl AIChatState { ... }` block:

```rust
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

impl AIChatState {
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
```

- [ ] **Step 3: Re-export `SelectorMode`**

In `crates/workspace/src/lib.rs`, change line 17 from:

```rust
pub use ai_chat::AIChatState;
```

to:

```rust
pub use ai_chat::{AIChatState, SelectorMode};
```

- [ ] **Step 4: Run tests to verify they fail, then pass**

Run: `cargo test -p workspace selector`
Expected first run: FAIL/compile error until code is in. After Steps 1–3: PASS — 6 tests pass.

- [ ] **Step 5: Commit**

```bash
git add crates/workspace/src/ai_chat.rs crates/workspace/src/lib.rs
git commit -m "feat(workspace): AIChatState models_error/custom flag + selector_mode"
```

---

### Task 4: Wire cache + provider-keyed fetch into `Alterm`

**Files:**
- Modify: `alterm/src/main.rs`
  - import line `:2`
  - add `now_secs()` helper
  - `Alterm` struct (`:216`)
  - constructor: cache load + seed + struct literal + return (`~:420–488`)
  - `Message` enum AI variants (`:345–346`, add one after `:344`)
  - `ToggleAIChat` handler (`~:1374`)
  - `AIProviderChanged` handler (`~:1459`)
  - `AIModelChanged` handler region — add `AIToggleCustomModel` (`~:1470`)
  - `AIFetchModels` handler (`~:1476–1516`)
  - `AIModelsFetched` handler (`~:1580–1588`)

**Interfaces:**
- Consumes: `ai::model_cache::{load, save, ModelCache, MODEL_CACHE_TTL_SECS}`, `ai::{fetch_models, ModelFetchError}`, `workspace::Block::AIChat`.
- Produces:
  - `Alterm` field `model_cache: ai::model_cache::ModelCache`
  - `fn now_secs() -> u64`
  - `fn refresh_all_model_lists(&mut self) -> Task<Message>`
  - Messages: `AIFetchModels(String, bool)`, `AIModelsFetched(String, Result<Vec<String>, ai::ModelFetchError>)`, `AIToggleCustomModel(pane_grid::Pane)`

- [ ] **Step 1: Imports + `now_secs` helper**

In `alterm/src/main.rs`, change line 2 from:

```rust
use std::time::{Duration, Instant};
```

to:

```rust
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
```

Add this free function near `fn rename_input_id()` (around line 258):

```rust
/// Current unix time in whole seconds (0 if the clock is before the epoch).
fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}
```

- [ ] **Step 2: Add the `model_cache` field to `Alterm`**

In the `struct Alterm { ... }` definition, after the `search: Option<SearchState>,` field, add:

```rust
    /// Persisted per-provider model-list cache (see `ai::model_cache`).
    model_cache: ai::model_cache::ModelCache,
```

- [ ] **Step 3: Load + seed the cache in the constructor**

In the constructor, immediately BEFORE the `let app = Alterm {` line, insert:

```rust
        // Load the persisted model-list cache and seed every AI chat pane so
        // its dropdown is populated instantly (before any network call).
        let model_cache = ai::model_cache::load(&AppConfig::config_dir());
        let mut tabs = tabs;
        for tab in &mut tabs {
            for (_pane, block) in tab.panes.iter_mut() {
                if let Block::AIChat { state } = block {
                    if state.available_models.is_empty() {
                        let hit = model_cache.lookup(
                            &state.provider_name,
                            now_secs(),
                            ai::model_cache::MODEL_CACHE_TTL_SECS,
                        );
                        state.available_models = hit.models;
                    }
                }
            }
        }
```

In the `let app = Alterm { ... };` struct literal, add `model_cache,` after `search: None,`. Change `let app` to `let mut app`:

```rust
        let mut app = Alterm {
            // ...existing fields...
            search: None,
            model_cache,
        };
```

Replace the return line `(app, fetch_handle)` with:

```rust
        // Cache-first refresh of every provider in use (only hits the network
        // where the cached list is missing or stale).
        let refresh_models = app.refresh_all_model_lists();
        (app, Task::batch([fetch_handle, refresh_models]))
```

- [ ] **Step 4: Add `refresh_all_model_lists`**

Add this method to the `impl Alterm` block (near `active_tab_mut`, around line 496):

```rust
    /// Dispatch a deduped, cache-first model-list refresh for every provider
    /// currently in use across all tabs.
    fn refresh_all_model_lists(&mut self) -> Task<Message> {
        let mut providers: Vec<String> = Vec::new();
        for tab in &self.tabs {
            for (_pane, block) in tab.panes.iter() {
                if let Block::AIChat { state } = block {
                    if !providers.contains(&state.provider_name) {
                        providers.push(state.provider_name.clone());
                    }
                }
            }
        }
        Task::batch(
            providers
                .into_iter()
                .map(|p| Task::done(Message::AIFetchModels(p, false))),
        )
    }
```

- [ ] **Step 5: Update the `Message` enum**

In the `enum Message`, replace:

```rust
    AIFetchModels(pane_grid::Pane),
    AIModelsFetched(pane_grid::Pane, Vec<String>),
```

with:

```rust
    AIFetchModels(String, bool),
    AIModelsFetched(String, Result<Vec<String>, ai::ModelFetchError>),
```

And add, right after `AIModelChanged(pane_grid::Pane, String),`:

```rust
    AIToggleCustomModel(pane_grid::Pane),
```

- [ ] **Step 6: Update `ToggleAIChat` to fetch by provider**

In the `Message::ToggleAIChat` handler, change the block-creation + fetch lines from:

```rust
                let block = Block::new_ai_chat(provider_name, model_name);
                let new_pane = self.add_window(block);
                let focus_task = widget_focus(WidgetId::from(
                    format!("ai-chat-input-{:?}", new_pane),
                ));
                let fetch_task = self.update(Message::AIFetchModels(new_pane));
                return Task::batch([focus_task, fetch_task]);
```

to:

```rust
                let block = Block::new_ai_chat(provider_name.clone(), model_name);
                let new_pane = self.add_window(block);
                let focus_task = widget_focus(WidgetId::from(
                    format!("ai-chat-input-{:?}", new_pane),
                ));
                let fetch_task = self.update(Message::AIFetchModels(provider_name, false));
                return Task::batch([focus_task, fetch_task]);
```

- [ ] **Step 7: Update `AIProviderChanged` + add `AIToggleCustomModel`**

Replace the `Message::AIProviderChanged` handler with:

```rust
            Message::AIProviderChanged(pane, provider) => {
                let new_model = self.config.ai.provider_model(&provider);
                let tab = self.active_tab_mut();
                if let Some(Block::AIChat { state }) = tab.panes.get_mut(pane) {
                    state.provider_name = provider.clone();
                    state.model_name = new_model;
                    state.available_models.clear();
                    state.models_error = None;
                }
                // Force a fresh model list for the newly selected provider.
                return self.update(Message::AIFetchModels(provider, true));
            }
```

Immediately after the `Message::AIModelChanged` handler, add:

```rust
            Message::AIToggleCustomModel(pane) => {
                let tab = self.active_tab_mut();
                if let Some(Block::AIChat { state }) = tab.panes.get_mut(pane) {
                    state.custom_model_entry = !state.custom_model_entry;
                }
            }
```

- [ ] **Step 8: Replace the `AIFetchModels` handler**

Replace the entire `Message::AIFetchModels(pane) => { ... }` handler with:

```rust
            Message::AIFetchModels(provider, force) => {
                if provider.is_empty() {
                    return Task::none();
                }

                // Seed matching panes from cache so the dropdown shows instantly.
                let hit = self.model_cache.lookup(
                    &provider,
                    now_secs(),
                    ai::model_cache::MODEL_CACHE_TTL_SECS,
                );
                let seed = hit.models.clone();
                for tab in &mut self.tabs {
                    for (_pane, block) in tab.panes.iter_mut() {
                        if let Block::AIChat { state } = block {
                            if state.provider_name == provider && state.available_models.is_empty() {
                                state.available_models = seed.clone();
                            }
                        }
                    }
                }

                if !force && !hit.needs_refresh {
                    return Task::none(); // cache still fresh — no network call
                }

                // Mark matching panes as loading.
                for tab in &mut self.tabs {
                    for (_pane, block) in tab.panes.iter_mut() {
                        if let Block::AIChat { state } = block {
                            if state.provider_name == provider {
                                state.models_loading = true;
                                state.models_error = None;
                            }
                        }
                    }
                }

                // Resolve connection details from config.
                let entry = self.config.ai.providers.get(&provider);
                let base_url = entry
                    .map(|e| e.resolved_base_url(&provider))
                    .unwrap_or_else(|| {
                        alterm_config::default_base_url(&provider).unwrap_or("").to_string()
                    });
                let api_key = entry.and_then(|e| e.api_key.clone());

                let p = provider.clone();
                return Task::perform(
                    async move { ai::fetch_models(&base_url, api_key.as_deref(), &p).await },
                    move |result| Message::AIModelsFetched(provider.clone(), result),
                );
            }
```

- [ ] **Step 9: Replace the `AIModelsFetched` handler**

Replace the entire `Message::AIModelsFetched(pane, models) => { ... }` handler with:

```rust
            Message::AIModelsFetched(provider, result) => {
                match result {
                    Ok(models) => {
                        self.model_cache.put(&provider, models.clone(), now_secs());
                        ai::model_cache::save(&AppConfig::config_dir(), &self.model_cache);
                        for tab in &mut self.tabs {
                            for (_pane, block) in tab.panes.iter_mut() {
                                if let Block::AIChat { state } = block {
                                    if state.provider_name == provider {
                                        state.available_models = models.clone();
                                        state.models_loading = false;
                                        state.models_error = None;
                                    }
                                }
                            }
                        }
                    }
                    Err(err) => {
                        let msg = err.user_message();
                        for tab in &mut self.tabs {
                            for (_pane, block) in tab.panes.iter_mut() {
                                if let Block::AIChat { state } = block {
                                    if state.provider_name == provider {
                                        state.models_loading = false;
                                        // Only surface the error if there's nothing to show.
                                        if state.available_models.is_empty() {
                                            state.models_error = Some(msg.clone());
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
```

- [ ] **Step 10: Build**

Run: `cargo build -p alterm`
Expected: compiles cleanly (warnings ok). Fix any type/borrow errors. The model selector view still references only `available_models`/`models_loading` at this point, so it compiles unchanged.

- [ ] **Step 11: Commit**

```bash
git add alterm/src/main.rs
git commit -m "feat(alterm): cache-first, provider-keyed model fetch + restore seeding"
```

---

### Task 5: Model selector view — dropdown / loading / error+retry / custom

**Files:**
- Modify: `alterm/src/main.rs` — `ai_chat_view`, the `let model_selector: Element<...> = ...` block (`~:2538–2568`)

**Interfaces:**
- Consumes: `state.selector_mode()`, `workspace::SelectorMode`, `state.provider_name`, messages `AIModelChanged`, `AIFetchModels(String, bool)`, `AIToggleCustomModel(pane_grid::Pane)`.

- [ ] **Step 1: Replace the `model_selector` block**

In `ai_chat_view`, replace the entire `let model_selector: Element<'a, Message> = if !state.available_models.is_empty() { ... } else { ... };` block with:

```rust
    let model_selector: Element<'a, Message> = match state.selector_mode() {
        workspace::SelectorMode::Custom => row![
            text_input("model name", &state.model_name)
                .on_input(move |v| Message::AIModelChanged(pane, v))
                .size(11)
                .padding(Padding::from([2, 6]))
                .width(Length::Fixed(180.0)),
            button(text("↩ list").size(10))
                .on_press(Message::AIToggleCustomModel(pane))
                .padding(Padding::from([2, 6])),
        ]
        .spacing(4)
        .align_y(iced::Alignment::Center)
        .into(),

        workspace::SelectorMode::List => {
            let mut models = state.available_models.clone();
            // Keep the current model selectable even if the API didn't list it.
            if !state.model_name.is_empty() && !models.contains(&state.model_name) {
                models.insert(0, state.model_name.clone());
            }
            pick_list(
                models,
                Some(state.model_name.clone()),
                move |selected| Message::AIModelChanged(pane, selected),
            )
            .text_size(11)
            .padding(Padding::from([2, 6]))
            .width(Length::Fixed(220.0))
            .into()
        }

        workspace::SelectorMode::Loading => container(
            text("Loading models…").size(10).color(Color::from_rgb(0.50, 0.50, 0.55)),
        )
        .padding(Padding::from([4, 8]))
        .into(),

        workspace::SelectorMode::Error => {
            let reason = state.models_error.clone().unwrap_or_default();
            let provider = state.provider_name.clone();
            row![
                text(format!("⚠ {reason}")).size(10).color(Color::from_rgb(0.95, 0.55, 0.35)),
                button(text("Retry").size(10))
                    .on_press(Message::AIFetchModels(provider, true))
                    .padding(Padding::from([2, 6])),
                button(text("custom").size(10))
                    .on_press(Message::AIToggleCustomModel(pane))
                    .padding(Padding::from([2, 6])),
            ]
            .spacing(6)
            .align_y(iced::Alignment::Center)
            .into()
        }

        workspace::SelectorMode::Empty => {
            let provider = state.provider_name.clone();
            button(text("Load models").size(10))
                .on_press(Message::AIFetchModels(provider, true))
                .padding(Padding::from([2, 6]))
                .into()
        }
    };
```

- [ ] **Step 2: Build**

Run: `cargo build -p alterm`
Expected: compiles cleanly. If `row!`, `container`, `button`, `text`, `text_input`, `pick_list`, `Length`, `Padding`, `Color` are not already in scope in this file, they are — they're used elsewhere in `ai_chat_view`; do not add imports unless the compiler complains.

- [ ] **Step 3: Run the full test suite**

Run: `cargo test`
Expected: PASS — all crates green (browser 13, workspace +6, ai +11, etc.).

- [ ] **Step 4: Commit**

```bash
git add alterm/src/main.rs
git commit -m "feat(alterm): model selector states (list/loading/error+retry/custom)"
```

---

### Task 6: Manual verification

**Files:** none (runtime verification).

> AI chat panes are ordinary iced widgets (not native webviews), so synthetic input / screenshots work for these, unlike the browser panes.

- [ ] **Step 1: Build a debug binary with logging**

```bash
cargo build -p alterm
```

- [ ] **Step 2: Verify cache write on first fetch**

Launch a **separate** debug instance (never kill the user's running `~/.cargo/bin/alterm`):

```bash
RUST_LOG=alterm=debug,ai=debug nohup ./target/debug/alterm > /tmp/alterm-ai.log 2>&1 & disown
```

Open an AI chat pane (sidebar AI button). With a configured provider (default is `ollama` at `http://localhost:11434/v1`), confirm:
- the model dropdown populates (if Ollama is running), and
- `~/.config/alterm/model-cache.json` now exists and contains that provider:

```bash
cat ~/.config/alterm/model-cache.json
```

Expected: JSON with a `providers` map containing the provider and a non-empty `models` array (when the provider is reachable).

- [ ] **Step 3: Verify instant restore from cache (the core fix)**

With `session.restore` enabled and an AI pane open, close that instance, then relaunch:

```bash
RUST_LOG=alterm=debug,ai=debug nohup ./target/debug/alterm > /tmp/alterm-ai2.log 2>&1 & disown
```

Expected: the restored AI pane shows the model **dropdown immediately** (seeded from `model-cache.json`) — NOT a free-text box.

- [ ] **Step 4: Verify the empty/error state + Retry + custom**

Switch the pane's provider to a key-requiring one with no key configured (e.g. `openai`). Expected: header shows `⚠ No API key — add one in Settings`, a `Retry` button, and a `custom` button. Click `custom` → a `model name` text field + `↩ list` appears. Click `↩ list` → returns to the error/list state. Clicking `Retry` re-attempts the fetch.

- [ ] **Step 5: Clean up the test instance**

Find and kill only the PID(s) you launched (never `pkill -f alterm`, never the user's instance):

```bash
ps aux | grep "target/debug/alterm" | grep -v grep
kill <your-debug-pid>
```

- [ ] **Step 6: Final commit (if any verification fixes were made)**

```bash
git add -A
git commit -m "fix(alterm): address AI model-list verification findings"
```

> Building the release, installing to `~/.cargo/bin/alterm`, merging to `main`, and pushing are user-triggered — do not do them without explicit confirmation.

---

## Self-Review

**1. Spec coverage:**
- Persisted cache + background refresh → Task 1 (cache) + Task 4 (load/seed/refresh/save). ✓
- Empty-state = error + Retry + tucked custom → Task 3 (`SelectorMode::{Error,Custom}`) + Task 5 (view) + Task 4 (`AIToggleCustomModel`). ✓
- Stale-only + manual refresh → Task 1 (`lookup.needs_refresh`/TTL) + Task 4 (`force` flag; provider-change & Retry force, startup/new-chat don't). ✓
- Approach A (model logic in `ai`) → Tasks 1–2. ✓
- Session-restore trigger (the main gap) → Task 4 Step 3 (seed) + Step 4 (`refresh_all_model_lists`) wired into the constructor return. ✓
- Typed `ModelFetchError` powering specific messages → Task 2 + Task 4 Step 9. ✓
- Failed refresh never wipes a usable list → Task 4 Step 9 (`if available_models.is_empty()`). ✓
- Testing (cache TTL/corrupt, parsers, selector branches) → Tasks 1–3. ✓
- Out-of-scope items untouched (Anthropic hardcode preserved verbatim; no OpenAI filtering; no model-to-config persistence). ✓

**2. Placeholder scan:** No TBD/TODO; every code step contains complete code and exact commands. ✓

**3. Type consistency:** `AIFetchModels(String, bool)` and `AIModelsFetched(String, Result<Vec<String>, ai::ModelFetchError>)` are used identically in the enum (Task 4 Step 5), the dispatch sites (Steps 3,4,6,7), the handlers (Steps 8,9), and the view (Task 5). `selector_mode()` / `SelectorMode` variants match between Task 3 and Task 5. `model_cache::{load,save,lookup,put,MODEL_CACHE_TTL_SECS}` signatures match between Task 1 and Task 4. ✓

**Note on cross-tab addressing:** all fetch results are applied by matching `provider_name` across every tab's panes (never by `pane_grid::Pane` id), which is why no tab id is threaded through the messages — consistent with the Global Constraints.
