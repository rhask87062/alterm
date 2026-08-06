# Browser Pane Tabs Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** One browser pane can hold many websites in tabs — each tab a live native webview, switched instantly, with `_blank` links opening as new in-pane tabs.

**Architecture:** A browser pane's `Block::Browser` state becomes `BrowserPaneState { tabs: Vec<BrowserTab>, active }`, where each `BrowserTab` owns the existing per-page `BrowserState` plus a permanent globally-unique `webview_id`. That id keys every `webview_manager` call, replacing the derived `(workspace-tab << 32) | pane` keys and deleting the remap-on-layout-change machinery. The iced chrome gains a tab-strip row; only the active tab's webview is visible.

**Tech Stack:** Rust workspace (crates `browser`, `workspace`, bin `alterm`), iced 0.14, wry 0.55 + webkit2gtk (Linux-only native webviews), serde session persistence.

## Global Constraints

- Work on branch `feature/browser-pane-tabs` (already created, based on `feature/browser-popups-new-tab-links`). Never commit to `main`.
- **NEVER kill running alterm instances** (`killall alterm` etc.) — the user works inside alterm.
- Spec: `docs/superpowers/specs/2026-08-06-browser-pane-tabs-design.md`.
- Browser native code is Linux-only (`#[cfg(target_os = "linux")]`); keep the non-Linux stub module in `crates/browser/src/lib.rs` in sync with any `webview_manager` public-API change.
- Popup webviews must NEVER get a wry IPC handler (`build_popup` in webview_manager.rs — do not touch its handler wiring). Destructive IPC commands stay gated on `alterm://` origin.
- The jcodemunch index is stale (indexed 2026-07-08). Read live files before editing; do not trust indexed line numbers.
- After every task: `cargo test --workspace` passes, `cargo clippy --workspace --all-targets` introduces no NEW warnings (9 pre-existing workspace warnings in terminal/config/ai crates plus 2 doc-comment-style ones in browser are known and left alone).
- Run tests with `cargo test -p <crate>` for speed during TDD loops; the full `--workspace` run gates each commit.

---

### Task 1: `BrowserPaneState` — pure tab model in the browser crate

**Files:**
- Modify: `crates/browser/src/lib.rs` (append after `BrowserState` impl, before `resolve_input`; tests go in the existing `mod tests`)

