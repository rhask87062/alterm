# AI Model-List Quality Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make AI model dropdowns correct and clean — live Anthropic models, filtered chat-only lists, and a persisted model selection.

**Architecture:** Builds on the shipped reliability work (cache + provider-keyed fetch are unchanged; filtered/live lists flow through them). Adds a pure chat-model filter applied to every provider's fetched list, a live Anthropic `/v1/models` fetch with hardcoded fallback, and persistence of the selected model to `config.toml`.

**Tech Stack:** Rust (edition 2021), reqwest, serde_json, iced 0.14.

## Global Constraints

- Chat-model filter = substring **denylist** `NON_CHAT_MARKERS`, applied (after parsing) to EVERY list `fetch_models` returns: OpenAI-compatible, Gemini, and Anthropic (live + hardcoded fallback). It is a denylist, not an allowlist — unknown/new chat models survive.
- The denylist is conservative: it does NOT include `audio`, `vision`, or `realtime` (those are chat-capable multimodal models).
- Anthropic: live `GET {base_url}/models` with headers `x-api-key: <key>` and `anthropic-version: 2023-06-01`; on ANY failure or empty result, fall back to `anthropic_hardcoded_models()`. The Anthropic path always yields a list (never a `ModelFetchError`).
- `anthropic_hardcoded_models()` model-id strings stay VERBATIM (the fallback set; making them live is done via the endpoint, not by editing the list).
- Selected model is persisted to `config.toml` on every `AIModelChanged`; the provider entry is created (with its canonical base URL) if absent; a save failure is logged, not fatal.
- Do NOT change the cache file format, the fetch/trigger plumbing, or the `Result<Vec<String>, ModelFetchError>` contract of `fetch_models` for openai/gemini.

---

### Task 1: Chat-model filter (pure) + apply to OpenAI/Gemini paths

**Files:**
- Modify: `crates/ai/src/lib.rs` (add `NON_CHAT_MARKERS`, `is_chat_model`, `filter_chat_models`; wrap the `Ok(...)` returns in `fetch_openai_compatible_models` and `fetch_gemini_models`)
- Test: inline `#[cfg(test)]` in `crates/ai/src/lib.rs`

**Interfaces:**
- Produces:
  - `pub fn is_chat_model(id: &str) -> bool`
  - `pub fn filter_chat_models(models: Vec<String>) -> Vec<String>`

- [ ] **Step 1: Add the filter functions and their tests**

In `crates/ai/src/lib.rs`, add immediately after the `provider_requires_key` function:

```rust
/// Substrings (lowercased compare) that mark a model id as NOT a text-chat
/// model — embeddings, speech, image/audio generation, moderation, etc.
/// Deliberately conservative: does NOT include `audio`/`vision`/`realtime`,
/// which are chat-capable multimodal models.
const NON_CHAT_MARKERS: &[&str] = &[
    "embed", "tts", "whisper", "dall-e", "dalle", "moderation",
    "rerank", "clip", "stable-diffusion", "sora", "image-", "-image",
];

/// True if `id` looks like a text-chat model (not embeddings/speech/image/etc.).
pub fn is_chat_model(id: &str) -> bool {
    let lower = id.to_lowercase();
    !NON_CHAT_MARKERS.iter().any(|m| lower.contains(m))
}

/// Drop non-chat models, preserving order.
pub fn filter_chat_models(models: Vec<String>) -> Vec<String> {
    models.into_iter().filter(|m| is_chat_model(m)).collect()
}
```

Then add these tests inside the existing `#[cfg(test)] mod model_listing_tests` block in the same file:

