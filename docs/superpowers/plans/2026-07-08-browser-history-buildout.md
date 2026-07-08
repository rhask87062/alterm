# Browser Build-Out Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Global persistent browser history with an in-webview `alterm://history` archive page, bookmarks, smart URL bar, page titles, loading states, find-in-page, zoom, and browser-standard keyboard shortcuts.

**Architecture:** All pure logic (stores, URL classification, HTML generation) lives in `crates/browser` behind unit tests. Webview event plumbing follows the existing thread-local-queue-drained-on-Tick pattern in `webview_manager.rs`. `alterm/src/main.rs` wires events to `Message`s and renders chrome UI. Internal pages are served by a wry custom protocol so history/bookmarks UI lives *inside* the webview (iced cannot draw over the native X11 webview).

**Tech Stack:** Rust, iced, wry 0.55 (webkit2gtk on Linux), serde/serde_json, chrono (day grouping), tempfile (tests only).

## Global Constraints

- wry is pinned at `0.55` with `features = ["devtools"]` — do not bump.
- Never draw iced widgets over the webview content area; chrome UI may only add rows above the webview (shrinking it via `set_bounds`).
- Webviews and iced share the main thread; all new state uses `thread_local!` queues drained on `Message::Tick` (same pattern as `NAV_EVENTS`).
- Data files: `~/.local/share/alterm/history.jsonl` (append-only JSONL) and `~/.local/share/alterm/bookmarks.json`.
- History compaction: load-time, threshold 100,000 records → keep newest 50,000 entries.
- All titles/URLs inserted into internal-page HTML MUST be HTML-escaped.
- Search-engine classification applies ONLY to URL-bar input, never to webview navigation events.
- Default search engine template: `https://duckduckgo.com/?q={}`; config key `browser.search_engine`.
- `SESSION_VERSION` stays `1`; new session fields use `#[serde(default)]` so old files load.
- Platform gating: new webview features are Linux-first; the non-Linux paths must still compile (no-op stubs, matching the existing `webview_manager` cfg pattern).
- Run tests with `cargo test -p <crate>`; full check `cargo test --workspace && cargo clippy --workspace`.
- Commit after every green task; conventional-commit style messages, e.g. `feat(browser): ...` (see git log for style).

---

### Task 1: `BrowserConfig` in the config crate

**Files:**
- Modify: `crates/config/src/lib.rs` (AppConfig struct ~line 12, Default impl ~line 21; add new section after `SessionConfig` ~line 320)

**Interfaces:**
- Produces: `alterm_config::AppConfig.browser: BrowserConfig` with field `search_engine: String` (template containing `{}`), default `"https://duckduckgo.com/?q={}"`.

- [ ] **Step 1: Write the failing test**

Add to the `tests` module at the bottom of `crates/config/src/lib.rs`:

```rust
#[test]
fn browser_config_defaults_and_roundtrips() {
    let config = AppConfig::default();
    assert_eq!(config.browser.search_engine, "https://duckduckgo.com/?q={}");
    // Old config files without a [browser] section must still parse.
    let parsed: AppConfig = toml::from_str("").unwrap();
    assert_eq!(parsed.browser.search_engine, "https://duckduckgo.com/?q={}");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p alterm-config browser_config_defaults_and_roundtrips`
(If the package name differs, check `crates/config/Cargo.toml` `[package] name` and use that.)
Expected: FAIL — `no field `browser` on type `AppConfig``

- [ ] **Step 3: Implement**

In `crates/config/src/lib.rs`, add to `AppConfig`:

```rust
    #[serde(default)]
    pub browser: BrowserConfig,
```

and to `impl Default for AppConfig`: `browser: BrowserConfig::default(),`

Add a new section (after `SessionConfig`):

```rust
// ── BrowserConfig ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct BrowserConfig {
    /// Search engine URL template. `{}` is replaced with the
    /// percent-encoded query typed in the URL bar.
    pub search_engine: String,
}

impl Default for BrowserConfig {
    fn default() -> Self {
        Self { search_engine: "https://duckduckgo.com/?q={}".to_string() }
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p alterm-config`
Expected: PASS (including the existing `default_config_roundtrips`)

- [ ] **Step 5: Commit**

```bash
git add crates/config/src/lib.rs
git commit -m "feat(config): add [browser] section with search_engine template"
```

---

### Task 2: History & bookmark stores

**Files:**
- Create: `crates/browser/src/history.rs`
- Modify: `crates/browser/src/lib.rs` (add `pub mod history;` at the top, unconditionally — the module is pure std + serde, no platform gating)
- Modify: `crates/browser/Cargo.toml`

**Interfaces:**
- Produces (all in `browser::history`):
  - `struct HistoryEntry { pub url: String, pub title: String, pub timestamp: u64 }`
  - `struct HistoryStore` — `load(path: PathBuf) -> Self`, `record_visit(&mut self, url: &str, timestamp: u64)`, `set_title(&mut self, url: &str, title: &str)`, `query(&self, filter: &str) -> Vec<HistoryEntry>` (newest-first), `delete(&mut self, url: &str, timestamp: u64)`, `clear(&mut self)`, `len(&self) -> usize`, `is_empty(&self) -> bool`
  - `struct Bookmark { pub url: String, pub title: String }`
  - `struct BookmarkStore` — `load(path: PathBuf) -> Self`, `toggle(&mut self, url: &str, title: &str) -> bool` (returns new bookmarked state), `is_bookmarked(&self, url: &str) -> bool`, `list(&self) -> &[Bookmark]`
  - `struct Stores { pub history: HistoryStore, pub bookmarks: BookmarkStore }`
  - `fn init(data_dir: &std::path::Path)` — creates dir, loads both stores into a thread-local
  - `fn with_stores<R>(f: impl FnOnce(&mut Stores) -> R) -> Option<R>` — `None` if `init` was never called
  - `fn now_ts() -> u64` — unix seconds
  - `const COMPACT_THRESHOLD: usize = 100_000;` / `const COMPACT_KEEP: usize = 50_000;`

- [ ] **Step 1: Add dependencies**

In `crates/browser/Cargo.toml`, extend `[dependencies]` (these are unconditional — the stores are platform-independent):

```toml
[dependencies]
log.workspace = true
serde = { version = "1", features = ["derive"] }
serde_json = "1"

[dev-dependencies]
tempfile = "3"
```

(Check the root `Cargo.toml` `[workspace.dependencies]`: if `serde`/`serde_json` are declared there, use `serde.workspace = true` / `serde_json.workspace = true` instead, matching `crates/workspace/Cargo.toml`.)

- [ ] **Step 2: Write the failing tests**

Create `crates/browser/src/history.rs` with the module skeleton and tests:

```rust
//! Global browser history + bookmarks, persisted under the alterm data dir.
//!
//! `history.jsonl` is append-only: every mutation appends a `Record` line
//! (visit, title patch, delete, clear). The file is only rewritten during
//! load-time compaction. `bookmarks.json` is small and rewritten wholesale.

use std::cell::RefCell;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

// (implementation added in Step 4)

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn history_in(dir: &TempDir) -> HistoryStore {
        HistoryStore::load(dir.path().join("history.jsonl"))
    }

    #[test]
    fn record_and_reload_visits() {
        let dir = TempDir::new().unwrap();
        let mut s = history_in(&dir);
        s.record_visit("https://a.com", 100);
        s.record_visit("https://b.com", 200);
        drop(s);
        let s = history_in(&dir);
        let all = s.query("");
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].url, "https://b.com"); // newest first
        assert_eq!(all[1].url, "https://a.com");
    }

    #[test]
    fn title_patch_applies_to_most_recent_matching_entry_and_persists() {
        let dir = TempDir::new().unwrap();
        let mut s = history_in(&dir);
        s.record_visit("https://a.com", 100);
        s.record_visit("https://b.com", 200);
        s.record_visit("https://a.com", 300);
        s.set_title("https://a.com", "A!");
        let all = s.query("");
        assert_eq!(all[0].title, "A!");       // ts=300 entry patched
        assert_eq!(all[2].title, "");          // ts=100 entry untouched
        drop(s);
        let s = history_in(&dir);
        assert_eq!(s.query("")[0].title, "A!"); // patch survives reload
    }

    #[test]
    fn query_filters_case_insensitively_on_url_and_title() {
        let dir = TempDir::new().unwrap();
        let mut s = history_in(&dir);
        s.record_visit("https://docs.rs/serde", 100);
        s.record_visit("https://example.com", 200);
        s.set_title("https://example.com", "Serde Guide");
        assert_eq!(s.query("SERDE").len(), 2); // matches url of one, title of other
        assert_eq!(s.query("docs.rs").len(), 1);
        assert_eq!(s.query("zzz").len(), 0);
    }

    #[test]
    fn delete_and_clear_persist() {
        let dir = TempDir::new().unwrap();
        let mut s = history_in(&dir);
        s.record_visit("https://a.com", 100);
        s.record_visit("https://b.com", 200);
        s.delete("https://a.com", 100);
        assert_eq!(s.query("").len(), 1);
        drop(s);
        let mut s = history_in(&dir);
        assert_eq!(s.query("").len(), 1);
        s.clear();
        drop(s);
        let s = history_in(&dir);
        assert!(s.is_empty());
    }

    #[test]
    fn corrupt_lines_are_skipped() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("history.jsonl");
        std::fs::write(&path,
            "{\"type\":\"visit\",\"url\":\"https://a.com\",\"title\":\"\",\"timestamp\":1}\nnot json\n").unwrap();
        let s = HistoryStore::load(path);
        assert_eq!(s.len(), 1);
    }

    #[test]
    fn load_compacts_oversized_file() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("history.jsonl");
        {
            let mut s = HistoryStore::load(path.clone());
            for i in 0..(COMPACT_THRESHOLD as u64 + 10) {
                s.record_visit(&format!("https://x.com/{i}"), i);
            }
        }
        let s = HistoryStore::load(path.clone());
        assert_eq!(s.len(), COMPACT_KEEP);
        assert_eq!(s.query("")[0].url,
            format!("https://x.com/{}", COMPACT_THRESHOLD as u64 + 9)); // newest kept
        // File was rewritten: reloading again keeps the same count.
        let s2 = HistoryStore::load(path);
        assert_eq!(s2.len(), COMPACT_KEEP);
    }

    #[test]
    fn bookmarks_toggle_and_persist() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("bookmarks.json");
        let mut b = BookmarkStore::load(path.clone());
        assert!(b.toggle("https://a.com", "A"));      // now bookmarked
        assert!(b.is_bookmarked("https://a.com"));
        assert!(!b.toggle("https://a.com", "A"));     // now removed
        assert!(b.toggle("https://b.com", "B"));
        drop(b);
        let b = BookmarkStore::load(path);
        assert!(!b.is_bookmarked("https://a.com"));
        assert!(b.is_bookmarked("https://b.com"));
        assert_eq!(b.list().len(), 1);
    }

    #[test]
    fn with_stores_returns_none_before_init() {
        // Runs on the test thread where init() was never called.
        assert!(with_stores(|_| ()).is_none());
    }
}
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test -p browser history`
Expected: FAIL — compile errors (`HistoryStore` not defined)