**Interfaces:**
- Consumes: existing `BrowserState` (unchanged).
- Produces (used by Tasks 3–5):
  - `pub struct BrowserTab { pub webview_id: u64, pub state: BrowserState }`
  - `pub struct BrowserPaneState { pub tabs: Vec<BrowserTab>, pub active: usize }`
  - `pub struct TabClose { pub closed_webview_id: u64, pub pane_empty: bool }`
  - `BrowserPaneState::new(url: &str) -> Self`
  - `BrowserPaneState::from_states(states: Vec<BrowserState>, active: usize) -> Self`
  - `active_tab(&self) -> &BrowserTab`, `active_state(&self) -> &BrowserState`, `active_state_mut(&mut self) -> &mut BrowserState`, `active_webview_id(&self) -> u64`
  - `open_at_end(&mut self, url: &str) -> u64`, `open_after_active(&mut self, url: &str) -> u64` (both return the new tab's webview id and activate it)
  - `close(&mut self, idx: usize) -> Option<TabClose>`, `select(&mut self, idx: usize) -> bool`, `next(&mut self)`, `prev(&mut self)`, `index_of_webview(&self, webview_id: u64) -> Option<usize>`

- [ ] **Step 1: Write the failing tests**

Append to the existing `mod tests` in `crates/browser/src/lib.rs`:

```rust
    // ── BrowserPaneState (in-pane tabs) ─────────────────────────────

    #[test]
    fn pane_state_starts_with_one_tab() {
        let p = BrowserPaneState::new("https://a.com");
        assert_eq!(p.tabs.len(), 1);
        assert_eq!(p.active, 0);
        assert_eq!(p.active_state().url, "https://a.com");
        assert_eq!(p.active_webview_id(), p.tabs[0].webview_id);
    }

    #[test]
    fn webview_ids_are_unique() {
        let mut p = BrowserPaneState::new("https://a.com");
        let id1 = p.tabs[0].webview_id;
        let id2 = p.open_at_end("https://b.com");
        let id3 = p.open_after_active("https://c.com");
        assert!(id1 != id2 && id2 != id3 && id1 != id3);
    }

    #[test]
    fn open_at_end_appends_and_activates() {
        let mut p = BrowserPaneState::new("https://a.com");
        let id = p.open_at_end("https://b.com");
        assert_eq!(p.tabs.len(), 2);
        assert_eq!(p.active, 1);
        assert_eq!(p.active_webview_id(), id);
        assert_eq!(p.active_state().url, "https://b.com");
    }

    #[test]
    fn open_after_active_inserts_next_to_active() {
        let mut p = BrowserPaneState::new("https://a.com");
        p.open_at_end("https://b.com"); // [a, b], active=1
        p.select(0);                    // active=0
        let id = p.open_after_active("https://c.com"); // [a, c, b]
        assert_eq!(p.active, 1);
        assert_eq!(p.active_webview_id(), id);
        assert_eq!(p.tabs[2].state.url, "https://b.com");
    }

    #[test]
    fn close_left_of_active_shifts_active_down() {
        let mut p = BrowserPaneState::new("https://a.com");
        p.open_at_end("https://b.com");
        p.open_at_end("https://c.com"); // [a, b, c], active=2
        let closed = p.close(0).unwrap();
        assert!(!closed.pane_empty);
        assert_eq!(p.tabs.len(), 2);
        assert_eq!(p.active, 1);
        assert_eq!(p.active_state().url, "https://c.com");
    }

    #[test]
    fn close_active_activates_right_neighbor() {
        let mut p = BrowserPaneState::new("https://a.com");
        p.open_at_end("https://b.com");
        p.open_at_end("https://c.com"); // [a, b, c]
        p.select(1);
        let closed = p.close(1).unwrap(); // [a, c]
        assert!(!closed.pane_empty);
        assert_eq!(p.active, 1);
        assert_eq!(p.active_state().url, "https://c.com");
    }

    #[test]
    fn close_last_position_activates_new_last() {
        let mut p = BrowserPaneState::new("https://a.com");
        p.open_at_end("https://b.com"); // [a, b], active=1
        p.close(1).unwrap();
        assert_eq!(p.active, 0);
        assert_eq!(p.active_state().url, "https://a.com");
    }

    #[test]
    fn close_final_tab_reports_pane_empty_without_removing() {
        let mut p = BrowserPaneState::new("https://a.com");
        let id = p.tabs[0].webview_id;
        let closed = p.close(0).unwrap();
        assert!(closed.pane_empty);
        assert_eq!(closed.closed_webview_id, id);
        // Caller closes the whole pane; the tab list is left intact so the
        // close-pane path can destroy every remaining webview uniformly.
        assert_eq!(p.tabs.len(), 1);
    }

    #[test]
    fn close_out_of_range_is_none() {
        let mut p = BrowserPaneState::new("https://a.com");
        assert!(p.close(5).is_none());
    }

    #[test]
    fn select_reports_change_and_ignores_bad_index() {
        let mut p = BrowserPaneState::new("https://a.com");
        p.open_at_end("https://b.com"); // active=1
        assert!(p.select(0));
        assert!(!p.select(0)); // already active
        assert!(!p.select(9)); // out of range
        assert_eq!(p.active, 0);
    }

    #[test]
    fn next_and_prev_wrap_around() {
        let mut p = BrowserPaneState::new("https://a.com");
        p.open_at_end("https://b.com");
        p.open_at_end("https://c.com"); // active=2
        p.next();
        assert_eq!(p.active, 0);
        p.prev();
        assert_eq!(p.active, 2);
        p.prev();
        assert_eq!(p.active, 1);
    }

    #[test]
    fn index_of_webview_resolves_ids() {
        let mut p = BrowserPaneState::new("https://a.com");
        let id_b = p.open_at_end("https://b.com");
        assert_eq!(p.index_of_webview(id_b), Some(1));
        assert_eq!(p.index_of_webview(id_b + 999_999), None);
    }

    #[test]
    fn from_states_clamps_active_and_survives_empty() {
        let states = vec![BrowserState::new("https://a.com"), BrowserState::new("https://b.com")];
        let p = BrowserPaneState::from_states(states, 7);
        assert_eq!(p.active, 1); // clamped to last

        let empty = BrowserPaneState::from_states(Vec::new(), 0);
        assert_eq!(empty.tabs.len(), 1); // fallback tab, never zero tabs
        assert_eq!(empty.active, 0);
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p browser pane_state 2>&1 | tail -5` (and the other new test names)
Expected: FAIL to compile — `BrowserPaneState` not found.

- [ ] **Step 3: Implement the model**

In `crates/browser/src/lib.rs`, after the `impl BrowserState` block:

```rust
/// Monotonic allocator for browser-tab webview ids. Ids are unique for the
/// process lifetime and are the keys for every `webview_manager` call — they
/// never change when panes or workspace tabs move.
static NEXT_WEBVIEW_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

fn alloc_webview_id() -> u64 {
    NEXT_WEBVIEW_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

/// One tab inside a browser pane: its page state plus the id of the native
/// webview that renders it.
pub struct BrowserTab {
    /// Permanent key for this tab's native webview (allocated once).
    pub webview_id: u64,
    /// Per-page state: URL, history, zoom, loading, title.
    pub state: BrowserState,
}

impl BrowserTab {
    fn new(url: &str) -> Self {
        BrowserTab { webview_id: alloc_webview_id(), state: BrowserState::new(url) }
    }
}

/// Result of closing a tab via [`BrowserPaneState::close`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TabClose {
    /// Webview id of the closed tab — the caller destroys the native webview.
    pub closed_webview_id: u64,
    /// True when this was the pane's last tab. The tab is left in place; the
    /// caller closes the whole pane, which destroys all remaining webviews.
    pub pane_empty: bool,
}

/// All tabs of one browser pane plus which one is active. Pure logic — no
/// GTK/webview calls — so every operation is unit-testable.
pub struct BrowserPaneState {
    pub tabs: Vec<BrowserTab>,
    pub active: usize,
}

impl BrowserPaneState {
    /// A pane with a single tab at `url`.
    pub fn new(url: &str) -> Self {
        BrowserPaneState { tabs: vec![BrowserTab::new(url)], active: 0 }
    }

    /// Rebuild from restored per-tab states (session restore). Allocates
    /// fresh webview ids, clamps `active`, and falls back to a single blank
    /// tab if `states` is empty — a pane never has zero tabs.
    pub fn from_states(states: Vec<BrowserState>, active: usize) -> Self {
        let tabs: Vec<BrowserTab> = states
            .into_iter()
            .map(|state| BrowserTab { webview_id: alloc_webview_id(), state })
            .collect();
        if tabs.is_empty() {
            return BrowserPaneState::new("about:blank");
        }
        let active = active.min(tabs.len() - 1);
        BrowserPaneState { tabs, active }
    }

    pub fn active_tab(&self) -> &BrowserTab {
        &self.tabs[self.active]
    }

    pub fn active_state(&self) -> &BrowserState {
        &self.tabs[self.active].state
    }

    pub fn active_state_mut(&mut self) -> &mut BrowserState {
        &mut self.tabs[self.active].state
    }

    pub fn active_webview_id(&self) -> u64 {
        self.tabs[self.active].webview_id
    }

    /// Append a new tab at the end of the strip (Ctrl+T / `+` button) and
    /// activate it. Returns the new tab's webview id.
    pub fn open_at_end(&mut self, url: &str) -> u64 {
        let tab = BrowserTab::new(url);
        let id = tab.webview_id;
        self.tabs.push(tab);
        self.active = self.tabs.len() - 1;
        id
    }

    /// Insert a new tab right after the active one (`_blank` links) and
    /// activate it. Returns the new tab's webview id.
    pub fn open_after_active(&mut self, url: &str) -> u64 {
        let tab = BrowserTab::new(url);
        let id = tab.webview_id;
        let at = self.active + 1;
        self.tabs.insert(at, tab);
        self.active = at;
        id
    }

    /// Close the tab at `idx`. Returns `None` if out of range. When closing
    /// the last remaining tab, reports `pane_empty` WITHOUT removing it (see
    /// [`TabClose`]); otherwise removes the tab and fixes `active` so the
    /// selection stays sensible (right neighbor when closing the active tab).
    pub fn close(&mut self, idx: usize) -> Option<TabClose> {
        if idx >= self.tabs.len() {
            return None;
        }
        if self.tabs.len() == 1 {
            return Some(TabClose {
                closed_webview_id: self.tabs[0].webview_id,
                pane_empty: true,
            });
        }
        let closed = self.tabs.remove(idx);
        if idx < self.active {
            self.active -= 1;
        } else {
            self.active = self.active.min(self.tabs.len() - 1);
        }
        Some(TabClose { closed_webview_id: closed.webview_id, pane_empty: false })
    }

    /// Activate the tab at `idx`. Returns true when the active tab changed.
    pub fn select(&mut self, idx: usize) -> bool {
        if idx >= self.tabs.len() || idx == self.active {
            return false;
        }
        self.active = idx;
        true
    }

    /// Activate the next tab, wrapping past the end.
    pub fn next(&mut self) {
        self.active = (self.active + 1) % self.tabs.len();
    }

    /// Activate the previous tab, wrapping past the start.
    pub fn prev(&mut self) {
        self.active = (self.active + self.tabs.len() - 1) % self.tabs.len();
    }

    /// Which tab (index) owns `webview_id`, if any.
    pub fn index_of_webview(&self, webview_id: u64) -> Option<usize> {
        self.tabs.iter().position(|t| t.webview_id == webview_id)
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p browser 2>&1 | tail -5`
Expected: all pass (existing + 13 new).

- [ ] **Step 5: Clippy and commit**

```bash
cargo clippy -p browser --all-targets 2>&1 | tail -5   # no new warnings
git add crates/browser/src/lib.rs
git commit -m "feat(browser): BrowserPaneState — pure in-pane tab model"
```

---

### Task 2: Session format — per-tab persistence with legacy compatibility

**Files:**
- Modify: `crates/workspace/src/session.rs` (BlockState + new struct + tests)
- Modify: `crates/workspace/src/block.rs` (`to_block_state` / `from_state` construction sites)

**Interfaces:**
- Consumes: nothing new (Block still holds a single `BrowserState` in this task).
- Produces (used by Task 3):
  - `pub struct BrowserTabState { pub url: String, pub history: Vec<String>, pub history_index: usize, pub zoom: f64 }` (serde, in `session.rs`)
  - `BlockState::Browser` gains `#[serde(default)] tabs: Vec<BrowserTabState>` and `#[serde(default)] active_tab: usize`; the legacy flat fields remain and mirror the active tab.
  - `SESSION_VERSION` stays 1 — old files must still load (new fields default).

- [ ] **Step 1: Write the failing tests**

In `crates/workspace/src/session.rs` tests module:

```rust
    #[test]
    fn legacy_browser_block_state_decodes_with_empty_tabs() {
        // A pre-tabs session file: no `tabs`, no `active_tab`.
        let json = r#"{"Browser":{"url":"https://a.com","history":["https://a.com"],"history_index":0}}"#;
        let bs: BlockState = serde_json::from_str(json).unwrap();
        match &bs {
            BlockState::Browser { tabs, active_tab, zoom, .. } => {
                assert!(tabs.is_empty());
                assert_eq!(*active_tab, 0);
                assert_eq!(*zoom, 1.0);
            }
            _ => panic!("expected Browser"),
        }
    }

    #[test]
    fn browser_tabs_round_trip_through_json() {
        let bs = BlockState::Browser {
            url: "https://b.com".into(),
            history: vec!["https://b.com".into()],
            history_index: 0,
            zoom: 1.25,
            tabs: vec![
                BrowserTabState {
                    url: "https://a.com".into(),
                    history: vec!["https://a.com".into()],
                    history_index: 0,
                    zoom: 1.0,
                },
                BrowserTabState {
                    url: "https://b.com".into(),
                    history: vec!["https://a.com".into(), "https://b.com".into()],
                    history_index: 1,
                    zoom: 1.25,
                },
            ],
            active_tab: 1,
        };
        let json = serde_json::to_string(&bs).unwrap();
        let back: BlockState = serde_json::from_str(&json).unwrap();
        assert_eq!(bs, back);
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p workspace session 2>&1 | tail -5`
Expected: FAIL to compile — no `tabs` field / `BrowserTabState` unknown.

- [ ] **Step 3: Extend the session types**

In `crates/workspace/src/session.rs`, above `BlockState`:

```rust
/// Persisted state of one in-pane browser tab.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BrowserTabState {
    pub url: String,
    pub history: Vec<String>,
    pub history_index: usize,
    #[serde(default = "default_zoom")]
    pub zoom: f64,
}
```

Change the `Browser` variant to:

```rust
    Browser {
        // Legacy flat fields: mirror the active tab so pre-tabs binaries can
        // still read new session files (they restore the active tab only).
        url: String,
        history: Vec<String>,
        history_index: usize,
        #[serde(default = "default_zoom")]
        zoom: f64,
        /// All in-pane tabs. Empty in pre-tabs session files — restore then
        /// falls back to the legacy flat fields as a single tab.
        #[serde(default)]
        tabs: Vec<BrowserTabState>,
        #[serde(default)]
        active_tab: usize,
    },
```

Fix the existing construction sites that now miss fields:
- `session.rs` test around line 239 (`PaneNode::Leaf(BlockState::Browser { .. })`): add `tabs: Vec::new(), active_tab: 0`.
- `crates/workspace/src/block.rs` `to_block_state`:

```rust
            Block::Browser { state } => BlockState::Browser {
                url: state.url.clone(),
                history: state.history.clone(),
                history_index: state.history_index,
                zoom: state.zoom,
                tabs: vec![crate::session::BrowserTabState {
                    url: state.url.clone(),
                    history: state.history.clone(),
                    history_index: state.history_index,
                    zoom: state.zoom,
                }],
                active_tab: 0,
            },
```

- `block.rs` `from_state`: add `..` binding is not allowed on the variant match since fields are named — change the pattern to
  `BlockState::Browser { url, history, history_index, zoom, tabs: _, active_tab: _ }` and keep the existing legacy-field restore body unchanged (multi-tab restore lands in Task 3).

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p workspace 2>&1 | tail -5`
Expected: all pass, including the pre-existing `browser_block_state_zoom_defaults_when_missing` and capture tests.

- [ ] **Step 5: Full check and commit**

```bash
cargo test --workspace 2>&1 | tail -5
cargo clippy -p workspace --all-targets 2>&1 | tail -5
git add crates/workspace/src/session.rs crates/workspace/src/block.rs
git commit -m "feat(workspace): persist browser panes as tab lists (legacy-compatible)"
```

---

### Task 3: Plumbing switch — `Block::Browser` holds `BrowserPaneState`, webview keys become stored ids

Behavior after this task is IDENTICAL to today (every pane has exactly one tab, no strip yet) — but all webview addressing goes through per-tab ids and the remap machinery is gone.

**Files:**
- Modify: `crates/workspace/src/lib.rs` (re-export)
- Modify: `crates/workspace/src/block.rs` (Block variant + from_state/to_block_state/title)
- Modify: `crates/browser/src/webview_manager.rs` (delete `remap`/`remap_map` + their tests)
- Modify: `alterm/src/main.rs` (all webview keying/event sites)

**Interfaces:**
- Consumes: `BrowserPaneState` API from Task 1; `BlockState::Browser { tabs, active_tab, .. }` from Task 2.
- Produces (used by Tasks 4–5):
  - `Block::Browser { state: BrowserPaneState }`
  - main.rs free fns: `set_browser_pane_visible(block: &Block, visible: bool)`, `destroy_browser_pane_webviews(block: &Block)`
  - main.rs methods: `find_browser_tab(&self, webview_id: u64) -> Option<(u64, pane_grid::Pane, usize)>`, `browser_active_webview_id(&self, tab_id: u64, pane: pane_grid::Pane) -> Option<u64>`, `create_webview_for_tab(&self, tab_id: u64, pane: pane_grid::Pane, webview_id: u64, url: &str)`
  - DELETED: `compose_key`, `webview_key`, `webview_manager::remap`, `create_browser_webview`, `create_browser_webview_for` (the new `create_webview_for_tab` replaces both).

- [ ] **Step 1: Switch the workspace types**

`crates/workspace/src/lib.rs` line ~22:

```rust
pub use browser::{BrowserPaneState, BrowserState, BrowserTab, TabClose};
```

`crates/workspace/src/block.rs`:
- `use browser::BrowserState;` → `use browser::{BrowserPaneState, BrowserState};`
- Variant: `Browser { state: BrowserPaneState },`
- `new_browser`:

```rust
    /// Create a new browser block with a single tab navigated to `url`.
    pub fn new_browser(url: &str) -> Self {
        Block::Browser { state: BrowserPaneState::new(url) }
    }
```

- `from_state` Browser arm (full multi-tab restore):

```rust
            BlockState::Browser { url, history, history_index, zoom, tabs, active_tab } => {
                let restore_one = |url: &str, history: &[String], history_index: usize, zoom: f64| {
                    let mut s = BrowserState::new(url);
                    if !history.is_empty() {
                        s.history = history.to_vec();
                        s.history_index = history_index.min(history.len() - 1);
                    }
                    s.zoom = zoom;
                    s
                };
                let states: Vec<BrowserState> = if tabs.is_empty() {
                    vec![restore_one(url, history, *history_index, *zoom)]
                } else {
                    tabs.iter()
                        .map(|t| restore_one(&t.url, &t.history, t.history_index, t.zoom))
                        .collect()
                };
                Block::Browser { state: BrowserPaneState::from_states(states, *active_tab) }
            }
```

- `to_block_state` Browser arm:

```rust
            Block::Browser { state } => {
                let active = state.active_state();
                BlockState::Browser {
                    url: active.url.clone(),
                    history: active.history.clone(),
                    history_index: active.history_index,
                    zoom: active.zoom,
                    tabs: state
                        .tabs
                        .iter()
                        .map(|t| crate::session::BrowserTabState {
                            url: t.state.url.clone(),
                            history: t.state.history.clone(),
                            history_index: t.state.history_index,
                            zoom: t.state.zoom,
                        })
                        .collect(),
                    active_tab: state.active,
                }
            }
```

- `title()` Browser arm: `format!("Browser — {}", state.active_state().display_title())`

- [ ] **Step 2: Delete the remap machinery in webview_manager**

In `crates/browser/src/webview_manager.rs` delete: `pub fn remap`, `fn remap_map`, and the three tests `remap_moves_values_to_new_keys`, `remap_handles_swaps_without_clobbering`, `remap_ignores_missing_and_identity`; drop `remap_map` from the test-module `use`. (Keys are now permanent per-tab ids — nothing ever needs re-keying.)

- [ ] **Step 3: Rewire main.rs — helpers**

Delete `compose_key` and `webview_key` (main.rs:202–216). If `pane_to_id` becomes dead, delete it too (clippy will say). Add in their place:

```rust
/// Show or hide a browser pane's native webviews. Hiding hides every tab's
/// webview; showing shows only the active tab's (background tabs stay hidden).
/// No-op for non-browser blocks.
fn set_browser_pane_visible(block: &Block, visible: bool) {
    if let Block::Browser { state } = block {
        for (i, t) in state.tabs.iter().enumerate() {
            webview_manager::set_visible(t.webview_id, visible && i == state.active);
        }
    }
}

/// Destroy every native webview owned by a block (all of a browser pane's
/// tabs). No-op for non-browser blocks.
fn destroy_browser_pane_webviews(block: &Block) {
    if let Block::Browser { state } = block {
        for t in &state.tabs {
            webview_manager::destroy(t.webview_id);
        }
    }
}
```

Add methods on `Alterm` (near the old `find_browser_pane`, which they replace):

```rust
    /// Resolve a webview id to its (workspace tab id, pane, in-pane tab index).
    fn find_browser_tab(&self, webview_id: u64) -> Option<(u64, pane_grid::Pane, usize)> {
        for tab in &self.tabs {
            for (pane, block) in tab.panes.iter() {
                if let Block::Browser { state } = block {
                    if let Some(idx) = state.index_of_webview(webview_id) {
                        return Some((tab.id, *pane, idx));
                    }
                }
            }
        }
        None
    }

    /// The webview id of a browser pane's active tab, if the pane exists.
    fn browser_active_webview_id(&self, tab_id: u64, pane: pane_grid::Pane) -> Option<u64> {
        self.tabs
            .iter()
            .find(|t| t.id == tab_id)
            .and_then(|t| t.panes.get(pane))
            .and_then(|b| match b {
                Block::Browser { state } => Some(state.active_webview_id()),
                _ => None,
            })
    }
```

- [ ] **Step 4: Rewire main.rs — webview creation**

Replace BOTH `create_browser_webview` (main.rs:765) and `create_browser_webview_for` (main.rs:1134) with one method:

```rust
    /// Create the native webview for one browser tab. Bounds come from the
    /// pane's layout region when available; panes outside the active
    /// workspace tab get fallback bounds that resize_all_panes corrects when
    /// their tab is selected.
    fn create_webview_for_tab(&self, tab_id: u64, pane: pane_grid::Pane, webview_id: u64, url: &str) {
        let Some(xid) = self.parent_xid else {
            log::warn!("Cannot create webview: parent XID not yet available");
            return;
        };

        use iced::Size;
        let grid_width = (self.window_width - SIDEBAR_WIDTH).max(80.0);
        let grid_height = (self.window_height - TAB_BAR_HEIGHT).max(40.0);
        let bounds = Size::new(
            (grid_width - GRID_PADDING * 2.0).max(40.0),
            (grid_height - GRID_PADDING * 2.0).max(40.0),
        );
        let regions = self
            .tabs
            .iter()
            .find(|t| t.id == tab_id)
            .map(|t| t.panes.layout().pane_regions(PANE_GRID_SPACING, PANE_GRID_MIN_SIZE, bounds));

        let chrome = self.browser_chrome_height(tab_id, pane);
        let (x, y, w, h) = match regions.as_ref().and_then(|r| r.get(&pane)) {
            Some(rect) => (
                (GRID_PADDING + rect.x) as f64,
                (TAB_BAR_HEIGHT + GRID_PADDING + rect.y + PANE_TITLE_BAR_HEIGHT + chrome) as f64,
                rect.width as f64,
                (rect.height - PANE_TITLE_BAR_HEIGHT - chrome).max(10.0) as f64,
            ),
            None => (0.0, (TAB_BAR_HEIGHT + PANE_TITLE_BAR_HEIGHT + BROWSER_NAV_BAR_HEIGHT) as f64, 600.0, 400.0),
        };

        if let Err(e) = webview_manager::create_webview(webview_id, xid, url, (x, y, w, h)) {
            log::error!("Failed to create webview {webview_id}: {e}");
        }
    }
```

Callers:
- `Message::OpenBrowser` (main.rs:2071):

```rust
            Message::OpenBrowser => {
                let url = "alterm://history";
                let block = Block::new_browser(url);
                let new_pane = self.add_window(block);
                let tab_id = self.active_tab().id;
                if let Some(Block::Browser { state }) = self.active_tab().panes.get(new_pane) {
                    self.create_webview_for_tab(tab_id, new_pane, state.active_webview_id(), url);
                }
                webview_manager::pump_gtk_events();
                self.resize_all_panes();
                return widget_focus(WidgetId::from(
                    format!("browser-url-input-{:?}", new_pane),
                ));
            }
```

(The old "apply persisted zoom" block goes away — a fresh pane is always zoom 1.0.)
- `open_url_in_new_tab` (main.rs:810): same replacement inside — build the block, then look up `state.active_webview_id()` for `create_webview_for_tab(tab_id, pane, id, url)`; drop its zoom block too. (This whole method is replaced by in-pane opening in Task 4; here it just has to compile with the new types.)
- `ensure_browser_webviews` (main.rs:1181): replace the collection with explicit loops over every tab of every browser pane:

```rust
        let mut missing: Vec<(u64, pane_grid::Pane, u64, String, f64)> = Vec::new();
        for tab in &self.tabs {
            for (pane, block) in tab.panes.iter() {
                if let Block::Browser { state } = block {
                    for t in &state.tabs {
                        if !webview_manager::exists(t.webview_id) {
                            missing.push((tab.id, *pane, t.webview_id, t.state.url.clone(), t.state.zoom));
                        }
                    }
                }
            }
        }
        let created_any = !missing.is_empty();
        for (tab_id, pane, webview_id, url, zoom) in missing {
            self.create_webview_for_tab(tab_id, pane, webview_id, &url);
            if (zoom - 1.0).abs() > f64::EPSILON {
                webview_manager::set_zoom(webview_id, zoom);
            }
        }
        self.update_webview_visibility();
```

Keep the function's existing tail (the `created_any`-gated re-derive of bounds) unchanged.
- `add_window` (main.rs:731): delete the `remap_ids` block and the `webview_manager::remap(&remap_ids)` call; `info.remap` is no longer used (bind the result as before, just don't read `.remap` — or `let _ = info.remap;` if clippy complains about an unused field read pattern).

- [ ] **Step 5: Rewire main.rs — visibility, resize, destroy**

- `update_webview_visibility` (main.rs:830):

```rust
    /// Show webviews in the active tab, hide webviews in all other tabs.
    /// Within a visible browser pane only the active in-pane tab is shown.
    fn update_webview_visibility(&self) {
        for (tab_idx, tab) in self.tabs.iter().enumerate() {
            let is_active = tab_idx == self.active_tab;
            for (_pane, block) in tab.panes.iter() {
                set_browser_pane_visible(block, is_active);
            }
        }
    }
```

- `resize_all_panes` (main.rs:635): in the "another pane is maximized" arm (line ~678) replace the `is_browser`/`set_visible` pair with `set_browser_pane_visible(block, false);`. Replace the `block.is_browser()` bounds section (lines 702–721) with:

```rust
                if let Block::Browser { state } = block {
                    let find_active = find_key == Some((tab_id, *pane));
                    let chrome = BROWSER_NAV_BAR_HEIGHT
                        + if find_active { BROWSER_FIND_BAR_HEIGHT } else { 0.0 };
                    let wv_x = (GRID_PADDING + rect.x) as f64;
                    let wv_y = (TAB_BAR_HEIGHT + GRID_PADDING + rect.y + PANE_TITLE_BAR_HEIGHT + chrome) as f64;
                    let wv_w = rect.width as f64;
                    let wv_h = (rect.height - PANE_TITLE_BAR_HEIGHT - chrome).max(10.0) as f64;
                    for (i, t) in state.tabs.iter().enumerate() {
                        if i == state.active {
                            webview_manager::set_bounds(t.webview_id, wv_x, wv_y, wv_w, wv_h);
                            webview_manager::set_visible(t.webview_id, true);
                        } else {
                            webview_manager::set_visible(t.webview_id, false);
                        }
                    }
                }
```

(Note: the loop body uses `tab.panes.get_mut`; the browser arm only reads — pattern-match on the `&mut Block` as `Block::Browser { state }` works for both.)
- `Message::PaneDragged(Picked)` (main.rs:1440): loop body → `set_browser_pane_visible(block, false);` (drop the `is_browser` check).
- `MaximizeToggle` (main.rs:1488) and `MaximizeTogglePane` (main.rs:1537): restore arms → `set_browser_pane_visible(block, true);` for every pane; maximize arms → `set_browser_pane_visible(block, false);` for every pane `!= focused` / `!= pane`.
- `Message::ClosePane` (main.rs:1473) and `Message::ClosePaneId` (main.rs:1520): replace `webview_manager::destroy(webview_key(...))` with:

```rust
                if let Some(block) = tab.panes.get(focused) {
                    destroy_browser_pane_webviews(block);
                }
```

(for `ClosePaneId`, look up `pane` on the active tab before the mutable operations, e.g. `if let Some(block) = self.active_tab().panes.get(pane) { destroy_browser_pane_webviews(block); }`).
- `Message::CloseTab` (main.rs:1575): replace the destroy loop body with `destroy_browser_pane_webviews(block);` (drop the `is_browser` check).

- [ ] **Step 6: Rewire main.rs — events, IPC, and per-pane message handlers**

- `apply_browser_nav_events` (main.rs:848):

```rust
    /// Drain navigation events reported by the webviews and apply each to
    /// the owning in-pane tab's state (which may be a background tab).
    fn apply_browser_nav_events(&mut self) {
        for (webview_id, url) in webview_manager::drain_nav_events() {
            let Some((tab_id, pane, idx)) = self.find_browser_tab(webview_id) else {
                continue; // stale event from an already-closed tab
            };
            if let Some(tab) = self.tabs.iter_mut().find(|t| t.id == tab_id) {
                if let Some(Block::Browser { state }) = tab.panes.get_mut(pane) {
                    if let Some(t) = state.tabs.get_mut(idx) {
                        let fresh = t.state.on_navigation(&url);
                        if fresh && !url.starts_with("alterm://") && !url.starts_with("about:") {
                            browser::history::with_stores(|s| {
                                s.history.record_visit(&url, browser::history::now_ts());
                            });
                        }
                    }
                }
            }
        }
    }
```

- Delete `find_browser_pane` (main.rs:877). In `apply_browser_webview_events` (main.rs:891) the title and load drains become:

```rust
        for (webview_id, title) in webview_manager::drain_title_events() {
            let Some((tab_id, pane, idx)) = self.find_browser_tab(webview_id) else {
                continue;
            };
            if let Some(tab) = self.tabs.iter_mut().find(|t| t.id == tab_id) {
                if let Some(Block::Browser { state }) = tab.panes.get_mut(pane) {
                    if let Some(t) = state.tabs.get_mut(idx) {
                        t.state.title = title.clone();
                        if !t.state.url.starts_with("alterm://") {
                            let url = t.state.url.clone();
                            browser::history::with_stores(|s| {
                                s.history.set_title(&url, &title);
                            });
                        }
                    }
                }
            }
        }

        for (webview_id, started) in webview_manager::drain_load_events() {
            let Some((tab_id, pane, idx)) = self.find_browser_tab(webview_id) else {
                continue;
            };
            if let Some(tab) = self.tabs.iter_mut().find(|t| t.id == tab_id) {
                if let Some(Block::Browser { state }) = tab.panes.get_mut(pane) {
                    if let Some(t) = state.tabs.get_mut(idx) {
                        t.state.set_loading(started);
                    }
                }
            }
        }
```

The find drain becomes:

```rust
        for (webview_id, count) in webview_manager::drain_find_events() {
            let target = self
                .browser_find
                .as_ref()
                .and_then(|f| self.browser_active_webview_id(f.tab_id, f.pane));
            if target == Some(webview_id) {
                if let Some(f) = self.browser_find.as_mut() {
                    f.matches = Some(count);
                }
            }
        }
```

- `handle_browser_ipc` (main.rs:956): rename the first parameter `pane_id` → `webview_id` (its `webview_manager::reload(webview_id)` calls already work — ids key the same map).
- `handle_browser_shortcut_ipc` (main.rs:1007): head becomes

```rust
    fn handle_browser_shortcut_ipc(&mut self, webview_id: u64, action: &str) -> Task<Message> {
        let Some((tab_id, pane, _idx)) = self.find_browser_tab(webview_id) else {
            return Task::none();
        };
        if self.tabs.get(self.active_tab).map(|t| t.id) != Some(tab_id) {
            return Task::none(); // stale event from a hidden workspace tab
        }
        // Only the pane's active tab has a visible webview; drop anything else.
        if self.browser_active_webview_id(tab_id, pane) != Some(webview_id) {
            return Task::none();
        }
```

(match arms unchanged.)
- Nav-bar message handlers `BrowserNavigate` / `BrowserBack` / `BrowserForward` / `BrowserReload` / `BrowserStop` (main.rs:2093–2161): same shape for each — drop the `webview_key` prelude and act on the active tab, e.g.:

```rust
            Message::BrowserNavigate(pane, url) => {
                let target = browser::resolve_input(&url, &self.config.browser.search_engine);
                let tab = self.active_tab_mut();
                if let Some(Block::Browser { state }) = tab.panes.get_mut(pane) {
                    let target = state.active_state_mut().navigate(&target);
                    webview_manager::navigate(state.active_webview_id(), &target);
                }
            }
            Message::BrowserBack(pane) => {
                let tab = self.active_tab_mut();
                if let Some(Block::Browser { state }) = tab.panes.get_mut(pane) {
                    if state.active_state_mut().begin_back() {
                        webview_manager::go_back(state.active_webview_id());
                    }
                }
            }
```

(`BrowserForward` → `begin_forward`/`go_forward`; `BrowserReload` → `active_state_mut().reload()` + `webview_manager::reload(id)`; `BrowserStop` → `active_state_mut().set_loading(false)` + `webview_manager::stop(id)`.)
- `BrowserUrlChanged` / `BrowserToggleBookmark` (main.rs:2134–2148): swap `state.` for `state.active_state_mut().` (bookmark arm reads `active_state()`'s url/title).
- `browser_zoom` (main.rs:1036):

```rust
    fn browser_zoom(&mut self, pane: pane_grid::Pane, change: ZoomChange) {
        let tab = self.active_tab_mut();
        if let Some(Block::Browser { state }) = tab.panes.get_mut(pane) {
            let zoom = {
                let s = state.active_state_mut();
                s.zoom = match change {
                    ZoomChange::In => (s.zoom * 1.1).min(5.0),
                    ZoomChange::Out => (s.zoom / 1.1).max(0.25),
                    ZoomChange::Reset => 1.0,
                };
                s.zoom
            };
            webview_manager::set_zoom(state.active_webview_id(), zoom);
        }
    }
```

(The inner block ends the mutable `active_state_mut` borrow before `active_webview_id` takes its shared borrow.)
- Find handlers (main.rs:2173–2218): replace every `webview_key(f.tab_id, f.pane)` / `webview_key(old.tab_id, old.pane)` with `self.browser_active_webview_id(...)`, guarded:

```rust
            Message::BrowserFindChanged(q) => {
                if let Some(f) = self.browser_find.as_mut() {
                    f.query = q;
                    f.matches = None;
                }
                if let Some(f) = self.browser_find.as_ref() {
                    if let Some(id) = self.browser_active_webview_id(f.tab_id, f.pane) {
                        if f.query.is_empty() {
                            webview_manager::find_finish(id);
                        } else {
                            webview_manager::find_start(id, &f.query);
                        }
                    }
                }
            }
```

(same pattern for `BrowserFindOpen`'s old-session cleanup, `BrowserFindNext`, `BrowserFindPrev`, `BrowserFindClose`.)
- View dispatch (main.rs:2704): `browser_view(pane, state.active_state(), self.spinner_frame, find)` — `browser_view` keeps its `&BrowserState` signature until Task 4.

- [ ] **Step 7: Build, test, and smoke-check**

```bash
cargo test --workspace 2>&1 | tail -5
cargo clippy --workspace --all-targets 2>&1 | tail -15   # only pre-existing warnings
cargo build --release 2>&1 | tail -3
```

Manual smoke (run `./target/release/alterm` as a NEW instance — never kill the user's): open a browser pane, navigate, back/forward, zoom, find-in-page, split a second browser pane, switch workspace tabs, maximize, close pane. Everything must behave exactly as before.

- [ ] **Step 8: Commit**

```bash
git add -A
git commit -m "refactor(alterm): browser webviews keyed by permanent per-tab ids

Block::Browser now holds BrowserPaneState (one tab so far). Deletes the
webview_key/compose_key derivation and the remap-on-layout-change machinery."
```

---

### Task 4: Tab strip UI, tab management messages, `_blank` opens in-pane

**Files:**
- Modify: `alterm/src/main.rs` (constant, messages, handlers, browser_view, `_blank` drain)

**Interfaces:**
- Consumes: everything Task 3 produced.
- Produces (used by Task 5):
  - `Message::BrowserTabSelected(pane_grid::Pane, usize)`, `Message::BrowserTabNew(pane_grid::Pane)`, `Message::BrowserTabClose(pane_grid::Pane, usize)`, `Message::BrowserTabCloseActive(pane_grid::Pane)`, `Message::BrowserTabNext(pane_grid::Pane)`, `Message::BrowserTabPrev(pane_grid::Pane)`
  - `const BROWSER_TAB_BAR_HEIGHT: f32 = 30.0;`
  - methods `open_browser_tab_in(&mut self, tab_id: u64, pane: pane_grid::Pane, url: &str, at_end: bool)`, `finish_find_for_pane(&mut self, tab_id: u64, pane: pane_grid::Pane)`, `switch_browser_tab(&mut self, tab_id: u64, pane: pane_grid::Pane, idx: usize)`

- [ ] **Step 1: Chrome height**

Next to `BROWSER_NAV_BAR_HEIGHT` (main.rs:182): `const BROWSER_TAB_BAR_HEIGHT: f32 = 30.0;`
- `browser_chrome_height` (main.rs:756): `BROWSER_TAB_BAR_HEIGHT + BROWSER_NAV_BAR_HEIGHT + if find { BROWSER_FIND_BAR_HEIGHT } else { 0.0 }`
- `resize_all_panes` browser arm (Task 3 version): `let chrome = BROWSER_TAB_BAR_HEIGHT + BROWSER_NAV_BAR_HEIGHT + if find_active { ... };`

- [ ] **Step 2: Messages and helpers**

Add the six `Message` variants (listed in Interfaces) to the `// Browser` section of the enum. Add methods:

```rust
    /// If a find session is open on this pane, finish it (webkit clears the
    /// highlights on the outgoing tab's webview) and drop the bar.
    fn finish_find_for_pane(&mut self, tab_id: u64, pane: pane_grid::Pane) {
        let matches = self
            .browser_find
            .as_ref()
            .is_some_and(|f| f.tab_id == tab_id && f.pane == pane);
        if matches {
            if let Some(id) = self.browser_active_webview_id(tab_id, pane) {
                webview_manager::find_finish(id);
            }
            self.browser_find = None;
            self.resize_all_panes(); // find bar row is gone
        }
    }

    /// Open a new in-pane browser tab and focus it. `at_end` picks strip
    /// placement: true = appended (Ctrl+T / + button), false = right after
    /// the active tab (_blank links). Works for panes in background
    /// workspace tabs (their webview stays hidden until the tab is shown).
    fn open_browser_tab_in(&mut self, tab_id: u64, pane: pane_grid::Pane, url: &str, at_end: bool) {
        self.finish_find_for_pane(tab_id, pane);
        let webview_id = {
            let Some(tab) = self.tabs.iter_mut().find(|t| t.id == tab_id) else { return };
            match tab.panes.get_mut(pane) {
                Some(Block::Browser { state }) => {
                    if at_end { state.open_at_end(url) } else { state.open_after_active(url) }
                }
                _ => return,
            }
        };
        self.create_webview_for_tab(tab_id, pane, webview_id, url);
        webview_manager::pump_gtk_events();
        self.resize_all_panes();
        self.update_webview_visibility();
    }

    /// Activate in-pane tab `idx`, closing any find session first.
    fn switch_browser_tab(&mut self, tab_id: u64, pane: pane_grid::Pane, idx: usize) {
        self.finish_find_for_pane(tab_id, pane);
        let changed = {
            let Some(tab) = self.tabs.iter_mut().find(|t| t.id == tab_id) else { return };
            match tab.panes.get_mut(pane) {
                Some(Block::Browser { state }) => state.select(idx),
                _ => false,
            }
        };
        if changed {
            self.resize_all_panes();
            self.update_webview_visibility();
        }
    }
```

- [ ] **Step 3: Message handlers**

In the `// -- Browser --` section of `update`:

```rust
            Message::BrowserTabNew(pane) => {
                let tab_id = self.active_tab().id;
                self.open_browser_tab_in(tab_id, pane, "alterm://history", true);
                return widget_focus(WidgetId::from(
                    format!("browser-url-input-{:?}", pane),
                ));
            }
            Message::BrowserTabSelected(pane, idx) => {
                let tab_id = self.active_tab().id;
                self.switch_browser_tab(tab_id, pane, idx);
            }
            Message::BrowserTabNext(pane) => {
                let tab_id = self.active_tab().id;
                self.finish_find_for_pane(tab_id, pane);
                if let Some(Block::Browser { state }) = self.active_tab_mut().panes.get_mut(pane) {
                    state.next();
                }
                self.resize_all_panes();
            }
            Message::BrowserTabPrev(pane) => {
                let tab_id = self.active_tab().id;
                self.finish_find_for_pane(tab_id, pane);
                if let Some(Block::Browser { state }) = self.active_tab_mut().panes.get_mut(pane) {
                    state.prev();
                }
                self.resize_all_panes();
            }
            Message::BrowserTabCloseActive(pane) => {
                let idx = match self.active_tab().panes.get(pane) {
                    Some(Block::Browser { state }) => state.active,
                    _ => return Task::none(),
                };
                return self.update(Message::BrowserTabClose(pane, idx));
            }
            Message::BrowserTabClose(pane, idx) => {
                let tab_id = self.active_tab().id;
                self.finish_find_for_pane(tab_id, pane);
                let result = match self.active_tab_mut().panes.get_mut(pane) {
                    Some(Block::Browser { state }) => state.close(idx),
                    _ => None,
                };
                if let Some(r) = result {
                    if r.pane_empty {
                        // Last tab: close the pane like its × button would. If
                        // it's the tab's only pane, close the workspace tab
                        // instead; if that's the last workspace tab, do nothing
                        // (same refusal as CloseTab on the final tab).
                        if self.active_tab().panes.len() > 1 {
                            return self.update(Message::ClosePaneId(pane));
                        } else if self.tabs.len() > 1 {
                            let idx = self.active_tab;
                            return self.update(Message::CloseTab(idx));
                        }
                    } else {
                        webview_manager::destroy(r.closed_webview_id);
                        self.resize_all_panes();
                    }
                }
            }
```

- [ ] **Step 4: `_blank` links open in-pane**

In `apply_browser_webview_events`, replace the new-tab drain loop with:

```rust
        // target="_blank" links open as a new tab in the pane they came from.
        for (opener, url) in webview_manager::drain_new_tab_events() {
            if let Some((tab_id, pane, _)) = self.find_browser_tab(opener) {
                self.open_browser_tab_in(tab_id, pane, &url, false);
            } else {
                log::warn!("_blank link from unknown webview {opener}; dropping {url}");
            }
        }
```

Delete `open_url_in_new_tab` (main.rs:810) — nothing calls it now.

- [ ] **Step 5: Tab strip in `browser_view`**

Change the signature and add the strip (imports: `mouse_area` is already imported at main.rs:11; add `BrowserPaneState` to the `workspace::{...}` import at main.rs:21):

```rust
/// Build the browser view for a pane: tab strip, nav bar for the active
/// tab, optional find bar, and the transparent webview placeholder.
fn browser_view<'a>(
    pane: pane_grid::Pane,
    pane_state: &'a BrowserPaneState,
    spinner_frame: usize,
    find: Option<&'a BrowserFindState>,
) -> Element<'a, Message> {
    let state = pane_state.active_state();

    // ── Tab strip ──
    let mut strip = row![].spacing(4).align_y(iced::Alignment::Center);
    for (i, t) in pane_state.tabs.iter().enumerate() {
        let active = i == pane_state.active;
        let title_btn = button(
            text(tab_strip_title(&t.state))
                .size(12)
                .wrapping(iced::widget::text::Wrapping::None),
        )
        .on_press(Message::BrowserTabSelected(pane, i))
        .padding(Padding::from([3, 8]))
        .width(Fill)
        .style(move |th: &Theme, s: button::Status| browser_tab_style(th, s, active));
        let close_btn = button(text("\u{2715}").size(10).center())
            .on_press(Message::BrowserTabClose(pane, i))
            .padding(Padding::from([3, 6]))
            .style(|th: &Theme, s: button::Status| nav_button_style(th, s));
        strip = strip.push(
            mouse_area(
                container(
                    row![title_btn, close_btn]
                        .spacing(2)
                        .align_y(iced::Alignment::Center),
                )
                .width(Length::FillPortion(1))
                .max_width(220.0),
            )
            .on_middle_press(Message::BrowserTabClose(pane, i)),
        );
    }
    strip = strip.push(
        button(text("+").size(14).center())
            .on_press(Message::BrowserTabNew(pane))
            .padding(Padding::from([2, 8]))
            .style(|th: &Theme, s: button::Status| nav_button_style(th, s)),
    );
    strip = strip.push(iced::widget::space().width(Fill));
    let tab_bar: Element<'a, Message> = container(strip)
        .width(Fill)
        .height(Length::Fixed(BROWSER_TAB_BAR_HEIGHT))
        .padding(Padding::from([3, 6]))
        .style(browser_chrome_style)
        .into();

    // ── Navigation bar ── (existing code, unchanged — reads `state`)
```

and the final layout becomes:

```rust
    let mut chrome = column![tab_bar, nav_bar];
```

Add the two helpers near `nav_button_style`:

```rust
/// Tab-strip label: the page title (or URL), truncated to fit the strip.
fn tab_strip_title(state: &BrowserState) -> String {
    let full = state.display_title();
    let mut s: String = full.chars().take(24).collect();
    if s.len() < full.len() {
        s.push('…');
    }
    s
}

/// Style for a tab-strip button; the active tab is lifted above the strip
/// with a brighter background.
fn browser_tab_style(theme: &Theme, status: button::Status, active: bool) -> button::Style {
    let mut style = nav_button_style(theme, status);
    if active {
        style.background = Some(Background::Color(if is_light_theme(theme) {
            Color::from_rgb(0.97, 0.97, 0.99)
        } else {
            Color::from_rgb(0.16, 0.16, 0.22)
        }));
    }
    style
}
```

Update the call site (main.rs:2704): `browser_view(pane, state, self.spinner_frame, find)`.

- [ ] **Step 6: Build, test, manual pass**

```bash
cargo test --workspace 2>&1 | tail -5
cargo clippy --workspace --all-targets 2>&1 | tail -15
cargo build --release 2>&1 | tail -3
```

Manual (fresh instance): open browser pane → strip shows one tab + `+`; `+` opens a second tab on the start page with URL bar focused; clicking tabs switches instantly (scroll position preserved); × and middle-click close; closing last tab closes the pane; `_blank` link (file:///tmp/alterm-popup-test.html) opens a new tab right of the active one, in-pane; per-tab zoom sticks; find bar closes on tab switch; session restore brings back all tabs with the right one active.

- [ ] **Step 7: Commit**

```bash
git add alterm/src/main.rs
git commit -m "feat(alterm): in-pane browser tabs — strip UI, open/close/switch, _blank opens in-pane"
```

---

### Task 5: Keyboard shortcuts (iced path + in-page IPC forwarder)

**Files:**
- Modify: `alterm/src/main.rs` (browser-pane key block ~2409, `handle_browser_shortcut_ipc`)
- Modify: `crates/browser/src/webview_manager.rs` (`SHORTCUT_FORWARDER`)

**Interfaces:**
- Consumes: `Message::BrowserTabNew` / `BrowserTabCloseActive` / `BrowserTabNext` / `BrowserTabPrev` from Task 4.
- Produces: forwarder actions `"tab-new"`, `"tab-close"`, `"tab-next"`, `"tab-prev"`.
- Verified free: workspace tabs use Ctrl+Shift+T/W and Ctrl+(Shift+)Tab; terminal scroll uses Shift+PageUp/Down; `match_shortcut` binds nothing to plain Ctrl+T/W/PageUp/PageDown (checked in keybindings.rs).

- [ ] **Step 1: iced-side shortcuts**

In the browser-pane shortcut block (main.rs:~2420), extend the `match &key`:

```rust
                            Key::Named(Named::PageDown) if ctrl => {
                                Some(Message::BrowserTabNext(focused))
                            }
                            Key::Named(Named::PageUp) if ctrl => {
                                Some(Message::BrowserTabPrev(focused))
                            }
```

and in the `Key::Character(c) if ctrl` inner match add:

```rust
                                "t" => Some(Message::BrowserTabNew(focused)),
                                "w" => Some(Message::BrowserTabCloseActive(focused)),
```

- [ ] **Step 2: In-page forwarder**

In `SHORTCUT_FORWARDER` (webview_manager.rs:22), inside the `e.ctrlKey && !e.altKey && !e.shiftKey` branch, add:

```js
    else if (k === 't' || k === 'T') action = 'tab-new';
    else if (k === 'w' || k === 'W') action = 'tab-close';
    else if (k === 'PageDown') action = 'tab-next';
    else if (k === 'PageUp') action = 'tab-prev';
```

- [ ] **Step 3: IPC routing**

In `handle_browser_shortcut_ipc`'s match:

```rust
            "tab-new" => self.update(Message::BrowserTabNew(pane)),
            "tab-close" => self.update(Message::BrowserTabCloseActive(pane)),
            "tab-next" => self.update(Message::BrowserTabNext(pane)),
            "tab-prev" => self.update(Message::BrowserTabPrev(pane)),
```

- [ ] **Step 4: Build, test, manual pass**

```bash
cargo test --workspace 2>&1 | tail -5
cargo clippy --workspace --all-targets 2>&1 | tail -15
cargo build --release 2>&1 | tail -3
```

Manual (fresh instance): with a browser pane focused — Ctrl+T (new tab), Ctrl+W (close tab), Ctrl+PageDown/PageUp (cycle with wraparound); repeat all four with keyboard focus INSIDE the page (click page content first — exercises the IPC path); confirm Ctrl+T/W still reach the shell in a terminal pane; confirm Ctrl+Shift+T/W still make/close workspace tabs.

- [ ] **Step 5: Commit**

```bash
git add alterm/src/main.rs crates/browser/src/webview_manager.rs
git commit -m "feat(alterm): browser tab shortcuts — Ctrl+T/W, Ctrl+PageUp/PageDown (iced + in-page)"
```

---

### Task 6: Verify, install, docs

**Files:**
- Modify: `docs/superpowers/specs/2026-08-06-browser-pane-tabs-design.md` (Status)
- Possibly modify: `website/src/data/site.ts` (only if it enumerates browser capabilities)
- Modify: memory files (outside repo)

- [ ] **Step 1: Full gate**

```bash
cargo test --workspace 2>&1 | tail -5
cargo clippy --workspace --all-targets 2>&1 | tail -15
cargo build --release 2>&1 | tail -3
```

- [ ] **Step 2: Session-compat check**

Back up `~/.config/alterm/session.json` (`cp` to the scratchpad), then from a fresh instance create a browser pane with 3 tabs, quit it, relaunch, confirm all 3 tabs restore with the correct active tab. Restore the user's original session.json afterward if the test clobbered anything they need.

- [ ] **Step 3: Install**

```bash
cargo install --path alterm 2>&1 | tail -3
```

(Replaces `~/.cargo/bin/alterm`; the user's running instance is untouched — they relaunch when ready.)

- [ ] **Step 4: Docs and site**

- Spec Status → `Implemented (<today's date>)`.
- `grep -rn "tab" website/src/data/site.ts` (and browser-feature copy) — if the site lists browser capabilities, add in-pane tabs to the entry, per the website-tracks-app-state memory (audit with jcodemunch/live read before editing).
- Update the `alterm-browser-buildout` memory: tabs implemented on `feature/browser-pane-tabs` (stacked on the popup branch), awaiting user verification; both branches merge together.

- [ ] **Step 5: Commit and hand off**

```bash
git add -A
git commit -m "docs(alterm): mark browser pane tabs spec implemented"
```

Hand the manual verification list (Task 4 Step 6 + Task 5 Step 4 + Google sign-in from a foreground AND background tab) to the user. After their confirmation, use superpowers:finishing-a-development-branch (note: this branch stacks on `feature/browser-popups-new-tab-links` — merging it brings both).