```rust
    #[test]
    fn keeps_chat_models() {
        for id in [
            "gpt-4o", "o1", "o3-mini", "claude-sonnet-4-5-20250929",
            "llama3.2", "gemini-2.0-flash", "grok-2", "mixtral-8x7b",
            "gpt-4o-audio-preview", "gpt-4-vision-preview", "gpt-4o-realtime-preview",
        ] {
            assert!(is_chat_model(id), "{id} should be kept");
        }
    }

    #[test]
    fn drops_non_chat_models() {
        for id in [
            "text-embedding-3-small", "text-embedding-ada-002", "nomic-embed-text",
            "whisper-1", "tts-1", "tts-1-hd", "dall-e-3", "gpt-image-1",
            "text-moderation-latest", "rerank-english-v3.0", "clip-vit-base",
            "stable-diffusion-xl", "sora",
        ] {
            assert!(!is_chat_model(id), "{id} should be dropped");
        }
    }

    #[test]
    fn filter_preserves_order_of_kept() {
        let input = vec![
            "gpt-4o".to_string(),
            "text-embedding-3-small".to_string(),
            "o1".to_string(),
            "whisper-1".to_string(),
        ];
        assert_eq!(filter_chat_models(input), vec!["gpt-4o".to_string(), "o1".to_string()]);
    }
```

- [ ] **Step 2: Run tests to verify they fail/compile-fail, then pass**

Run: `cargo test -p ai model_listing`
Expected first run: fails to compile (functions not yet defined) or the new tests are absent. After Step 1: PASS.

- [ ] **Step 3: Apply the filter to the OpenAI-compatible and Gemini fetch paths**

In `crates/ai/src/lib.rs`, change the final line of `fetch_openai_compatible_models` from:

```rust
    Ok(parse_openai_models(&body))
```
to:
```rust
    Ok(filter_chat_models(parse_openai_models(&body)))
```

And change the final line of `fetch_gemini_models` from:

```rust
    Ok(parse_gemini_models(&body))
```
to:
```rust
    Ok(filter_chat_models(parse_gemini_models(&body)))
```

- [ ] **Step 4: Build + run the crate tests**

Run: `cargo test -p ai`
Expected: PASS (all prior `ai` tests + the 3 new filter tests).

- [ ] **Step 5: Commit**

```bash
git add crates/ai/src/lib.rs
git commit -m "feat(ai): filter non-chat models from fetched lists"
```

---

### Task 2: Live Anthropic model fetch with hardcoded fallback

**Files:**
- Modify: `crates/ai/src/lib.rs` (add `fetch_anthropic_models` + `try_fetch_anthropic`; route `"anthropic"` in `fetch_models`)
- Test: inline `#[cfg(test)]` in `crates/ai/src/lib.rs`

**Interfaces:**
- Consumes (from Task 1): `filter_chat_models`. Also uses existing `parse_openai_models`, `anthropic_hardcoded_models`.
- Produces: `fetch_models("anthropic", …)` now returns a live (filtered) list when reachable, else the (filtered) hardcoded set.

- [ ] **Step 1: Add the Anthropic fetch functions**

In `crates/ai/src/lib.rs`, add after `anthropic_hardcoded_models`:

```rust
/// Fetch Anthropic's model list live. Anthropic's `GET /v1/models` returns an
/// OpenAI-shaped `{ "data": [ { "id": ... } ] }` body, so `parse_openai_models`
/// is reused. Returns `None` on any network/HTTP/parse failure so the caller
/// can fall back to the hardcoded set.
async fn try_fetch_anthropic(base_url: &str, api_key: Option<&str>) -> Option<Vec<String>> {
    let url = format!("{}/models", base_url);
    let client = reqwest::Client::new();
    let mut request = client.get(&url).header("anthropic-version", "2023-06-01");
    if let Some(key) = api_key {
        request = request.header("x-api-key", key);
    }
    let resp = request.send().await.ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let body: serde_json::Value = resp.json().await.ok()?;
    Some(filter_chat_models(parse_openai_models(&body)))
}

/// Anthropic models: live list when reachable and non-empty, else the
/// hardcoded fallback. Always yields a (filtered) list — never an error.
async fn fetch_anthropic_models(base_url: &str, api_key: Option<&str>) -> Vec<String> {
    match try_fetch_anthropic(base_url, api_key).await {
        Some(models) if !models.is_empty() => models,
        _ => filter_chat_models(anthropic_hardcoded_models()),
    }
}
```