- [ ] **Step 4: Implement**

Fill in `crates/browser/src/history.rs` above the tests module:

```rust
/// Compact when the on-disk record count exceeds this.
pub const COMPACT_THRESHOLD: usize = 100_000;
/// After compaction, keep this many newest entries.
pub const COMPACT_KEEP: usize = 50_000;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HistoryEntry {
    pub url: String,
    pub title: String,
    /// Unix timestamp, seconds.
    pub timestamp: u64,
}

/// One line of `history.jsonl`. Mutations are appended, never edited in
/// place; `load` replays the log. Compaction squashes to plain visits.
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum Record {
    Visit { url: String, title: String, timestamp: u64 },
    Title { url: String, title: String },
    Delete { url: String, timestamp: u64 },
    Clear,
}

pub struct HistoryStore {
    path: PathBuf,
    /// Oldest-first.
    entries: Vec<HistoryEntry>,
    /// Records seen on disk at load time (drives compaction).
    disk_records: usize,
}

impl HistoryStore {
    pub fn load(path: PathBuf) -> Self {
        let mut entries: Vec<HistoryEntry> = Vec::new();
        let mut disk_records = 0usize;
        if let Ok(text) = std::fs::read_to_string(&path) {
            for line in text.lines().filter(|l| !l.trim().is_empty()) {
                disk_records += 1;
                match serde_json::from_str::<Record>(line) {
                    Ok(Record::Visit { url, title, timestamp }) => {
                        entries.push(HistoryEntry { url, title, timestamp });
                    }
                    Ok(Record::Title { url, title }) => {
                        if let Some(e) = entries.iter_mut().rev().find(|e| e.url == url) {
                            e.title = title;
                        }
                    }
                    Ok(Record::Delete { url, timestamp }) => {
                        entries.retain(|e| !(e.url == url && e.timestamp == timestamp));
                    }
                    Ok(Record::Clear) => entries.clear(),
                    Err(e) => log::warn!("history: skipping corrupt line: {e}"),
                }
            }
        }
        let mut store = HistoryStore { path, entries, disk_records };
        if store.disk_records > COMPACT_THRESHOLD {
            store.compact();
        }
        store
    }

    /// Rewrite the file as plain visits, keeping the newest COMPACT_KEEP.
    fn compact(&mut self) {
        let excess = self.entries.len().saturating_sub(COMPACT_KEEP);
        self.entries.drain(..excess);
        let mut out = String::new();
        for e in &self.entries {
            let rec = Record::Visit {
                url: e.url.clone(), title: e.title.clone(), timestamp: e.timestamp,
            };
            if let Ok(line) = serde_json::to_string(&rec) {
                out.push_str(&line);
                out.push('\n');
            }
        }
        if let Err(e) = std::fs::write(&self.path, out) {
            log::warn!("history: compaction write failed: {e}");
        }
        self.disk_records = self.entries.len();
    }

    fn append(&mut self, rec: &Record) {
        let line = match serde_json::to_string(rec) {
            Ok(l) => l,
            Err(e) => { log::warn!("history: serialize failed: {e}"); return; }
        };
        let res = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .and_then(|mut f| writeln!(f, "{line}"));
        match res {
            Ok(()) => self.disk_records += 1,
            Err(e) => log::warn!("history: append failed (recording skipped): {e}"),
        }
    }

    pub fn record_visit(&mut self, url: &str, timestamp: u64) {
        self.append(&Record::Visit {
            url: url.to_string(), title: String::new(), timestamp,
        });
        self.entries.push(HistoryEntry {
            url: url.to_string(), title: String::new(), timestamp,
        });
    }

    /// Retro-fill the title on the most recent entry for `url`.
    pub fn set_title(&mut self, url: &str, title: &str) {
        let Some(e) = self.entries.iter_mut().rev().find(|e| e.url == url) else {
            return;
        };
        if e.title == title || title.is_empty() {
            return;
        }
        e.title = title.to_string();
        self.append(&Record::Title { url: url.to_string(), title: title.to_string() });
    }

    /// Entries matching `filter` (case-insensitive substring of URL or
    /// title), newest first. Empty filter returns everything.
    pub fn query(&self, filter: &str) -> Vec<HistoryEntry> {
        let needle = filter.to_lowercase();
        self.entries
            .iter()
            .rev()
            .filter(|e| {
                needle.is_empty()
                    || e.url.to_lowercase().contains(&needle)
                    || e.title.to_lowercase().contains(&needle)
            })
            .cloned()
            .collect()
    }

    pub fn delete(&mut self, url: &str, timestamp: u64) {
        self.entries.retain(|e| !(e.url == url && e.timestamp == timestamp));
        self.append(&Record::Delete { url: url.to_string(), timestamp });
    }

    pub fn clear(&mut self) {
        self.entries.clear();
        self.append(&Record::Clear);
    }

    pub fn len(&self) -> usize { self.entries.len() }
    pub fn is_empty(&self) -> bool { self.entries.is_empty() }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Bookmark {
    pub url: String,
    pub title: String,
}

pub struct BookmarkStore {
    path: PathBuf,
    bookmarks: Vec<Bookmark>,
}

impl BookmarkStore {
    pub fn load(path: PathBuf) -> Self {
        let bookmarks = std::fs::read_to_string(&path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        BookmarkStore { path, bookmarks }
    }

    fn save(&self) {
        match serde_json::to_string_pretty(&self.bookmarks) {
            Ok(json) => {
                if let Err(e) = std::fs::write(&self.path, json) {
                    log::warn!("bookmarks: write failed: {e}");
                }
            }
            Err(e) => log::warn!("bookmarks: serialize failed: {e}"),
        }
    }

    /// Toggle a bookmark; returns `true` if the URL is now bookmarked.
    pub fn toggle(&mut self, url: &str, title: &str) -> bool {
        if let Some(i) = self.bookmarks.iter().position(|b| b.url == url) {
            self.bookmarks.remove(i);
            self.save();
            false
        } else {
            self.bookmarks.push(Bookmark {
                url: url.to_string(), title: title.to_string(),
            });
            self.save();
            true
        }
    }

    pub fn is_bookmarked(&self, url: &str) -> bool {
        self.bookmarks.iter().any(|b| b.url == url)
    }

    pub fn list(&self) -> &[Bookmark] { &self.bookmarks }
}

pub struct Stores {
    pub history: HistoryStore,
    pub bookmarks: BookmarkStore,
}

thread_local! {
    static STORES: RefCell<Option<Stores>> = const { RefCell::new(None) };
}

/// Load the stores from `data_dir` into main-thread state. Call once at
/// startup, before any webview is created.
pub fn init(data_dir: &Path) {
    if let Err(e) = std::fs::create_dir_all(data_dir) {
        log::warn!("history: cannot create data dir {data_dir:?}: {e}");
    }
    let stores = Stores {
        history: HistoryStore::load(data_dir.join("history.jsonl")),
        bookmarks: BookmarkStore::load(data_dir.join("bookmarks.json")),
    };
    STORES.with(|s| *s.borrow_mut() = Some(stores));
}

/// Run `f` against the stores. `None` when `init` hasn't been called
/// (recording is skipped; browsing keeps working).
pub fn with_stores<R>(f: impl FnOnce(&mut Stores) -> R) -> Option<R> {
    STORES.with(|s| s.borrow_mut().as_mut().map(f))
}

/// Current unix time in seconds.
pub fn now_ts() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}
```

Add at the top of `crates/browser/src/lib.rs` (before the `webview_manager` cfg blocks):

```rust
pub mod history;
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p browser`
Expected: PASS (new history tests + all existing BrowserState tests)

Note: `with_stores_returns_none_before_init` relies on tests running on threads where `init` was never called; all other tests use explicit `HistoryStore::load` and never touch the thread-local, so there is no cross-test interference.

- [ ] **Step 6: Commit**

```bash
git add crates/browser/Cargo.toml crates/browser/src/history.rs crates/browser/src/lib.rs Cargo.lock
git commit -m "feat(browser): append-only global history store + bookmark store"
```

---

### Task 3: Smart URL-bar input resolution

**Files:**
- Modify: `crates/browser/src/lib.rs` (add functions near `normalise_url` ~line 205; tests at bottom)

**Interfaces:**
- Produces (in `browser` crate root, `pub`):
  - `fn resolve_input(input: &str, search_engine: &str) -> String` — URL-bar text → navigable URL (direct URL, `https://`-prefixed host, or search URL)
  - `fn percent_encode(s: &str) -> String`
- `normalise_url` is NOT changed (webview nav events keep using it). But it must learn to pass through `alterm://` — see Step 4.

- [ ] **Step 1: Write the failing tests**

Add to the `tests` module in `crates/browser/src/lib.rs`:

```rust
const DDG: &str = "https://duckduckgo.com/?q={}";

#[test]
fn resolve_input_passes_urls_through() {
    assert_eq!(resolve_input("https://a.com/x", DDG), "https://a.com/x");
    assert_eq!(resolve_input("http://a.com", DDG), "http://a.com");
    assert_eq!(resolve_input("about:blank", DDG), "about:blank");
    assert_eq!(resolve_input("alterm://history", DDG), "alterm://history");
}

#[test]
fn resolve_input_prefixes_bare_hosts() {
    assert_eq!(resolve_input("google.com", DDG), "https://google.com");
    assert_eq!(resolve_input("docs.rs/serde/latest", DDG), "https://docs.rs/serde/latest");
    assert_eq!(resolve_input("localhost:3000/app", DDG), "https://localhost:3000/app");
}

#[test]
fn resolve_input_searches_everything_else() {
    assert_eq!(
        resolve_input("rust lifetimes", DDG),
        "https://duckduckgo.com/?q=rust%20lifetimes"
    );
    assert_eq!(resolve_input("rust", DDG), "https://duckduckgo.com/?q=rust");
    // Spaces force a search even when a dot is present.
    assert_eq!(
        resolve_input("what is docs.rs", DDG),
        "https://duckduckgo.com/?q=what%20is%20docs.rs"
    );
    // Template without {} gets the query appended.
    assert_eq!(
        resolve_input("cats", "https://x.com/search?q="),
        "https://x.com/search?q=cats"
    );
    assert_eq!(resolve_input("", DDG), "about:blank");
}

#[test]
fn percent_encode_escapes_reserved_bytes() {
    assert_eq!(percent_encode("a b&c=d?e#f"), "a%20b%26c%3Dd%3Fe%23f");
    assert_eq!(percent_encode("safe-._~AZaz09"), "safe-._~AZaz09");
}

#[test]
fn normalise_url_passes_alterm_scheme() {
    assert_eq!(normalise_url("alterm://history"), "alterm://history");
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p browser resolve_input`
Expected: FAIL — `resolve_input` not found

- [ ] **Step 3: Implement**

Add above `normalise_url` in `crates/browser/src/lib.rs`:

