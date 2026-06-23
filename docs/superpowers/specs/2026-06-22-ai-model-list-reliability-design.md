# AI Model-List Reliability — Design

- **Date:** 2026-06-22
- **Status:** Approved (design)
- **Scope:** "Improvement A" — make the AI chat model dropdown reliable. List quality (Improvement B) is a separate, later spec.

## Problem

The AI chat already has a model dropdown (`pick_list` in `alterm/src/main.rs` `ai_chat_view`) backed by a real fetch layer (`ai::fetch_models`). In practice it silently collapses to a free-text "model name" box, forcing the user to type a model by hand. Root causes:

1. **Session-restored chats never fetch.** Restore goes through `Block::from_state` (`crates/workspace/src/block.rs`), which does not dispatch `AIFetchModels`. Only newly-created chats and provider-changes fetch. A restored AI pane therefore starts with an empty model list → text box.
2. **Fetch fails silently.** `ai::fetch_models` returns an empty `Vec` on *any* error (no API key, 401, provider offline, bad base URL). Empty is indistinguishable from "still loading," and the UI just shows the text box with no explanation.
3. **No recovery.** There is no manual refresh; once empty, the only way to re-trigger a fetch is to toggle the provider away and back.

## Decisions (locked during brainstorming)

1. **Persisted cache + background refresh.** Store the last-known model list per provider on disk (alongside `session.json`). Show it instantly on open/restore; refresh in the background. Offline/slow API still shows the last good list.
2. **Empty-state UX = error + Retry, custom tucked away.** When there is genuinely no list (e.g. a cloud provider with no API key, nothing cached), show a clear reason + a Retry button. A small "use a custom model" link reveals a text field. The dropdown stays the primary path; typing is an explicit escape hatch, never the forced default.
3. **Refresh cadence = stale-only + manual.** Show cached list instantly; background-refresh only when the cache is older than a TTL (~24h), on provider change, and on explicit Retry/refresh.
4. **Structure = approach A.** All model logic lives in the `ai` crate (new `ai::model_cache` module). `main.rs` orchestrates *when* to fetch and maps results into pane UI state.

## Architecture

Cache-first flow. Any trigger that needs models shows the cached list immediately and only fires a network fetch if the cache is missing or stale.

```
trigger (open / restore / provider-change / retry)
        │
        ▼
App.model_cache.lookup(provider, now, ttl)  ──►  { models, needs_refresh }
        │                                              │
        ├─ set pane.available_models = models          │ needs_refresh?
        │  (instant, even offline)                     ▼
        │                                   Task::perform(ai::fetch_models)
        ▼                                              │
   view renders dropdown                               ▼
                                        AIModelsFetched(pane, provider, Result)
                                                       │
                                   Ok  → update pane list + cache + save to disk
                                   Err → keep stale list, or show reason + Retry
```

## Components & changes

### New: `crates/ai/src/model_cache.rs`

Pure and unit-testable. Takes a directory path as an argument so the `ai` crate stays decoupled from `alterm_config`.

```rust
pub const MODEL_CACHE_TTL_SECS: u64 = 24 * 60 * 60;

#[derive(Serialize, Deserialize, Default, Clone)]
pub struct CachedProvider {
    pub models: Vec<String>,
    pub fetched_at: u64,        // unix seconds
}

#[derive(Serialize, Deserialize, Default, Clone)]
pub struct ModelCache {
    pub providers: HashMap<String, CachedProvider>,
}

pub struct CacheLookup {
    pub models: Vec<String>,
    pub needs_refresh: bool,    // missing || stale
}

impl ModelCache {
    pub fn lookup(&self, provider: &str, now: u64, ttl: u64) -> CacheLookup;
    pub fn put(&mut self, provider: &str, models: Vec<String>, now: u64);
}

/// Loads `<dir>/model-cache.json`. Missing or corrupt file → empty cache (never panics).
pub fn load(dir: &Path) -> ModelCache;
/// Best-effort write of `<dir>/model-cache.json`; logs on failure, never panics.
pub fn save(dir: &Path, cache: &ModelCache);
```

- File location: `~/.config/alterm/model-cache.json` (caller passes `AppConfig::config_dir()`).
- `now` is injected (`u64` unix seconds) so TTL logic is deterministic in tests; production caller uses `SystemTime::now()`.

### Changed: `crates/ai/src/lib.rs`

- `fetch_models` returns a typed result instead of swallowing errors:

```rust
pub enum ModelFetchError {
    MissingApiKey,   // provider requires a key and none is set (no network call made)
    Unauthorized,    // 401 / 403
    Unreachable,     // connection / network error
    BadResponse,     // unexpected JSON shape / parse failure
}
impl ModelFetchError { pub fn user_message(&self) -> String; }

pub async fn fetch_models(
    base_url: &str,
    api_key: Option<&str>,
    provider_type: &str,
) -> Result<Vec<String>, ModelFetchError>;
```