- [ ] **Step 2: Route the `"anthropic"` arm in `fetch_models`**

In `fetch_models`, change:

```rust
        "anthropic" => Ok(anthropic_hardcoded_models()),
```
to:
```rust
        "anthropic" => Ok(fetch_anthropic_models(base_url, api_key).await),
```

Also update the `fetch_models` doc comment line `/// - **Anthropic**: no listing endpoint — returns a hardcoded set.` to:
```rust
/// - **Anthropic**: `GET {base_url}/models` (x-api-key + anthropic-version), hardcoded fallback.
```

- [ ] **Step 3: Add a fallback-cleanliness test**

In the `#[cfg(test)] mod model_listing_tests` block, add:

```rust
    #[test]
    fn anthropic_fallback_is_nonempty_and_all_chat() {
        let fallback = filter_chat_models(anthropic_hardcoded_models());
        assert!(!fallback.is_empty());
        // None of the hardcoded ids should be filtered out as non-chat.
        assert_eq!(fallback.len(), anthropic_hardcoded_models().len());
    }
```

- [ ] **Step 4: Build + test**

Run: `cargo test -p ai`
Expected: PASS. Also run `cargo build -p ai` — clean.

(The live network path isn't unit-tested; it's exercised in Task 5 manual verification. The fallback and filtering are covered above.)

- [ ] **Step 5: Commit**

```bash
git add crates/ai/src/lib.rs
git commit -m "feat(ai): live Anthropic model fetch with hardcoded fallback"
```

---

### Task 3: Persist selected model to config

**Files:**
- Modify: `crates/config/src/lib.rs` (add `AIConfig::set_provider_model`)
- Test: inline `#[cfg(test)]` in `crates/config/src/lib.rs`

**Interfaces:**
- Consumes (existing): `ProviderEntry::for_provider(name, model)`, the `AIProviders` fields (`openai`/`anthropic`/`google`/`xai`/`lmstudio`/`ollama: Option<ProviderEntry>`).
- Produces: `AIConfig::set_provider_model(&mut self, provider: &str, model: &str)`.

- [ ] **Step 1: Add the method and tests**

In `crates/config/src/lib.rs`, add to the `impl AIConfig { … }` block (next to `provider_model`):

```rust
    /// Set the model for a provider, creating the provider entry (with its
    /// canonical base URL) if it doesn't exist yet. Unknown provider names are
    /// ignored. Call `save` afterward to persist.
    pub fn set_provider_model(&mut self, provider: &str, model: &str) {
        let slot: Option<&mut Option<ProviderEntry>> = match provider {
            "openai" => Some(&mut self.providers.openai),
            "anthropic" => Some(&mut self.providers.anthropic),
            "google" => Some(&mut self.providers.google),
            "xai" => Some(&mut self.providers.xai),
            "lmstudio" => Some(&mut self.providers.lmstudio),
            "ollama" => Some(&mut self.providers.ollama),
            _ => None,
        };
        if let Some(entry_slot) = slot {
            match entry_slot {
                Some(entry) => entry.model = model.to_string(),
                None => *entry_slot = Some(ProviderEntry::for_provider(provider, model)),
            }
        }
    }
```

Then add a test module at the end of `crates/config/src/lib.rs` (or extend an existing `#[cfg(test)]` block if present):