```rust
/// Resolve text typed in the URL bar into a navigable URL.
///
/// Applied ONLY to URL-bar submissions — navigation events reported by the
/// webview are already real URLs and go through [`normalise_url`] instead.
///
/// - Explicit scheme (`http`, `https`, `about`, `alterm`) → unchanged.
/// - Single token with a dot in its host part, or `localhost[:port]` →
///   `https://` prefixed.
/// - Anything else → search via `search_engine` (a URL template whose `{}`
///   is replaced with the percent-encoded query; appended if no `{}`).
pub fn resolve_input(input: &str, search_engine: &str) -> String {
    let t = input.trim();
    if t.is_empty() {
        return "about:blank".to_string();
    }
    let lower = t.to_lowercase();
    if lower.starts_with("http://")
        || lower.starts_with("https://")
        || lower.starts_with("about:")
        || lower.starts_with("alterm://")
    {
        return t.to_string();
    }
    let no_spaces = !t.contains(char::is_whitespace);
    let host = t.split('/').next().unwrap_or(t);
    let hostname = host.split(':').next().unwrap_or(host);
    if no_spaces && (hostname == "localhost" || hostname.contains('.')) {
        return format!("https://{t}");
    }
    let q = percent_encode(t);
    if search_engine.contains("{}") {
        search_engine.replacen("{}", &q, 1)
    } else {
        format!("{search_engine}{q}")
    }
}