- `pub fn provider_requires_key(provider: &str) -> bool` — `openai`/`anthropic`/`google`/`xai` → true; `ollama`/`lmstudio` → false. Used to short-circuit to `MissingApiKey` without a network call.
- Extract pure helpers `parse_openai_models(&serde_json::Value) -> Vec<String>` and `parse_gemini_models(&serde_json::Value) -> Vec<String>` so response parsing is unit-testable.
- `user_message()` copy: `MissingApiKey`/`Unauthorized` mention adding an API key in Settings; `Unreachable` mentions the provider being offline/unreachable; `BadResponse` is a generic "unexpected response."

### Changed: `crates/workspace/src/ai_chat.rs` (`AIChatState`)

Add two fields (keep `available_models`, `models_loading`):

```rust
pub models_error: Option<String>,   // human-readable reason the last fetch failed
pub custom_model_entry: bool,       // user chose to type a custom model
```

Initialize to `None` / `false` in `new()`.

### Changed: `alterm/src/main.rs`

- App struct gains:
  - `model_cache: ai::model_cache::ModelCache` — loaded once at startup from `config_dir()`.
  - `models_inflight: HashSet<String>` — providers with a fetch in flight, to dedupe concurrent fetches.
- Message changes:
  - `AIFetchModels(pane_grid::Pane, bool)` — the `bool` is `force` (skip the staleness check; used by Retry and provider-change).
  - `AIModelsFetched(pane_grid::Pane, String, Result<Vec<String>, ai::ModelFetchError>)` — carries the provider name as a race guard.
  - New `AIToggleCustomModel(pane_grid::Pane)`.
- An `ensure_models(pane, force)` helper centralizes the cache-first logic: look up cache → set `available_models` + `models_loading` → spawn fetch if `force || needs_refresh` and provider not already in flight.

## Triggers (the bug fixes)

- **Session restore** *(the primary gap)*: after restore builds panes (`main.rs` ~1979–1990, the restored-panes hook), call `ensure_models` for each **unique** provider among restored `AIChat` panes.
- **New chat** (`main.rs:1375`) and **provider change** (`main.rs:1468`): route through `ensure_models`; provider-change passes `force = true`.
- **Retry button**: dispatches `AIFetchModels(pane, true)` and clears `models_error`.
- **Dedupe**: `ensure_models` skips spawning if the provider is already in `models_inflight`; the entry is cleared in the `AIModelsFetched` handler.

## Error / empty-state handling

`ai_chat_view` model-selector branch order:

1. `custom_model_entry` → text input + "↩ back to list" control.
2. `!available_models.is_empty()` → **dropdown** (primary); subtle refresh hint if `models_loading`.
3. `models_loading` → "Loading models…".
4. `models_error = Some(reason)` → `⚠ {reason}` + **[Retry]** + "use a custom model" link.
5. otherwise (idle/empty) → trigger a fetch.

Rules:

- **A failed background refresh never wipes a usable list.** `models_error` is only surfaced (branch 4) when `available_models` is empty. Offline-with-cache keeps showing the dropdown.
- **Race guard:** if `AIModelsFetched`'s provider != the pane's current provider (user switched mid-flight), do not touch pane UI state, but still update the cache for the returned provider.
- **Persistence:** on a successful fetch, update `model_cache` in memory and `save()` to disk (best-effort).

## Testing

**`ai::model_cache` unit tests:**
- `lookup` fresh → `needs_refresh = false`, returns models.
- `lookup` missing provider → `needs_refresh = true`, empty models.
- `lookup` stale (`now - fetched_at > ttl`) → `needs_refresh = true`, still returns stale models.
- `put`/`lookup` roundtrip.
- `load` tolerates a missing file (empty cache) and corrupt JSON (empty cache, no panic).
- `save` → `load` roundtrip via a temp dir.

**`ai` unit tests:**
- `provider_requires_key` mapping for all six providers.
- `ModelFetchError::user_message` non-empty; `MissingApiKey`/`Unauthorized` mention the API key.
- `parse_openai_models` / `parse_gemini_models` against representative sample JSON (including the Gemini `models/` prefix strip).

**State test:**
- Extract a pure `model_selector_mode(&AIChatState) -> SelectorMode` and test the five-way branch selection without rendering.

**Manual verification (AI panes are not webviews, so synthetic-click testing works):**
- New chat with a configured provider → dropdown populates; `model-cache.json` written.
- Restart app → restored chat shows cached models **instantly** (the core fix).
- No key / provider down → `⚠ reason` + Retry + "use a custom model"; the link reveals the text field; Retry re-fetches.
- Switch provider → list refreshes.

## Out of scope (→ Improvement B, next spec)

- Anthropic live `/v1/models` fetch (stays hardcoded, but now cached like the rest).
- Filtering OpenAI-compatible results to chat-capable models (embeddings/tts/whisper/etc. still appear).
- Persisting the user's selected model back to config.

## File change summary

| File | Change |
|------|--------|
| `crates/ai/src/model_cache.rs` | **new** — persisted per-provider cache, TTL, lookup |
| `crates/ai/src/lib.rs` | `fetch_models` → `Result`; `ModelFetchError`; `provider_requires_key`; pure parse helpers; `pub mod model_cache` |
| `crates/workspace/src/ai_chat.rs` | `AIChatState`: add `models_error`, `custom_model_entry` |
| `alterm/src/main.rs` | App fields (`model_cache`, `models_inflight`); `ensure_models`; message changes; restore trigger; view branches; Retry/custom controls |