```rust
#[cfg(test)]
mod set_model_tests {
    use super::*;

    #[test]
    fn updates_existing_entry_keeps_base_url() {
        let mut cfg = AIConfig::default();
        cfg.providers.openai = Some(ProviderEntry {
            api_key: Some("k".into()),
            base_url: Some("http://custom/v1".into()),
            model: "old".into(),
        });
        cfg.set_provider_model("openai", "gpt-4o");
        let e = cfg.providers.openai.as_ref().unwrap();
        assert_eq!(e.model, "gpt-4o");
        assert_eq!(e.base_url.as_deref(), Some("http://custom/v1")); // unchanged
        assert_eq!(e.api_key.as_deref(), Some("k")); // unchanged
    }

    #[test]
    fn creates_entry_when_missing_with_canonical_base_url() {
        let mut cfg = AIConfig::default();
        cfg.providers.openai = None;
        cfg.set_provider_model("openai", "gpt-4o");
        let e = cfg.providers.openai.as_ref().unwrap();
        assert_eq!(e.model, "gpt-4o");
        assert_eq!(e.base_url.as_deref(), default_base_url("openai"));
    }

    #[test]
    fn unknown_provider_is_noop() {
        let mut cfg = AIConfig::default();
        let before = cfg.providers.openai.clone();
        cfg.set_provider_model("bogus", "x"); // must not panic
        assert_eq!(cfg.providers.openai.is_some(), before.is_some());
    }
}
```

> Note: `ProviderEntry` already derives `Clone` (used by the test). `default_base_url` is a free function in this module.

- [ ] **Step 2: Run tests to verify they fail/compile-fail, then pass**

Run: `cargo test -p alterm-config set_model`
Expected first run: compile-fail (method missing). After Step 1: PASS (3 tests).

- [ ] **Step 3: Run the crate suite**

Run: `cargo test -p alterm-config`
Expected: PASS (existing 14 + 3 new).

- [ ] **Step 4: Commit**

```bash
git add crates/config/src/lib.rs
git commit -m "feat(config): AIConfig::set_provider_model (create-or-update)"
```

---

### Task 4: Persist model selection from the UI

**Files:**
- Modify: `alterm/src/main.rs` — the `Message::AIModelChanged(pane, model)` handler (currently at `alterm/src/main.rs:1543-1548`)

**Interfaces:**
- Consumes (from Task 3): `self.config.ai.set_provider_model(&provider, &model)`. Existing: `AppConfig::config_path()`, `self.config.save(path) -> Result<…>`.

- [ ] **Step 1: Update the handler to persist**

In `alterm/src/main.rs`, replace the `Message::AIModelChanged` handler:

```rust
            Message::AIModelChanged(pane, model) => {
                let tab = self.active_tab_mut();
                if let Some(Block::AIChat { state }) = tab.panes.get_mut(pane) {
                    state.model_name = model;
                }
            }
```

with:

```rust
            Message::AIModelChanged(pane, model) => {
                // Update in-memory pane state and capture the provider so we can
                // persist the choice to config.
                let provider = {
                    let tab = self.active_tab_mut();
                    if let Some(Block::AIChat { state }) = tab.panes.get_mut(pane) {
                        state.model_name = model.clone();
                        state.provider_name.clone()
                    } else {
                        return Task::none();
                    }
                };
                self.config.ai.set_provider_model(&provider, &model);
                if let Err(e) = self.config.save(&AppConfig::config_path()) {
                    log::error!("Failed to persist model selection: {e}");
                }
            }
```

- [ ] **Step 2: Build**

Run: `cargo build -p alterm`
Expected: clean (pre-existing warnings OK).

- [ ] **Step 3: Run the full workspace suite**

Run: `cargo test`
Expected: PASS across all crates.

- [ ] **Step 4: Commit**

```bash
git add alterm/src/main.rs
git commit -m "feat(alterm): persist selected AI model to config"
```

---

### Task 5: Manual verification (controller-run)

**Files:** none (runtime verification).

> AI panes are ordinary iced widgets, so synthetic clicks/screenshots work. Use an isolated config dir (`XDG_CONFIG_HOME=/tmp/...`) so the user's real config/session are never touched. Never kill the user's instances — only debug instances launched here, by explicit PID.

- [ ] **Step 1: Build debug + start a mock `/models` server that returns mixed models**

