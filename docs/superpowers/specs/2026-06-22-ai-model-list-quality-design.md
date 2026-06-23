# AI Model-List Quality — Design (Improvement B)

- **Date:** 2026-06-22
- **Status:** Approved (design)
- **Scope:** Make the model lists *correct and clean*. Builds on the shipped "Improvement A" reliability work (`alterm-ai-model-list-reliability`); the cache and fetch plumbing are unchanged — filtered/live results just flow through them.

## Problem

Even when the model dropdown populates, the list is wrong or noisy:
1. **Anthropic is hardcoded** (`anthropic_hardcoded_models()` in `crates/ai/src/lib.rs`) — it can never be up to date, and some ids are already stale.
2. **OpenAI-compatible (and Gemini) results are unfiltered** — embeddings, tts, whisper, dall-e, moderation, rerank, etc. are dumped into the chat-model dropdown.
3. **The selected model isn't persisted** — `AIModelChanged` only updates in-memory state, so new chats and restarts revert to the config default.

## Decisions (locked during brainstorming)

1. **Anthropic live fetch with fallback.** Fetch `GET {base_url}/models` (Anthropic headers); on any error or empty result, fall back to the existing hardcoded set, so Anthropic is never worse than today.
2. **Chat-model filter = substring denylist, applied to every provider's fetched list** (including local ollama/lmstudio, so local embedding models are filtered too). New/unknown chat models survive (not an allowlist). Conservative list — does NOT filter `audio`/`vision`/`realtime` (those are chat-capable multimodal).
3. **Persist the selected model** to `config.toml` on every selection.

## Components & changes

### `crates/ai/src/lib.rs`

**Anthropic live fetch.** `fetch_models` routes `"anthropic"` to a new `fetch_anthropic_models(base_url, api_key)` instead of returning the hardcoded list directly:

```rust
async fn fetch_anthropic_models(base_url: &str, api_key: Option<&str>) -> Vec<String> {
    // GET {base_url}/models with x-api-key + anthropic-version: 2023-06-01.
    // Anthropic returns an OpenAI-shaped { "data": [ { "id": ... } ] } body,
    // so parse_openai_models is reused. On ANY failure (network, non-2xx,
    // parse) or an empty list, return anthropic_hardcoded_models().
}
```

- Signature stays `Vec<String>` (not `Result`): Anthropic always yields *something* (live list or the hardcoded fallback), so it can't surface a `ModelFetchError`. The `MissingApiKey` short-circuit in `fetch_models` still fires before this is reached. `anthropic_hardcoded_models()` is retained as the fallback.
- The `anthropic-version` header value `"2023-06-01"` matches `crates/ai/src/anthropic.rs`.

**Chat-model filter (pure, testable).**

```rust
/// Substrings that mark a model as NOT a text-chat model. Lowercased compare.
const NON_CHAT_MARKERS: &[&str] = &[
    "embed", "tts", "whisper", "dall-e", "dalle", "moderation",
    "rerank", "clip", "stable-diffusion", "sora", "image-", "-image",
];

/// True if `id` looks like a text-chat model (not embeddings/audio-gen/image/etc.).
pub fn is_chat_model(id: &str) -> bool {
    let lower = id.to_lowercase();
    !NON_CHAT_MARKERS.iter().any(|m| lower.contains(m))
}

/// Drop non-chat models, preserving order.
pub fn filter_chat_models(models: Vec<String>) -> Vec<String> {
    models.into_iter().filter(|m| is_chat_model(m)).collect()
}
```

- Applied to the fetched list in each fetch path: `fetch_openai_compatible_models`, `fetch_gemini_models`, and `fetch_anthropic_models` (and the hardcoded fallback) — i.e. every list `fetch_models` can return is filtered. Parsing stays pure and unfiltered; filtering is a separate, composable step applied right after parse.
- `image-` / `-image` (not bare `image`) targets ids like `gpt-image-1` while leaving hypothetical chat names that merely contain the word elsewhere unaffected; bare markers like `embed`/`tts`/`whisper` are specific enough.

### `crates/config/src/lib.rs`

```rust
impl AIConfig {
    /// Set (and persist on save) the model for a provider, creating the
    /// provider entry with its canonical base URL if it doesn't exist yet.
    pub fn set_provider_model(&mut self, provider: &str, model: &str);
}
```

- Updates `self.providers.<provider>.model`. If the entry is `None`, create it via `ProviderEntry::for_provider(provider, model)`; if `Some`, set `.model`. Unknown provider names are ignored (no panic).
- Needs a mutable accessor; add `AIProviders::get_mut(&mut self, name) -> Option<&mut ProviderEntry>` mirroring the existing `get`, OR match on the field directly inside `set_provider_model`. Implementation may use either; behavior is what matters.

### `alterm/src/main.rs`

`Message::AIModelChanged(pane, model)` handler — after setting `state.model_name`:

```rust
            Message::AIModelChanged(pane, model) => {
                let provider = {
                    let tab = self.active_tab_mut();
                    match tab.panes.get_mut(pane) {
                        Some(Block::AIChat { state }) => {
                            state.model_name = model.clone();
                            state.provider_name.clone()
                        }
                        _ => return Task::none(),
                    }
                };
                self.config.ai.set_provider_model(&provider, &model);
                if let Err(e) = self.config.save(&AppConfig::config_path()) {
                    log::error!("Failed to persist model selection: {e}");
                }
            }
```

(Exact borrow structure may differ; the requirement is: update in-memory state, persist to config, log on save failure.)

## Data flow

`fetch_models(provider)` → provider-specific fetch → `parse_*` → **`filter_chat_models`** → `Result<Vec<String>>` → (existing) cache + UI. Anthropic: live fetch → parse → filter, else hardcoded → filter. Selection: `AIModelChanged` → in-memory + `set_provider_model` → `config.save`.

## Error handling

- Anthropic live fetch failure/empty → hardcoded fallback (already filtered). Never an error to the UI.
- Filtering an empty list yields an empty list (handled by the existing empty/error UI from Improvement A).
- `config.save` failure on model-select → logged, non-fatal (the in-memory selection still applies for the session).

## Testing

**`ai` unit tests:**
- `is_chat_model` / `filter_chat_models`: keeps `gpt-4o`, `o1`, `claude-sonnet-4-...`, `llama3.2`, `gemini-2.0-flash`; drops `text-embedding-3-small`, `whisper-1`, `tts-1`, `dall-e-3`, `text-moderation-latest`, `nomic-embed-text`, `gpt-image-1`. Order preserved.
- (Anthropic parsing already covered by `parse_openai_models` tests — same shape.)

**`config` unit tests:**
- `set_provider_model` updates an existing entry's model.
- `set_provider_model` creates an entry (with canonical base_url) when previously `None`.
- Unknown provider name is a no-op (no panic).

**Manual (controller, isolated config + mock):** mock `/models` returns chat + non-chat ids → dropdown shows only chat models; select a model → `config.toml` shows the persisted `model`.

## Non-goals

- No new providers; no model metadata/grouping/descriptions; no change to the cache file format or fetch/trigger plumbing (Improvement A). Filtered lists are cached the same way unfiltered ones were.

## File change summary

| File | Change |
|------|--------|
| `crates/ai/src/lib.rs` | `fetch_anthropic_models` (live + fallback); `is_chat_model`/`filter_chat_models` + `NON_CHAT_MARKERS`; apply filter in all fetch paths; route `"anthropic"` to live fetch |
| `crates/config/src/lib.rs` | `AIConfig::set_provider_model` (+ `AIProviders::get_mut` if used) |
| `alterm/src/main.rs` | `AIModelChanged` persists selection to config |