/// Percent-encode a query string for use in a URL (RFC 3986 unreserved
/// characters pass through; everything else is `%XX`-escaped).
pub fn percent_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}
```

In `normalise_url`, extend the scheme check so internal pages survive normalisation:

```rust
    if trimmed.starts_with("http://")
        || trimmed.starts_with("https://")
        || trimmed.starts_with("about:")
        || trimmed.starts_with("alterm://")
    {
        trimmed.to_string()
    } else {
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p browser`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/browser/src/lib.rs
git commit -m "feat(browser): smart URL-bar input resolution (URL vs search)"
```

---

### Task 4: Internal pages (alterm://history, alterm://bookmarks)

**Files:**
- Create: `crates/browser/src/internal_pages.rs`
- Modify: `crates/browser/src/lib.rs` (add `pub mod internal_pages;`)
- Modify: `crates/browser/Cargo.toml` (add `chrono = "0.4"` to `[dependencies]`)

**Interfaces:**
- Consumes: `history::{HistoryEntry, Bookmark, Stores, with_stores}` from Task 2.
- Produces (in `browser::internal_pages`):
  - `fn respond(uri: &str) -> (&'static str, String)` — full router: takes the request URI (e.g. `alterm://history?q=rust`), reads the thread-local stores, returns `(content_type, body)`. Works (with an empty store view) when stores are uninitialised.
  - `fn history_page(entries: &[HistoryEntry], bookmarks: &[Bookmark], query: &str) -> String`
  - `fn bookmarks_page(bookmarks: &[Bookmark]) -> String`
  - `fn error_page(path: &str) -> String`
  - `fn html_escape(s: &str) -> String`
  - `fn parse_query(uri: &str) -> String` — extracts and percent-decodes the `q` parameter.
- IPC contract produced here and consumed in Tasks 6/7 — buttons post JSON via `window.ipc.postMessage`:
  - `{"cmd":"history-delete","url":"...","ts":123}`
  - `{"cmd":"history-clear"}`
  - `{"cmd":"bookmark-remove","url":"..."}`

- [ ] **Step 1: Add chrono**

In `crates/browser/Cargo.toml` `[dependencies]`: `chrono = "0.4"`

- [ ] **Step 2: Write the failing tests**

Create `crates/browser/src/internal_pages.rs`:

```rust
//! HTML generation for alterm's internal browser pages
//! (`alterm://history`, `alterm://bookmarks`).
//!
//! Pure functions from store data to HTML strings, plus a `respond` router
//! used by the webview custom-protocol handler. Every title and URL is
//! HTML-escaped: page titles are untrusted input.

use chrono::{Local, TimeZone};

use crate::history::{self, Bookmark, HistoryEntry};

// (implementation added in Step 4)

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(url: &str, title: &str, ts: u64) -> HistoryEntry {
        HistoryEntry { url: url.into(), title: title.into(), timestamp: ts }
    }

    #[test]
    fn html_escape_neutralises_hostile_input() {
        assert_eq!(
            html_escape(r#"<script>alert("x")</script>&'"#),
            "&lt;script&gt;alert(&quot;x&quot;)&lt;/script&gt;&amp;&#39;"
        );
    }

    #[test]
    fn history_page_escapes_titles_and_urls() {
        let hostile = entry(
            "https://evil.com/?a=<b>&c=\"d\"",
            "<img src=x onerror=alert(1)>",
            1,
        );
        let html = history_page(&[hostile], &[], "");
        assert!(!html.contains("<img src=x"));
        assert!(html.contains("&lt;img src=x onerror=alert(1)&gt;"));
        assert!(!html.contains(r#"?a=<b>"#));
    }

    #[test]
    fn history_page_lists_entries_and_reflects_query() {
        let entries = vec![
            entry("https://a.com", "Site A", 1_700_000_000),
            entry("https://b.com", "", 1_700_000_100),
        ];
        let html = history_page(&entries, &[], "rust");
        assert!(html.contains("Site A"));
        assert!(html.contains("https://b.com")); // untitled entries show URL
        assert!(html.contains(r#"value="rust""#)); // search box keeps query
        assert!(html.contains(r#"href="https://a.com""#));
    }

    #[test]
    fn history_page_shows_bookmarks_section_and_empty_state() {
        let bm = vec![Bookmark { url: "https://b.com".into(), title: "B".into() }];
        let html = history_page(&[], &bm, "");
        assert!(html.contains("Bookmarks"));
        assert!(html.contains("No history yet"));
    }

    #[test]
    fn bookmarks_page_lists_and_escapes() {
        let bm = vec![Bookmark {
            url: "https://b.com".into(),
            title: "<b>B</b>".into(),
        }];
        let html = bookmarks_page(&bm);
        assert!(html.contains("&lt;b&gt;B&lt;/b&gt;"));
        assert!(html.contains(r#"href="https://b.com""#));
    }

    #[test]
    fn parse_query_decodes_q_param() {
        assert_eq!(parse_query("alterm://history?q=rust%20lifetimes"), "rust lifetimes");
        assert_eq!(parse_query("alterm://history?q=a%26b"), "a&b");
        assert_eq!(parse_query("alterm://history"), "");
    }

    #[test]
    fn respond_routes_known_and_unknown_paths() {
        // Stores are uninitialised on the test thread: pages render empty.
        let (mime, body) = respond("alterm://history");
        assert_eq!(mime, "text/html");
        assert!(body.contains("History"));
        let (_, body) = respond("alterm://bookmarks");
        assert!(body.contains("Bookmarks"));
        let (_, body) = respond("alterm://nonsense");
        assert!(body.contains("Unknown internal page"));
    }
}
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test -p browser internal_pages`
Expected: FAIL — compile errors

- [ ] **Step 4: Implement**

Fill in above the tests module:

```rust
/// Escape text for safe embedding in HTML content or attribute values.
pub fn html_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
    out
}

/// Extract and percent-decode the `q` query parameter from a URI.
pub fn parse_query(uri: &str) -> String {
    let Some(qs) = uri.splitn(2, '?').nth(1) else { return String::new() };
    for pair in qs.split('&') {
        if let Some(v) = pair.strip_prefix("q=") {
            return percent_decode(v);
        }
    }
    String::new()
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' => {
                if let (Some(h), Some(l)) = (
                    bytes.get(i + 1).and_then(|b| (*b as char).to_digit(16)),
                    bytes.get(i + 2).and_then(|b| (*b as char).to_digit(16)),
                ) {
                    out.push((h * 16 + l) as u8);
                    i += 3;
                } else {
                    out.push(b'%');
                    i += 1;
                }
            }
            b'+' => { out.push(b' '); i += 1; }
            b => { out.push(b); i += 1; }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Shared dark-theme CSS + the IPC helper script for internal pages.
const PAGE_STYLE: &str = r#"<style>
:root { color-scheme: dark; }
body { background: #141419; color: #ccccd6; font: 14px/1.5 system-ui, sans-serif;
       margin: 0; padding: 24px 32px; }
a { color: #8ab4f8; text-decoration: none; overflow-wrap: anywhere; }
a:hover { text-decoration: underline; }
h1 { font-size: 20px; margin: 0 0 16px; color: #e8e8ee; }
h2 { font-size: 13px; text-transform: uppercase; letter-spacing: .08em;
     color: #8888a0; margin: 24px 0 8px; }
.row { display: flex; align-items: baseline; gap: 12px; padding: 5px 8px;
       border-radius: 6px; }
.row:hover { background: #1e1e28; }
.row:hover .del { visibility: visible; }
.time { color: #666678; font-size: 12px; min-width: 48px; }
.url { color: #666678; font-size: 12px; overflow-wrap: anywhere; }
.del { visibility: hidden; margin-left: auto; cursor: pointer; color: #666678;
       background: none; border: none; font-size: 13px; }
.del:hover { color: #ff7b72; }
.toolbar { display: flex; gap: 12px; margin-bottom: 20px; align-items: center; }
input[type=text] { background: #1e1e28; color: #ccccd6; border: 1px solid #2e2e3a;
       border-radius: 6px; padding: 6px 10px; width: 320px; font-size: 13px; }
.btn { background: #1e1e28; color: #ccccd6; border: 1px solid #2e2e3a;
       border-radius: 6px; padding: 6px 12px; cursor: pointer; font-size: 13px; }
.btn:hover { background: #26262f; }
.empty { color: #666678; padding: 16px 8px; }
nav { margin-bottom: 20px; font-size: 13px; }
nav a { margin-right: 16px; color: #8888a0; }
</style>
<script>
function ipc(obj) { window.ipc.postMessage(JSON.stringify(obj)); }
function delEntry(url, ts) { ipc({cmd: 'history-delete', url: url, ts: ts}); }
function clearHistory() {
  if (confirm('Clear all browsing history?')) ipc({cmd: 'history-clear'});
}
function removeBookmark(url) { ipc({cmd: 'bookmark-remove', url: url}); }
</script>"#;

fn page_shell(title: &str, body: &str) -> String {
    format!(
        "<!DOCTYPE html><html><head><meta charset=\"utf-8\">\
         <title>{}</title>{PAGE_STYLE}</head><body>{body}</body></html>",
        html_escape(title)
    )
}

fn nav_links() -> &'static str {
    r#"<nav><a href="alterm://history">History</a><a href="alterm://bookmarks">Bookmarks</a></nav>"#
}

/// `HH:MM` in local time.
fn format_time(ts: u64) -> String {
    Local
        .timestamp_opt(ts as i64, 0)
        .single()
        .map(|t| t.format("%H:%M").to_string())
        .unwrap_or_default()
}

/// Day heading for grouping: Today / Yesterday / `Weekday, D Month YYYY`.
fn format_day(ts: u64) -> String {
    let Some(then) = Local.timestamp_opt(ts as i64, 0).single() else {
        return "Unknown date".to_string();
    };
    let today = Local::now().date_naive();
    let date = then.date_naive();
    if date == today {
        "Today".to_string()
    } else if today.pred_opt() == Some(date) {
        "Yesterday".to_string()
    } else {
        then.format("%A, %-d %B %Y").to_string()
    }
}

fn history_row(e: &HistoryEntry) -> String {
    let label = if e.title.is_empty() { &e.url } else { &e.title };
    // JSON-encode the url for the onclick JS string (quotes/backslashes).
    let js_url = serde_json::to_string(&e.url).unwrap_or_else(|_| "\"\"".into());
    format!(
        r#"<div class="row"><span class="time">{}</span><a href="{}">{}</a><span class="url">{}</span><button class="del" title="Remove from history" onclick='delEntry({}, {})'>&#10005;</button></div>"#,
        format_time(e.timestamp),
        html_escape(&e.url),
        html_escape(label),
        html_escape(&e.url),
        html_escape(&js_url),
        e.timestamp,
    )
}

/// The `alterm://history` page (also the start page): search box,
/// bookmarks strip, entries grouped by day, newest first.
pub fn history_page(entries: &[HistoryEntry], bookmarks: &[Bookmark], query: &str) -> String {
    let mut body = String::new();
    body.push_str(nav_links());
    body.push_str("<h1>History</h1>");
    body.push_str(&format!(
        r#"<div class="toolbar"><form action="alterm://history" method="get">
           <input type="text" name="q" placeholder="Search history..." value="{}" autofocus></form>
           <button class="btn" onclick="clearHistory()">Clear history</button></div>"#,
        html_escape(query)
    ));

    if !bookmarks.is_empty() && query.is_empty() {
        body.push_str("<h2>Bookmarks</h2>");
        for b in bookmarks {
            let label = if b.title.is_empty() { &b.url } else { &b.title };
            body.push_str(&format!(
                r#"<div class="row"><a href="{}">&#9733; {}</a></div>"#,
                html_escape(&b.url),
                html_escape(label),
            ));
        }
    }

    if entries.is_empty() {
        body.push_str(r#"<div class="empty">No history yet. Pages you visit will appear here.</div>"#);
    } else {
        let mut current_day = String::new();
        for e in entries {
            let day = format_day(e.timestamp);
            if day != current_day {
                body.push_str(&format!("<h2>{}</h2>", html_escape(&day)));
                current_day = day;
            }
            body.push_str(&history_row(e));
        }
    }
    page_shell("History", &body)
}

/// The `alterm://bookmarks` page.
pub fn bookmarks_page(bookmarks: &[Bookmark]) -> String {
    let mut body = String::new();
    body.push_str(nav_links());
    body.push_str("<h1>Bookmarks</h1>");
    if bookmarks.is_empty() {
        body.push_str(r#"<div class="empty">No bookmarks yet. Press the &#9734; button in the URL bar to bookmark a page.</div>"#);
    } else {
        for b in bookmarks {
            let label = if b.title.is_empty() { &b.url } else { &b.title };
            let js_url = serde_json::to_string(&b.url).unwrap_or_else(|_| "\"\"".into());
            body.push_str(&format!(
                r#"<div class="row"><a href="{}">&#9733; {}</a><span class="url">{}</span><button class="del" title="Remove bookmark" onclick='removeBookmark({})'>&#10005;</button></div>"#,
                html_escape(&b.url),
                html_escape(label),
                html_escape(&b.url),
                html_escape(&js_url),
            ));
        }
    }
    page_shell("Bookmarks", &body)
}

/// Error page for unknown `alterm://` paths.
pub fn error_page(path: &str) -> String {
    let body = format!(
        r#"{}<h1>Unknown internal page</h1><div class="empty">{} is not a known alterm page. Try <a href="alterm://history">alterm://history</a>.</div>"#,
        nav_links(),
        html_escape(path),
    );
    page_shell("Not found", &body)
}

/// Router used by the webview custom-protocol handler. Reads the
/// thread-local stores (empty view when uninitialised).
pub fn respond(uri: &str) -> (&'static str, String) {
    let path = uri.splitn(2, '?').next().unwrap_or(uri);
    let path = path.trim_end_matches('/');
    match path {
        "alterm://history" => {
            let query = parse_query(uri);
            let (entries, bookmarks) = history::with_stores(|s| {
                (s.history.query(&query), s.bookmarks.list().to_vec())
            })
            .unwrap_or_default();
            ("text/html", history_page(&entries, &bookmarks, &query))
        }
        "alterm://bookmarks" => {
            let bookmarks = history::with_stores(|s| s.bookmarks.list().to_vec())
                .unwrap_or_default();
            ("text/html", bookmarks_page(&bookmarks))
        }
        other => ("text/html", error_page(other)),
    }
}
```

Add to `crates/browser/src/lib.rs`, next to `pub mod history;`:

```rust
pub mod internal_pages;
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p browser`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add crates/browser/Cargo.toml crates/browser/src/internal_pages.rs crates/browser/src/lib.rs Cargo.lock
git commit -m "feat(browser): internal alterm://history and alterm://bookmarks pages"
```

---

### Task 5: BrowserState — fresh-nav signal, zoom, load state; session zoom persistence

**Files:**
- Modify: `crates/browser/src/lib.rs` (`BrowserState` struct ~line 35, `new` ~line 63, `on_navigation` ~line 98; tests)
- Modify: `crates/workspace/src/session.rs` (`BlockState::Browser` line 22)
- Modify: `crates/workspace/src/block.rs` (`to_block_state` ~line 512 browser arm, `from_state` ~line 177 browser arm)

**Interfaces:**
- Consumes: existing `BrowserState`.
- Produces:
  - `BrowserState.zoom: f64` (default `1.0`), `BrowserState.set_loading(&mut self, loading: bool)`
  - `BrowserState::on_navigation(&mut self, url: &str) -> bool` — **signature change**: returns `true` when the navigation was fresh (recorded a new history entry), `false` for confirmed back/forward moves and duplicate reports. Callers use this to decide global-history recording.
  - `BlockState::Browser { url, history, history_index, zoom }` with `#[serde(default = "default_zoom")] zoom: f64`; `fn default_zoom() -> f64 { 1.0 }` in session.rs.

- [ ] **Step 1: Write the failing tests**

In `crates/browser/src/lib.rs` tests module:

```rust
#[test]
fn on_navigation_reports_freshness() {
    let mut s = BrowserState::new("https://a.com");
    assert!(s.on_navigation("https://b.com"));   // fresh
    assert!(!s.on_navigation("https://b.com"));  // duplicate of current
    assert!(s.begin_back());
    assert!(!s.on_navigation("https://a.com"));  // late back-report, not fresh
}

#[test]
fn zoom_defaults_to_one() {
    let s = BrowserState::new("https://a.com");
    assert_eq!(s.zoom, 1.0);
}

#[test]
fn set_loading_toggles() {
    let mut s = BrowserState::new("https://a.com");
    s.set_loading(true);
    assert!(s.loading);
    s.set_loading(false);
    assert!(!s.loading);
}
```

In `crates/workspace/src/session.rs` tests module (extend the existing round-trip coverage):

```rust
#[test]
fn browser_block_state_zoom_defaults_when_missing() {
    // A v1 session file written before the zoom field existed.
    let json = r#"{"Browser":{"url":"https://a.com","history":["https://a.com"],"history_index":0}}"#;
    let bs: BlockState = serde_json::from_str(json).unwrap();
    match bs {
        BlockState::Browser { zoom, .. } => assert_eq!(zoom, 1.0),
        other => panic!("expected browser, got {other:?}"),
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p browser on_navigation_reports_freshness && cargo test -p workspace browser_block_state_zoom`
Expected: FAIL — type errors / missing field

- [ ] **Step 3: Implement**

`crates/browser/src/lib.rs`:

1. Add to `BrowserState` (after `pending_move`):

```rust
    /// Page zoom factor (1.0 = 100%). Applied via the webview manager.
    pub zoom: f64,
```

and in `BrowserState::new`: `zoom: 1.0,`

2. Change `on_navigation` to return `bool`. In the `match self.pending_move` arms: back (`-1`) and forward (`1`) arms end with `false`; the fresh arm returns whether it pushed:

```rust
    pub fn on_navigation(&mut self, url: &str) -> bool {
        let url = normalise_url(url);

        let fresh = match self.pending_move {
            -1 => {
                self.history_index = self.history_index.saturating_sub(1);
                self.pending_move = 0;
                false
            }
            1 => {
                if self.history_index + 1 < self.history.len() {
                    self.history_index += 1;
                }
                self.pending_move = 0;
                false
            }
            _ => {
                if url != self.url {
                    self.history.truncate(self.history_index + 1);
                    self.history.push(url.clone());
                    self.history_index = self.history.len() - 1;
                    true
                } else {
                    false
                }
            }
        };

        self.url = url.clone();
        self.input_url = url;
        self.loading = false;
        self.update_nav_flags();
        fresh
    }
```

(keep the existing `log::debug!` at the end, before `fresh` is returned — bind it as shown with `let fresh = ...;`)

3. Add after `reload`:

```rust
    /// Update the loading flag from a webview load-state event.
    pub fn set_loading(&mut self, loading: bool) {
        self.loading = loading;
    }
```

`crates/workspace/src/session.rs`:

```rust
    Browser {
        url: String,
        history: Vec<String>,
        history_index: usize,
        #[serde(default = "default_zoom")]
        zoom: f64,
    },
```

and near the top of the file:

```rust
fn default_zoom() -> f64 {
    1.0
}
```

Also update the `sample()` test fixture in session.rs to include `zoom: 1.0,` in its `BlockState::Browser` literal.

`crates/workspace/src/block.rs`:

- `to_block_state` browser arm gains `zoom: state.zoom,`
- `from_state` browser arm gains, inside the `if let Block::Browser { state }`:

```rust
                    state.zoom = *zoom;
```

and the pattern becomes `BlockState::Browser { url, history, history_index, zoom }`.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p browser && cargo test -p workspace`
Expected: PASS. Existing browser tests calling `on_navigation` compile unchanged (return value ignored); if clippy complains about unused results later, that's fine — `on_navigation` is not `#[must_use]`.

- [ ] **Step 5: Commit**

```bash
git add crates/browser/src/lib.rs crates/workspace/src/session.rs crates/workspace/src/block.rs
git commit -m "feat(browser): fresh-navigation signal, zoom state, session zoom persistence"
```

---

### Task 6: webview_manager — custom protocol, title/load/IPC/find events, stop/zoom/find APIs

**Files:**
- Modify: `crates/browser/src/webview_manager.rs`
- Modify: `crates/browser/src/lib.rs` (extend the non-supported-platform stub module)
- Modify: `crates/browser/Cargo.toml` (Linux-only `webkit2gtk` dep)

**Interfaces:**
- Consumes: `internal_pages::respond`, the IPC JSON contract from Task 4.
- Produces (in `webview_manager`, each with a no-op stub in the unsupported-platform module in lib.rs):
  - `fn drain_title_events() -> Vec<(u64, String)>`
  - `fn drain_load_events() -> Vec<(u64, bool)>` — `true` = load started, `false` = finished
  - `fn drain_ipc_events() -> Vec<(u64, String)>` — raw JSON bodies
  - `fn drain_find_events() -> Vec<(u64, u32)>` — match counts from find
  - `fn stop(pane_id: u64)`
  - `fn set_zoom(pane_id: u64, level: f64)`
  - `fn find_start(pane_id: u64, text: &str)`, `fn find_next(pane_id: u64)`, `fn find_prev(pane_id: u64)`, `fn find_finish(pane_id: u64)`
- In-page shortcut IPC contract (consumed in Task 8): `{"cmd":"shortcut","action":"back"|"forward"|"focus-url"|"reload"|"history"|"bookmark"|"find"|"zoom-in"|"zoom-out"|"zoom-reset"}`

This task is webview plumbing — no unit tests are possible; the gate is `cargo check -p browser` on all touched cfg paths plus manual verification in Task 11.

- [ ] **Step 1: Add the Linux webkit2gtk dependency**

Run `cargo tree -p wry -e normal | grep webkit2gtk` to see the exact webkit2gtk version wry 0.55 uses, then add the SAME version requirement to `crates/browser/Cargo.toml`:

```toml
[target.'cfg(target_os = "linux")'.dependencies]
gtk = "0.18"
webkit2gtk = { version = "=2.0.1", features = ["v2_38"] }
```

(Adjust `=2.0.1`/features to exactly match wry's resolved version so cargo unifies them into one copy.)

- [ ] **Step 2: Add event queues and drains**

In `webview_manager.rs`, extend the `thread_local!` block:

```rust
    /// Title-change events `(pane_id, title)`.
    static TITLE_EVENTS: RefCell<Vec<(u64, String)>> = const { RefCell::new(Vec::new()) };
    /// Load-state events `(pane_id, started)`; `false` = finished.
    static LOAD_EVENTS: RefCell<Vec<(u64, bool)>> = const { RefCell::new(Vec::new()) };
    /// IPC messages `(pane_id, json_body)` posted by pages via window.ipc.
    static IPC_EVENTS: RefCell<Vec<(u64, String)>> = const { RefCell::new(Vec::new()) };
    /// Find-in-page match counts `(pane_id, count)`.
    static FIND_EVENTS: RefCell<Vec<(u64, u32)>> = const { RefCell::new(Vec::new()) };
```

and add drains next to `drain_nav_events`:

```rust
/// Drain queued title-change events.
pub fn drain_title_events() -> Vec<(u64, String)> {
    TITLE_EVENTS.with(|q| std::mem::take(&mut *q.borrow_mut()))
}

/// Drain queued load-state events (`true` = started, `false` = finished).
pub fn drain_load_events() -> Vec<(u64, bool)> {
    LOAD_EVENTS.with(|q| std::mem::take(&mut *q.borrow_mut()))
}

/// Drain queued IPC messages (raw JSON bodies posted by internal pages
/// and the shortcut forwarder script).
pub fn drain_ipc_events() -> Vec<(u64, String)> {
    IPC_EVENTS.with(|q| std::mem::take(&mut *q.borrow_mut()))
}

/// Drain queued find-in-page match counts.
pub fn drain_find_events() -> Vec<(u64, u32)> {
    FIND_EVENTS.with(|q| std::mem::take(&mut *q.borrow_mut()))
}
```

- [ ] **Step 3: Extend `create_webview`**

Add the in-page shortcut forwarder as a module const:

```rust
/// Forwards browser shortcuts pressed while the page has keyboard focus.
/// The capture phase listener beats page handlers; only our exact combos
/// are intercepted.
const SHORTCUT_FORWARDER: &str = r#"
document.addEventListener('keydown', (e) => {
  const k = e.key;
  let action = null;
  if (e.altKey && !e.ctrlKey && k === 'ArrowLeft') action = 'back';
  else if (e.altKey && !e.ctrlKey && k === 'ArrowRight') action = 'forward';
  else if (e.ctrlKey && !e.altKey && !e.shiftKey) {
    if (k === 'l' || k === 'L') action = 'focus-url';
    else if (k === 'r' || k === 'R') action = 'reload';
    else if (k === 'h' || k === 'H') action = 'history';
    else if (k === 'd' || k === 'D') action = 'bookmark';
    else if (k === 'f' || k === 'F') action = 'find';
    else if (k === '=' || k === '+') action = 'zoom-in';
    else if (k === '-') action = 'zoom-out';
    else if (k === '0') action = 'zoom-reset';
  }
  if (action) {
    e.preventDefault();
    e.stopPropagation();
    window.ipc.postMessage(JSON.stringify({cmd: 'shortcut', action: action}));
  }
}, true);
"#;
```

In `create_webview`, chain onto the existing `WebViewBuilder` (after `.with_navigation_handler(...)`):

```rust
        .with_document_title_changed_handler(move |title| {
            TITLE_EVENTS.with(|q| q.borrow_mut().push((pane_id, title)));
        })
        .with_on_page_load_handler(move |event, _url| {
            let started = matches!(event, wry::PageLoadEvent::Started);
            LOAD_EVENTS.with(|q| q.borrow_mut().push((pane_id, started)));
        })
        .with_ipc_handler(move |req| {
            IPC_EVENTS.with(|q| q.borrow_mut().push((pane_id, req.body().clone())));
        })
        .with_initialization_script(SHORTCUT_FORWARDER)
        .with_custom_protocol("alterm".into(), |_webview_id, request| {
            let uri = request.uri().to_string();
            let (mime, body) = crate::internal_pages::respond(&uri);
            wry::http::Response::builder()
                .header("Content-Type", mime)
                .body(std::borrow::Cow::<'static, [u8]>::Owned(body.into_bytes()))
                .unwrap_or_else(|_| {
                    wry::http::Response::new(std::borrow::Cow::Borrowed(&b""[..]))
                })
        })
```

**Adjust to the actual wry 0.55 signatures** (check `~/.cargo/registry/.../wry-0.55*/src/lib.rs` or docs.rs/wry/0.55): `with_ipc_handler` receives `wry::http::Request<String>` (`req.body()` is `&String`); `with_custom_protocol` handler is `Fn(WebViewId, Request<Vec<u8>>) -> Response<Cow<'static, [u8]>>`.

**Duplicate-scheme registration:** on Linux, wry registers the URI scheme on the webview's `WebContext`. If creating a *second* browser pane errors with a duplicate-scheme panic/`Err`, share one `WebContext` across all webviews: add a `thread_local! { static WEB_CONTEXT: RefCell<Option<wry::WebContext>> = ... }`, create it once (`wry::WebContext::new(Some(data_dir))` with the same alterm data dir so cookies persist), and pass it via `WebViewBuilder::with_web_context(&mut ctx)` (Linux builder ext). Recent wry versions dedupe scheme registration per context and dispatch by `WebViewId` — this is why the handler receives the id. Verify by opening two browser panes (Task 11 checklist).

- [ ] **Step 4: Add control APIs**

After `go_forward`:

```rust
/// Stop the current page load (Linux: webkit stop_loading; no-op elsewhere).
pub fn stop(pane_id: u64) {
    #[cfg(target_os = "linux")]
    WEBVIEWS.with(|wvs| {
        if let Some(wv) = wvs.borrow().get(&pane_id) {
            use webkit2gtk::WebViewExt;
            use wry::WebViewExtUnix;
            wv.webview().stop_loading();
        }
    });
    #[cfg(not(target_os = "linux"))]
    let _ = pane_id;
}

/// Set the page zoom factor (1.0 = 100%).
pub fn set_zoom(pane_id: u64, level: f64) {
    WEBVIEWS.with(|wvs| {
        if let Some(wv) = wvs.borrow().get(&pane_id) {
            if let Err(e) = wv.zoom(level) {
                log::warn!("WebView zoom failed for pane {pane_id}: {e}");
            }
        }
    });
}

/// Begin (or update) a find-in-page search. Emits match counts via
/// `drain_find_events`.
pub fn find_start(pane_id: u64, text: &str) {
    #[cfg(target_os = "linux")]
    WEBVIEWS.with(|wvs| {
        if let Some(wv) = wvs.borrow().get(&pane_id) {
            use webkit2gtk::{FindControllerExt, FindOptions, WebViewExt};
            use wry::WebViewExtUnix;
            if let Some(fc) = wv.webview().find_controller() {
                fc.count_matches(
                    text,
                    (FindOptions::CASE_INSENSITIVE | FindOptions::WRAP_AROUND).bits(),
                    u32::MAX,
                );
                fc.search(
                    text,
                    (FindOptions::CASE_INSENSITIVE | FindOptions::WRAP_AROUND).bits(),
                    u32::MAX,
                );
            }
        }
    });
    #[cfg(not(target_os = "linux"))]
    let _ = (pane_id, text);
}

/// Jump to the next find match.
pub fn find_next(pane_id: u64) {
    #[cfg(target_os = "linux")]
    WEBVIEWS.with(|wvs| {
        if let Some(wv) = wvs.borrow().get(&pane_id) {
            use webkit2gtk::{FindControllerExt, WebViewExt};
            use wry::WebViewExtUnix;
            if let Some(fc) = wv.webview().find_controller() {
                fc.search_next();
            }
        }
    });
    #[cfg(not(target_os = "linux"))]
    let _ = pane_id;
}

/// Jump to the previous find match.
pub fn find_prev(pane_id: u64) {
    #[cfg(target_os = "linux")]
    WEBVIEWS.with(|wvs| {
        if let Some(wv) = wvs.borrow().get(&pane_id) {
            use webkit2gtk::{FindControllerExt, WebViewExt};
            use wry::WebViewExtUnix;
            if let Some(fc) = wv.webview().find_controller() {
                fc.search_previous();
            }
        }
    });
    #[cfg(not(target_os = "linux"))]
    let _ = pane_id;
}

/// End the find session and clear highlights.
pub fn find_finish(pane_id: u64) {
    #[cfg(target_os = "linux")]
    WEBVIEWS.with(|wvs| {
        if let Some(wv) = wvs.borrow().get(&pane_id) {
            use webkit2gtk::{FindControllerExt, WebViewExt};
            use wry::WebViewExtUnix;
            if let Some(fc) = wv.webview().find_controller() {
                fc.search_finish();
            }
        }
    });
    #[cfg(not(target_os = "linux"))]
    let _ = pane_id;
}
```

Connect the match-count signal at creation time — in `create_webview`, after the webview is built and before it's inserted into `WEBVIEWS`:

```rust
    #[cfg(target_os = "linux")]
    {
        use webkit2gtk::{FindControllerExt, WebViewExt};
        use wry::WebViewExtUnix;
        if let Some(fc) = webview.webview().find_controller() {
            fc.connect_counted_matches(move |_, count| {
                FIND_EVENTS.with(|q| q.borrow_mut().push((pane_id, count)));
            });
        }
    }
```

**API-name check:** if `find_controller()` returns the controller directly (not `Option`), drop the `if let Some`. If `FindOptions` methods take the bitflags type rather than `.bits()`, pass the flags directly. Match whatever the resolved `webkit2gtk` crate exposes — the calls are `count_matches`, `search`, `search_next`, `search_previous`, `search_finish`, `connect_counted_matches`.

- [ ] **Step 5: Update the unsupported-platform stub in lib.rs**

Extend the fallback `webview_manager` module in `crates/browser/src/lib.rs`:

```rust
    pub fn drain_title_events() -> Vec<(u64, String)> { Vec::new() }
    pub fn drain_load_events() -> Vec<(u64, bool)> { Vec::new() }
    pub fn drain_ipc_events() -> Vec<(u64, String)> { Vec::new() }
    pub fn drain_find_events() -> Vec<(u64, u32)> { Vec::new() }
    pub fn stop(_pane_id: u64) {}
    pub fn set_zoom(_pane_id: u64, _level: f64) {}
    pub fn find_start(_pane_id: u64, _text: &str) {}
    pub fn find_next(_pane_id: u64) {}
    pub fn find_prev(_pane_id: u64) {}
    pub fn find_finish(_pane_id: u64) {}
```

- [ ] **Step 6: Verify it compiles and existing tests pass**

Run: `cargo check -p browser && cargo test -p browser && cargo clippy -p browser`
Expected: clean build, all tests PASS

- [ ] **Step 7: Commit**

```bash
git add crates/browser/Cargo.toml crates/browser/src/webview_manager.rs crates/browser/src/lib.rs Cargo.lock
git commit -m "feat(browser): custom protocol, title/load/ipc/find events, stop/zoom/find APIs"
```

---

### Task 7: main.rs wiring — stores init, event drains, global recording, smart navigation, start page

**Files:**
- Modify: `alterm/src/main.rs`:
  - `main()`/app construction (find where `AppConfig` is loaded; init stores there)
  - `Message` enum (~line 375)
  - `Message::Tick` handler (~line 1110)
  - `apply_browser_nav_events` (~line 758)
  - `Message::OpenBrowser` (~line 1780) and `Message::BrowserNavigate` (~line 1792)

**Interfaces:**
- Consumes: `browser::history::{init, with_stores, now_ts}`, `browser::resolve_input`, `webview_manager::{drain_title_events, drain_load_events, drain_ipc_events, stop, set_zoom}`, `BrowserState::on_navigation -> bool`, `state.set_loading`.
- Produces `Message` variants used by Tasks 8–10:

```rust
    BrowserToggleBookmark(pane_grid::Pane),
    BrowserOpenHistory(pane_grid::Pane),
    BrowserStop(pane_grid::Pane),
    BrowserZoomIn(pane_grid::Pane),
    BrowserZoomOut(pane_grid::Pane),
    BrowserZoomReset(pane_grid::Pane),
```

- Produces helper: `fn find_browser_pane(&self, pane_id: u64) -> Option<(u64, pane_grid::Pane)>` — reverse `webview_key` lookup returning `(tab_id, pane)`.

- [ ] **Step 1: Initialise stores at startup**

Where `AppConfig` is loaded during app construction (search `AppConfig::load` in main.rs), add:

```rust
        // Global browser history/bookmarks live under the XDG data dir.
        let browser_data_dir = dirs::data_dir()
            .unwrap_or_else(|| {
                dirs::home_dir()
                    .unwrap_or_else(|| std::path::PathBuf::from("."))
                    .join(".local/share")
            })
            .join("alterm");
        browser::history::init(&browser_data_dir);
```

(`dirs` is already a dependency of the alterm crate — it's used in `Message::OpenPreview`.)

- [ ] **Step 2: Add the Message variants and reverse lookup helper**

Add the six variants above to the `Message` enum next to the existing `Browser*` variants (~line 375).

Add near `apply_browser_nav_events`:

```rust
    /// Resolve a webview key back to its (tab_id, pane). Used when events
    /// arrive keyed by webview id.
    fn find_browser_pane(&self, pane_id: u64) -> Option<(u64, pane_grid::Pane)> {
        for tab in &self.tabs {
            let tab_id = tab.id;
            for (pane, block) in tab.panes.iter() {
                if block.is_browser() && webview_key(tab_id, *pane) == pane_id {
                    return Some((tab_id, *pane));
                }
            }
        }
        None
    }
```

- [ ] **Step 3: Record global history in `apply_browser_nav_events`**

Change the inner match so fresh navigations are recorded (skip internal pages):

```rust
                    if let Block::Browser { state } = block {
                        let fresh = state.on_navigation(&url);
                        if fresh && !url.starts_with("alterm://") && !url.starts_with("about:") {
                            browser::history::with_stores(|s| {
                                s.history.record_visit(&url, browser::history::now_ts());
                            });
                        }
                    }
```

- [ ] **Step 4: Drain title / load / IPC events on Tick**

In the `Message::Tick` handler, directly after `self.apply_browser_nav_events();`, add a call to a new method and implement it next to `apply_browser_nav_events`:

```rust
                self.apply_browser_webview_events();
```

```rust
    /// Drain title, load-state, and IPC events from the webviews and apply
    /// them to pane state and the global stores.
    fn apply_browser_webview_events(&mut self) -> Task<Message> {
        for (pane_id, title) in webview_manager::drain_title_events() {
            if let Some((tab_id, pane)) = self.find_browser_pane(pane_id) {
                if let Some(tab) = self.tabs.iter_mut().find(|t| t.id == tab_id) {
                    if let Some(Block::Browser { state }) = tab.panes.get_mut(pane) {
                        state.title = title.clone();
                        if !state.url.starts_with("alterm://") {
                            let url = state.url.clone();
                            browser::history::with_stores(|s| {
                                s.history.set_title(&url, &title);
                            });
                        }
                    }
                }
            }
        }

        for (pane_id, started) in webview_manager::drain_load_events() {
            if let Some((tab_id, pane)) = self.find_browser_pane(pane_id) {
                if let Some(tab) = self.tabs.iter_mut().find(|t| t.id == tab_id) {
                    if let Some(Block::Browser { state }) = tab.panes.get_mut(pane) {
                        state.set_loading(started);
                    }
                }
            }
        }

        let ipc_events = webview_manager::drain_ipc_events();
        let mut tasks = Vec::new();
        for (pane_id, body) in ipc_events {
            tasks.push(self.handle_browser_ipc(pane_id, &body));
        }
        Task::batch(tasks)
    }

    /// Apply one IPC message posted by a page (internal-page actions and
    /// forwarded keyboard shortcuts).
    fn handle_browser_ipc(&mut self, pane_id: u64, body: &str) -> Task<Message> {
        let Ok(msg) = serde_json::from_str::<serde_json::Value>(body) else {
            log::warn!("browser ipc: unparseable message: {body}");
            return Task::none();
        };
        let cmd = msg.get("cmd").and_then(|v| v.as_str()).unwrap_or("");
        match cmd {
            "history-delete" => {
                let url = msg.get("url").and_then(|v| v.as_str()).unwrap_or("");
                let ts = msg.get("ts").and_then(|v| v.as_u64()).unwrap_or(0);
                browser::history::with_stores(|s| s.history.delete(url, ts));
                webview_manager::reload(pane_id);
                Task::none()
            }
            "history-clear" => {
                browser::history::with_stores(|s| s.history.clear());
                webview_manager::reload(pane_id);
                Task::none()
            }
            "bookmark-remove" => {
                let url = msg.get("url").and_then(|v| v.as_str()).unwrap_or("");
                browser::history::with_stores(|s| {
                    if s.bookmarks.is_bookmarked(url) {
                        s.bookmarks.toggle(url, "");
                    }
                });
                webview_manager::reload(pane_id);
                Task::none()
            }
            "shortcut" => {
                let action = msg.get("action").and_then(|v| v.as_str()).unwrap_or("");
                self.handle_browser_shortcut_ipc(pane_id, action)
            }
            other => {
                log::warn!("browser ipc: unknown cmd {other:?}");
                Task::none()
            }
        }
    }
```

Add a placeholder for the shortcut dispatch (fully implemented in Task 8):

```rust
    /// Route a shortcut forwarded from inside a webview page.
    fn handle_browser_shortcut_ipc(&mut self, pane_id: u64, action: &str) -> Task<Message> {
        let _ = (pane_id, action);
        Task::none()
    }
```

Note `Message::Tick`'s handler must propagate the returned task — check how the Tick arm returns; batch this task into its existing return (e.g. `let wv_task = self.apply_browser_webview_events();` then include `wv_task` in the arm's final `Task::batch`). The alterm crate already depends on `serde_json` via workspace crates — if `use serde_json` fails, add `serde_json = "1"` to `alterm/Cargo.toml`.

- [ ] **Step 5: Smart navigation + start page + new message handlers**

`Message::OpenBrowser` (~line 1780): change

```rust
                let url = "alterm://history";
```

`Message::BrowserNavigate` (~line 1792): resolve input through the search engine first:

```rust
            Message::BrowserNavigate(pane, url) => {
                let target = browser::resolve_input(&url, &self.config.browser.search_engine);
                let tab_id = self.active_tab().id;
                let pane_id = webview_key(tab_id, pane);
                let tab = self.active_tab_mut();
                if let Some(Block::Browser { state }) = tab.panes.get_mut(pane) {
                    let target = state.navigate(&target);
                    webview_manager::navigate(pane_id, &target);
                }
            }
```

(Confirm the config field path: the app stores its config as `self.config` — search `state.config.terminal.copy_on_select` in main.rs, which shows the pattern; use the same root.)

Add handlers next to the other Browser messages:

```rust
            Message::BrowserToggleBookmark(pane) => {
                let tab = self.active_tab_mut();
                if let Some(Block::Browser { state }) = tab.panes.get_mut(pane) {
                    let url = state.url.clone();
                    let title = state.title.clone();
                    if !url.starts_with("alterm://") {
                        browser::history::with_stores(|s| s.bookmarks.toggle(&url, &title));
                    }
                }
            }
            Message::BrowserOpenHistory(pane) => {
                return self.update(Message::BrowserNavigate(pane, "alterm://history".into()));
            }
            Message::BrowserStop(pane) => {
                let tab_id = self.active_tab().id;
                let pane_id = webview_key(tab_id, pane);
                let tab = self.active_tab_mut();
                if let Some(Block::Browser { state }) = tab.panes.get_mut(pane) {
                    state.set_loading(false);
                    webview_manager::stop(pane_id);
                }
            }
            Message::BrowserZoomIn(pane) => self.browser_zoom(pane, ZoomChange::In),
            Message::BrowserZoomOut(pane) => self.browser_zoom(pane, ZoomChange::Out),
            Message::BrowserZoomReset(pane) => self.browser_zoom(pane, ZoomChange::Reset),
```

with helper + enum (place near the other browser helpers):

```rust
enum ZoomChange { In, Out, Reset }
```

```rust
    /// Step or reset a browser pane's zoom and apply it to the webview.
    fn browser_zoom(&mut self, pane: pane_grid::Pane, change: ZoomChange) {
        let tab_id = self.active_tab().id;
        let pane_id = webview_key(tab_id, pane);
        let tab = self.active_tab_mut();
        if let Some(Block::Browser { state }) = tab.panes.get_mut(pane) {
            state.zoom = match change {
                ZoomChange::In => (state.zoom * 1.1).min(5.0),
                ZoomChange::Out => (state.zoom / 1.1).max(0.25),
                ZoomChange::Reset => 1.0,
            };
            webview_manager::set_zoom(pane_id, state.zoom);
        }
    }
```

(If the surrounding `update` match arms return `Task<Message>` from every arm, make the three zoom arms `{ self.browser_zoom(...); }` blocks that fall through to the shared tail — mirror `Message::BrowserUrlChanged`'s arm shape.)

Also: in `ensure_browser_webviews` and `Message::OpenBrowser`, after webview creation apply persisted zoom:

```rust
            if let Some(Block::Browser { state }) = ... /* the pane's block */ {
                if (state.zoom - 1.0).abs() > f64::EPSILON {
                    webview_manager::set_zoom(webview_key(tab_id, pane), state.zoom);
                }
            }
```

In `ensure_browser_webviews`, extend the collected tuple to `(tab_id, pane, url, zoom)` (read `state.zoom` where `state.url` is read) and apply after `create_browser_webview_for`.

- [ ] **Step 6: Verify**

Run: `cargo check -p alterm && cargo test --workspace`
Expected: clean; all tests pass.

Run the app briefly (`cargo run` — NEVER kill other running alterm instances) and check: opening a browser pane lands on the internal History page; visiting a site records it (restart-independent: `cat ~/.local/share/alterm/history.jsonl`).

- [ ] **Step 7: Commit**

```bash
git add alterm/src/main.rs alterm/Cargo.toml Cargo.lock
git commit -m "feat(alterm): wire global history, smart URL bar, internal start page"
```

---

### Task 8: Keyboard shortcuts (iced-side + in-page IPC)

**Files:**
- Modify: `alterm/src/main.rs` (`Message::KeyboardInput` handler ~line 1959; `handle_browser_shortcut_ipc` placeholder from Task 7)

**Interfaces:**
- Consumes: Message variants from Task 7, `widget_focus` + URL-input WidgetId pattern (`format!("browser-url-input-{:?}", pane)`, see `Message::OpenBrowser`), the shortcut IPC contract from Task 6.
- Produces: `fn browser_shortcut_message(action: &str, pane: pane_grid::Pane) -> Option<Message>` (pure mapping, unit-testable if placed in workspace crate — keep in main.rs, tested manually).

- [ ] **Step 1: iced-side shortcuts**

In `Message::KeyboardInput`, after the rename/search/palette early-outs and BEFORE the `match_shortcut` registry dispatch (~line 2018), add:

```rust
                // Browser-pane shortcuts (when a browser pane is focused and
                // iced owns the keyboard, e.g. after clicking the chrome).
                if let Some(focused) = self.active_tab().focus {
                    let is_browser = self
                        .active_tab()
                        .panes
                        .get(focused)
                        .is_some_and(|b| b.is_browser());
                    if is_browser {
                        let alt = modifiers.alt() && !modifiers.control();
                        let ctrl = modifiers.control() && !modifiers.alt() && !modifiers.shift();
                        let msg = match &key {
                            Key::Named(Named::ArrowLeft) if alt => {
                                Some(Message::BrowserBack(focused))
                            }
                            Key::Named(Named::ArrowRight) if alt => {
                                Some(Message::BrowserForward(focused))
                            }
                            Key::Character(c) if ctrl => match c.as_str() {
                                "l" => None, // handled below: focus needs a Task
                                "r" => Some(Message::BrowserReload(focused)),
                                "h" => Some(Message::BrowserOpenHistory(focused)),
                                "d" => Some(Message::BrowserToggleBookmark(focused)),
                                "=" | "+" => Some(Message::BrowserZoomIn(focused)),
                                "-" => Some(Message::BrowserZoomOut(focused)),
                                "0" => Some(Message::BrowserZoomReset(focused)),
                                _ => None,
                            },
                            _ => None,
                        };
                        if let Some(msg) = msg {
                            return self.update(msg);
                        }
                        if ctrl && matches!(&key, Key::Character(c) if c.as_str() == "l") {
                            return widget_focus(WidgetId::from(
                                format!("browser-url-input-{:?}", focused),
                            ));
                        }
                    }
                }
```

(`Ctrl+F` is added in Task 10 together with the find bar.)

- [ ] **Step 2: Implement the IPC shortcut router**

Replace the Task 7 placeholder:

```rust
    /// Route a shortcut forwarded from inside a webview page.
    fn handle_browser_shortcut_ipc(&mut self, pane_id: u64, action: &str) -> Task<Message> {
        let Some((tab_id, pane)) = self.find_browser_pane(pane_id) else {
            return Task::none();
        };
        // Shortcuts act on the pane they came from; switch focus if needed.
        if self.tabs.get(self.active_tab).map(|t| t.id) != Some(tab_id) {
            return Task::none(); // stale event from a hidden tab's webview
        }
        match action {
            "back" => self.update(Message::BrowserBack(pane)),
            "forward" => self.update(Message::BrowserForward(pane)),
            "reload" => self.update(Message::BrowserReload(pane)),
            "history" => self.update(Message::BrowserOpenHistory(pane)),
            "bookmark" => self.update(Message::BrowserToggleBookmark(pane)),
            "zoom-in" => self.update(Message::BrowserZoomIn(pane)),
            "zoom-out" => self.update(Message::BrowserZoomOut(pane)),
            "zoom-reset" => self.update(Message::BrowserZoomReset(pane)),
            "focus-url" => widget_focus(WidgetId::from(
                format!("browser-url-input-{:?}", pane),
            )),
            "find" => Task::none(), // wired in Task 10
            other => {
                log::warn!("browser ipc: unknown shortcut {other:?}");
                Task::none()
            }
        }
    }
```

- [ ] **Step 3: Verify**

Run: `cargo check -p alterm && cargo test --workspace`
Expected: clean.

Manual: in a browser pane, click the page then press `Alt+Left`/`Alt+Right`, `Ctrl+H`, `Ctrl+D`, `Ctrl+L`, `Ctrl+=`/`Ctrl+-`/`Ctrl+0` — each should act on that pane (these travel the IPC path). Click the URL bar (iced focus) and repeat `Ctrl+H` — the iced path.

- [ ] **Step 4: Commit**

```bash
git add alterm/src/main.rs
git commit -m "feat(alterm): browser keyboard shortcuts via iced and in-page IPC paths"
```

---

### Task 9: Nav bar UI — star, history button, stop/reload swap, loading spinner

**Files:**
- Modify: `alterm/src/main.rs` (`browser_view` ~line 3325; App struct for spinner frame; `Message::Tick` handler)

**Interfaces:**
- Consumes: `Message::{BrowserToggleBookmark, BrowserOpenHistory, BrowserStop}` (Task 7), `browser::history::with_stores` (Task 2), `state.loading` (accurate since Task 7's load events).
- Produces: nav bar rendering consumed by Task 10 (find row appended below it).

- [ ] **Step 1: Spinner frame counter**

Add to the App struct (near other UI state fields): `spinner_frame: usize,` (init `0` in the constructor). In the `Message::Tick` handler add:

```rust
                self.spinner_frame = self.spinner_frame.wrapping_add(1);
```

- [ ] **Step 2: Extend `browser_view`**

`browser_view` needs the spinner frame; change the signature and the call site (search `browser_view(` in the pane-content match):

```rust
fn browser_view<'a>(
    pane: pane_grid::Pane,
    state: &'a BrowserState,
    spinner_frame: usize,
) -> Element<'a, Message> {
```

Inside, replace the reload button block with a stop/reload swap:

```rust
    let reload_btn = if state.loading {
        button(text("\u{2715}").size(14).center()) // ✕ stop
            .on_press(Message::BrowserStop(pane))
            .padding(Padding::from([4, 8]))
            .style(|theme: &Theme, status: button::Status| nav_button_style(theme, status))
    } else {
        button(text("\u{21BB}").size(14).center()) // ↻ reload
            .on_press(Message::BrowserReload(pane))
            .padding(Padding::from([4, 8]))
            .style(|theme: &Theme, status: button::Status| nav_button_style(theme, status))
    };
```

After the `url_input` definition, add star + history buttons and the spinner:

```rust
    let bookmarked = browser::history::with_stores(|s| s.bookmarks.is_bookmarked(&state.url))
        .unwrap_or(false);
    let star_label = if bookmarked { "\u{2605}" } else { "\u{2606}" }; // ★ / ☆
    let mut star_btn = button(text(star_label).size(14).center())
        .padding(Padding::from([4, 8]))
        .style(|theme: &Theme, status: button::Status| nav_button_style(theme, status));
    if !state.url.starts_with("alterm://") {
        star_btn = star_btn.on_press(Message::BrowserToggleBookmark(pane));
    }

    let history_btn = button(text("\u{1F553}").size(14).center()) // 🕓
        .on_press(Message::BrowserOpenHistory(pane))
        .padding(Padding::from([4, 8]))
        .style(|theme: &Theme, status: button::Status| nav_button_style(theme, status));

    const SPINNER: [&str; 8] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧"];
    let spinner: Element<'a, Message> = if state.loading {
        text(SPINNER[spinner_frame % SPINNER.len()])
            .size(13)
            .into()
    } else {
        iced::widget::space().width(Length::Fixed(0.0)).into()
    };
```

and change the nav bar row to:

```rust
        row![back_btn, fwd_btn, reload_btn, url_input, spinner, star_btn, history_btn]
```

(If the `🕓` glyph renders as tofu in the app's font, use `text("\u{231B}")` (⌛) or the string "Hist" — check visually in Step 3.)

- [ ] **Step 3: Verify**

Run: `cargo check -p alterm`, then run the app: spinner animates during a slow page load, ✕ appears while loading and stops the load, ★ fills after bookmarking (and the bookmark shows on `alterm://history`), 🕓 opens the history page. The pane title bar shows the real page title (`Browser — Example Domain`).

- [ ] **Step 4: Commit**

```bash
git add alterm/src/main.rs
git commit -m "feat(alterm): browser nav bar star/history/stop buttons and loading spinner"
```

---

### Task 10: Find-in-page bar

**Files:**
- Modify: `alterm/src/main.rs`:
  - App struct: `browser_find: Option<BrowserFindState>`
  - `Message` enum: find variants
  - `browser_view` (find row), the three webview-bounds computations (~lines 655, 725, 889) and `resize_all_panes`
  - `Message::KeyboardInput` (Ctrl+F & find-bar keys), `handle_browser_shortcut_ipc` ("find" arm), Tick drain for find events

**Interfaces:**
- Consumes: `webview_manager::{find_start, find_next, find_prev, find_finish, drain_find_events}` (Task 6), nav bar from Task 9.
- Produces:

```rust
/// Active find-in-page session for one browser pane.
struct BrowserFindState {
    tab_id: u64,
    pane: pane_grid::Pane,
    query: String,
    /// Total matches reported by webkit (None until the first count arrives).
    matches: Option<u32>,
}
```

```rust
    BrowserFindOpen(pane_grid::Pane),
    BrowserFindChanged(String),
    BrowserFindNext,
    BrowserFindPrev,
    BrowserFindClose,
```

- Produces constant: `const BROWSER_FIND_BAR_HEIGHT: f32 = 36.0;` (next to `BROWSER_NAV_BAR_HEIGHT`, ~line 182).

- [ ] **Step 1: State, messages, chrome-height helper**

Add the struct, message variants, constant, and `browser_find: None` field init. Add the helper next to `create_browser_webview`:

```rust
    /// Total chrome height (nav bar + optional find bar) above a browser
    /// pane's webview.
    fn browser_chrome_height(&self, tab_id: u64, pane: pane_grid::Pane) -> f32 {
        let find = self
            .browser_find
            .as_ref()
            .is_some_and(|f| f.tab_id == tab_id && f.pane == pane);
        BROWSER_NAV_BAR_HEIGHT + if find { BROWSER_FIND_BAR_HEIGHT } else { 0.0 }
    }
```

In each of the three webview-bounds computations (resize path ~line 655, `create_browser_webview` ~line 725, `create_browser_webview_for` ~line 889), replace the constant with the helper — e.g.:

```rust
            let chrome = self.browser_chrome_height(tab_id, pane);
            let wv_y = (TAB_BAR_HEIGHT + GRID_PADDING + rect.y + PANE_TITLE_BAR_HEIGHT + chrome) as f64;
            let wv_h = (rect.height - PANE_TITLE_BAR_HEIGHT - chrome).max(10.0) as f64;
```

(in `create_browser_webview` the tab id is `self.active_tab().id`; the fallback-bounds branches use `BROWSER_NAV_BAR_HEIGHT` unchanged — a freshly created webview never has a find bar open.)

- [ ] **Step 2: Message handlers**

Next to the other Browser arms:

```rust
            Message::BrowserFindOpen(pane) => {
                let tab_id = self.active_tab().id;
                self.browser_find = Some(BrowserFindState {
                    tab_id, pane, query: String::new(), matches: None,
                });
                self.resize_all_panes();
                return widget_focus(WidgetId::from(format!("browser-find-input-{:?}", pane)));
            }
            Message::BrowserFindChanged(q) => {
                if let Some(f) = self.browser_find.as_mut() {
                    f.query = q;
                    f.matches = None;
                    let pane_id = webview_key(f.tab_id, f.pane);
                    if f.query.is_empty() {
                        webview_manager::find_finish(pane_id);
                    } else {
                        webview_manager::find_start(pane_id, &f.query);
                    }
                }
            }
            Message::BrowserFindNext => {
                if let Some(f) = self.browser_find.as_ref() {
                    webview_manager::find_next(webview_key(f.tab_id, f.pane));
                }
            }
            Message::BrowserFindPrev => {
                if let Some(f) = self.browser_find.as_ref() {
                    webview_manager::find_prev(webview_key(f.tab_id, f.pane));
                }
            }
            Message::BrowserFindClose => {
                if let Some(f) = self.browser_find.take() {
                    webview_manager::find_finish(webview_key(f.tab_id, f.pane));
                }
                self.resize_all_panes();
            }
```

In the Tick drain (`apply_browser_webview_events`), add:

```rust
        for (pane_id, count) in webview_manager::drain_find_events() {
            if let Some(f) = self.browser_find.as_mut() {
                if webview_key(f.tab_id, f.pane) == pane_id {
                    f.matches = Some(count);
                }
            }
        }
```

- [ ] **Step 3: Find bar UI**

`browser_view` gains a parameter `find: Option<&'a BrowserFindState>` (pass `self.browser_find.as_ref().filter(|f| f.tab_id == active_tab_id && f.pane == pane)` from the call site). After the nav bar, when `find` is `Some(f)`:

```rust
    let find_bar: Option<Element<'a, Message>> = find.map(|f| {
        let count_label = match f.matches {
            Some(0) => "No matches".to_string(),
            Some(n) => format!("{n} matches"),
            None => String::new(),
        };
        container(
            row![
                text_input("Find in page...", &f.query)
                    .on_input(Message::BrowserFindChanged)
                    .on_submit(Message::BrowserFindNext)
                    .size(13)
                    .padding(Padding::from([4, 10]))
                    .width(Length::Fixed(260.0))
                    .id(WidgetId::from(format!("browser-find-input-{:?}", pane))),
                button(text("\u{25B2}").size(12).center())
                    .on_press(Message::BrowserFindPrev)
                    .padding(Padding::from([4, 8]))
                    .style(|t: &Theme, s: button::Status| nav_button_style(t, s)),
                button(text("\u{25BC}").size(12).center())
                    .on_press(Message::BrowserFindNext)
                    .padding(Padding::from([4, 8]))
                    .style(|t: &Theme, s: button::Status| nav_button_style(t, s)),
                text(count_label).size(12),
                iced::widget::space().width(Fill),
                button(text("\u{2715}").size(12).center())
                    .on_press(Message::BrowserFindClose)
                    .padding(Padding::from([4, 8]))
                    .style(|t: &Theme, s: button::Status| nav_button_style(t, s)),
            ]
            .spacing(6)
            .align_y(iced::Alignment::Center),
        )
        .width(Fill)
        .padding(Padding::from([4, 8]))
        .style(browser_chrome_style)
        .into()
    });
```

Extract the nav bar's inline container-style closure into a shared free function (place next to `nav_button_style`) and use it for both rows:

```rust
/// Background/border style shared by the browser nav bar and find bar rows.
fn browser_chrome_style(theme: &Theme) -> iced::widget::container::Style {
    let light = is_light_theme(theme);
    iced::widget::container::Style {
        background: Some(Background::Color(if light {
            Color::from_rgb(0.92, 0.92, 0.94)
        } else {
            Color::from_rgb(0.08, 0.08, 0.11)
        })),
        border: Border {
            color: if light {
                Color::from_rgb(0.80, 0.80, 0.85)
            } else {
                Color::from_rgb(0.15, 0.15, 0.20)
            },
            width: 0.0,
            radius: 0.0.into(),
        },
        ..Default::default()
    }
}
```

and the final layout becomes:

```rust
    let mut chrome = column![nav_bar];
    if let Some(fb) = find_bar {
        chrome = chrome.push(fb);
    }
    container(chrome.push(webview_area))
```

(Reuse the nav bar's container-style closure by extracting it into `fn browser_chrome_style(theme: &Theme) -> iced::widget::container::Style` and using it for both rows.)

- [ ] **Step 4: Shortcuts**

- iced path: in the Task 8 browser-shortcut block, add to the `Key::Character(c) if ctrl` match: `"f" => Some(Message::BrowserFindOpen(focused)),`
- Escape while find bar open — add an early-out near the rename/search Escape handling in `KeyboardInput` (before the browser block):

```rust
                if self.browser_find.is_some() {
                    match &key {
                        Key::Named(Named::Escape) => return self.update(Message::BrowserFindClose),
                        Key::Named(Named::Enter) if modifiers.shift() => {
                            return self.update(Message::BrowserFindPrev)
                        }
                        _ => {}
                    }
                }
```

(Plain Enter is handled by the find input's `on_submit`.)
- IPC path: replace the `"find" => Task::none(),` arm from Task 8 with `"find" => self.update(Message::BrowserFindOpen(pane)),`
- Also close the find bar when its pane/tab goes away: in the `ClosePane`/tab-close handlers, or simplest, defensively in the Tick drain:

```rust
        if let Some(f) = self.browser_find.as_ref() {
            let alive = self
                .tabs
                .iter()
                .find(|t| t.id == f.tab_id)
                .and_then(|t| t.panes.get(f.pane))
                .is_some_and(|b| b.is_browser());
            if !alive {
                self.browser_find = None;
            }
        }
```

- [ ] **Step 5: Verify**

Run: `cargo check -p alterm && cargo test --workspace && cargo clippy --workspace`

Manual: `Ctrl+F` in a browser pane (both from chrome focus and from inside the page) opens the bar and the webview shifts down (no overlap); typing highlights matches and shows a count; Enter/▼ next, Shift+Enter/▲ previous; Escape/✕ closes, clears highlights, and the webview reclaims the row.

- [ ] **Step 6: Commit**

```bash
git add alterm/src/main.rs
git commit -m "feat(alterm): find-in-page bar for browser panes"
```

---

### Task 11: End-to-end verification & wrap-up

**Files:**
- No new code; fixes only if verification fails.

- [ ] **Step 1: Full automated pass**

Run: `cargo test --workspace && cargo clippy --workspace -- -D warnings && cargo build --release`
Expected: all green. (If the repo doesn't normally enforce `-D warnings`, drop it.)

- [ ] **Step 2: Manual end-to-end checklist**

Launch a fresh instance (do NOT kill existing alterm instances — the user works inside alterm):

1. New browser pane → opens the internal History page (start page).
2. Type `rust lifetimes` in the URL bar → DuckDuckGo results. Type `docs.rs` → navigates directly. Type `localhost:1234` → attempts `https://localhost:1234`.
3. Visit 3–4 sites; pane title bar shows real page titles; spinner animates while loading; ✕ stops a slow load.
4. 🕓 → history page lists visits with titles, grouped under "Today", newest first. Search box filters. Delete (✕) removes an entry. Clear history works (with confirm).
5. Click a history entry → navigates there. Alt+Left returns to the history page.
6. ★ bookmarks a page (star fills); bookmark appears on History and Bookmarks pages; remove works; Ctrl+D toggles.
7. **Two browser panes at once** → both render internal pages correctly (custom-protocol registration is shared, not duplicated — the Task 6 WebContext note).
8. Ctrl+F find bar: matches highlight, count shows, next/prev cycle, Escape closes; webview visibly shifts down/up without overlapping chrome.
9. Ctrl+= / Ctrl+- / Ctrl+0 zoom; restart alterm → session restores tabs/panes AND zoom level; history survives restart (`~/.local/share/alterm/history.jsonl` exists and greps clean).
10. Old-session compatibility was covered by the serde-default test; visually confirm restore of a pre-existing session file still works.

- [ ] **Step 3: Update the design doc status**

In `docs/superpowers/specs/2026-07-08-browser-history-design.md`, change `**Status:**` to `Implemented`. Commit:

```bash
git add docs/superpowers/specs/2026-07-08-browser-history-design.md
git commit -m "docs(alterm): mark browser build-out spec implemented"
```

- [ ] **Step 4: Finish the branch**

Use superpowers:finishing-a-development-branch — run the full suite once more, then offer merge of `feature/browser-history-buildout` into `main`.

Post-merge follow-ups (not part of this plan): update the alterm website state (per project memory, the Astro site must track real app state) and consider the cross-tab webview keying tech-debt item.