```bash
cargo build -p alterm
mkdir -p /tmp/altq/alterm
cat > /tmp/altq/mock.py <<'PY'
from http.server import BaseHTTPRequestHandler, HTTPServer
import json
class H(BaseHTTPRequestHandler):
    def do_GET(self):
        body=json.dumps({"data":[
          {"id":"gpt-4o"},{"id":"o1"},
          {"id":"text-embedding-3-small"},{"id":"whisper-1"},
          {"id":"dall-e-3"},{"id":"tts-1"}]}).encode()
        self.send_response(200); self.send_header("Content-Type","application/json")
        self.send_header("Content-Length",str(len(body))); self.end_headers(); self.wfile.write(body)
    def log_message(self,*a): pass
HTTPServer(("127.0.0.1",18090),H).serve_forever()
PY
( cd /tmp/altq && nohup python3 mock.py >/tmp/altq/mock.log 2>&1 & disown )
cat > /tmp/altq/alterm/config.toml <<'TOML'
[ai]
default_provider = "openai"
[ai.providers.openai]
api_key = "test"
base_url = "http://localhost:18090/v1"
model = "gpt-4o"
[session]
restore = true
TOML
```

- [ ] **Step 2: Launch isolated, open an AI pane, verify the dropdown is filtered**

Launch: `XDG_CONFIG_HOME=/tmp/altq RUST_LOG=ai=debug nohup ./target/debug/alterm >/tmp/altq/app.log 2>&1 & disown`
Open an AI pane (click the sidebar "AI" button), then open the model dropdown.
Expected: dropdown shows only `gpt-4o` and `o1` — NOT `text-embedding-3-small`, `whisper-1`, `dall-e-3`, `tts-1`.

- [ ] **Step 3: Verify selection is persisted**

Select `o1` in the dropdown, then:
```bash
grep -A3 'providers.openai' /tmp/altq/alterm/config.toml
```
Expected: `model = "o1"` written to the isolated `config.toml`.

- [ ] **Step 4: (Optional) Anthropic fallback sanity**

With no Anthropic key configured, switching the provider picker to `anthropic` shows the hardcoded fallback list (key-less cloud providers surface "No API key" instead — that's expected; the live path needs a real key). The fallback list rendering is covered by the unit test; this step is a visual sanity check only.

- [ ] **Step 5: Clean up (explicit PID only)**

```bash
# kill only the debug instance + mock; never the user's ~/.cargo/bin/alterm
for pid in $(ps -eo pid,args | awk '/debug\/alterm/ && !/awk/ {print $1}'); do kill "$pid"; done
for pid in $(ps -eo pid,args | awk '/altq\/mock/ && !/awk/ {print $1}'); do kill "$pid"; done
rm -rf /tmp/altq
```

---

## Self-Review

**1. Spec coverage:**
- Anthropic live fetch + fallback → Task 2. ✓
- Chat-model filter (denylist, all providers) → Task 1 (openai/gemini) + Task 2 (anthropic live + fallback). ✓
- Persist selected model → Task 3 (`set_provider_model`) + Task 4 (handler). ✓
- Conservative denylist (no audio/vision/realtime) → Task 1 `NON_CHAT_MARKERS` + `keeps_chat_models` test asserts `gpt-4o-audio-preview`/`gpt-4-vision-preview`/`gpt-4o-realtime-preview` survive. ✓
- Anthropic hardcoded strings verbatim → Task 2 doesn't edit them; only adds the live path + routes to it. ✓
- No change to cache format / fetch_models Result contract for openai/gemini → only the `Ok(...)` payload is filtered; signatures unchanged. ✓
- Tests for filter and set_provider_model → Tasks 1 and 3. ✓

**2. Placeholder scan:** No TBD/TODO; every code step has complete code and exact commands. ✓

**3. Type consistency:** `is_chat_model(&str)->bool` and `filter_chat_models(Vec<String>)->Vec<String>` are defined in Task 1 and used identically in Task 2. `set_provider_model(&mut self, &str, &str)` defined in Task 3 and called identically in Task 4. `fetch_anthropic_models(base_url, api_key).await -> Vec<String>` wrapped in `Ok(...)` matches `fetch_models`'s `Result<Vec<String>, ModelFetchError>`. ✓
