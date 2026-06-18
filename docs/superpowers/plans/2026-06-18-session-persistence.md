# Session Persistence Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Save the full workspace (window size, tabs, exact pane layout, and per-pane content) on a 30s timer and on close, and restore it on launch.

**Architecture:** A new pure `session` module in the `workspace` crate holds a serde model mirroring app state (`SessionState → TabState → PaneNode → BlockState`), plus capture (live → model), restore (model → `Vec<Tab>`), and atomic file I/O with corruption backup. `alterm/src/main.rs` wires startup restore, a periodic-save subscription, and a save-on-close hook. Built in three shippable phases: A (foundation + non-terminal panes), B (terminal cwd), C (terminal styled scrollback).

**Tech Stack:** Rust, iced 0.14 (`pane_grid::{State, Configuration, Node, Axis}`), `alacritty_terminal` (grid/cells), `portable_pty` (`CommandBuilder::cwd`, `Child::process_id`), `serde` + `serde_json`.

## Global Constraints

- Session file: `~/.config/alterm/session.json` (JSON). Use `alterm_config::AppConfig::config_dir()` for the directory.
- `version: u32` field in `SessionState`; current schema version is `1`. On version mismatch or any load/parse error, back up the file to `session.json.bak` and start fresh — launch must never fail because of session state.
- Writes are atomic: write `session.json.tmp`, then `rename` over `session.json`.
- Scrollback persist cap: `SCROLLBACK_PERSIST_LINES = 1000` lines per terminal, styled (ANSI SGR preserved).
- Save timing: periodic every 30s + on clean close (`exit_on_close_request(false)` → `CloseRequested` → save → `iced::exit()`). No per-keystroke saving.
- Config toggle: `[session] restore` (bool, default `true`), added to `alterm_config::AppConfig` (the top-level config struct, package `alterm-config`, imported as `alterm_config`) with `#[serde(default)]` so existing `config.toml` files still parse. `AppConfig` already has a manual `Default` impl and methods `config_dir()`/`config_path()`/`load()`. The `workspace` crate already depends on `alterm-config`.
- cwd capture is Linux-only (`/proc/<pid>/cwd`); other platforms return `None` and restore in the default directory.
- Exact-layout restore: rebuild the saved `pane_grid` tree (`PaneNode`) with its ratios — do NOT use the auto-grid builder from the window-placement feature.
- `iced::pane_grid::Axis` is not serde-serializable — use a local `SerAxis` enum and convert.
- TDD: failing test first; commit after each green task.

---

## Phase A — Foundation + non-terminal panes

### Task 1: `SessionConfig` toggle in the config crate

**Files:**
- Modify: `crates/config/src/lib.rs`
- Test: `crates/config/src/lib.rs` (`#[cfg(test)]`)

**Interfaces:**
- Produces: `pub struct SessionConfig { pub restore: bool }`; `AppConfig` gains `pub session: SessionConfig`.

(The config tests live in `crates/config/src/lib.rs`'s existing `#[cfg(test)] mod tests` with `use super::*;` — `AppConfig` is in scope.)

- [ ] **Step 1: Write the failing test**

Add to the `#[cfg(test)] mod tests` in `crates/config/src/lib.rs` (create the module if absent):

```rust
    #[test]
    fn session_defaults_to_restore_true_when_absent() {
        // A config TOML with no [session] section must still parse, restore = true.
        let toml = r#"
            [general]
            [ai]
            default_provider = "openai"
            max_tokens = 1024
            temperature = 0.7
            system_prompt = ""
            [ai.providers]
            [appearance]
            font_size = 14.0
            font_family = "monospace"
            theme = "dark"
            [terminal]
            scrollback_lines = 10000
            cursor_blink = true
            copy_on_select = false
        "#;
        let cfg: AppConfig = toml::from_str(toml).expect("parse");
        assert!(cfg.session.restore);
    }

    #[test]
    fn session_restore_can_be_disabled() {
        let toml = r#"
            [general]
            [ai]
            default_provider = "openai"
            max_tokens = 1024
            temperature = 0.7
            system_prompt = ""
            [ai.providers]
            [appearance]
            font_size = 14.0
            font_family = "monospace"
            theme = "dark"
            [terminal]
            scrollback_lines = 10000
            cursor_blink = true
            copy_on_select = false
            [session]
            restore = false
        "#;
        let cfg: AppConfig = toml::from_str(toml).expect("parse");
        assert!(!cfg.session.restore);
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p alterm-config session_`
Expected: FAIL (no field `session`).

- [ ] **Step 3: Write minimal implementation**

In `crates/config/src/lib.rs`, add the field to `AppConfig` (the top-level struct with `general/ai/appearance/terminal`):

```rust
    #[serde(default)]
    pub session: SessionConfig,
```

Add the new struct and its default near the other config structs:

```rust
fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionConfig {
    /// Restore the previous session on launch.
    #[serde(default = "default_true")]
    pub restore: bool,
}

impl Default for SessionConfig {
    fn default() -> Self {
        SessionConfig { restore: true }
    }
}
```

`AppConfig` has a manual `Default` impl (in this file) — add `session: SessionConfig::default()` to it. Also append a `[session]` block with `restore = true` to `config/default.toml`.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p alterm-config session_`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/config/src/lib.rs config/default.toml
git commit -m "feat(config): add [session] restore toggle (default on)"
```

---

### Task 2: Session serde model + tree conversion

**Files:**
- Create: `crates/workspace/src/session.rs`
- Modify: `crates/workspace/src/lib.rs` (`pub mod session;`), `crates/workspace/src/ai_chat.rs` (derive serde on `DisplayMessage`), `crates/workspace/Cargo.toml` (add `serde`, `serde_json`)
- Test: `crates/workspace/src/session.rs` (`#[cfg(test)]`)

**Interfaces:**
- Produces:
  - `SessionState`, `WindowState`, `TabState`, `PaneNode`, `BlockState`, `SerAxis` (all serde).
  - `pub const SESSION_VERSION: u32 = 1;`
  - `pub fn build_configuration(node: &PaneNode, make_leaf: &mut dyn FnMut(&BlockState) -> Block) -> pane_grid::Configuration<Block>`
  - `pub fn capture_pane_node(state: &pane_grid::State<Block>, capture_leaf: &mut dyn FnMut(&Block) -> BlockState) -> PaneNode`

- [ ] **Step 1: Add dependencies and derive serde on DisplayMessage**

In `crates/workspace/Cargo.toml` `[dependencies]` add (use workspace deps):
```toml
serde = { workspace = true }
serde_json = { workspace = true }
```
In `crates/workspace/src/ai_chat.rs`, add `use serde::{Serialize, Deserialize};` and put `#[derive(Debug, Clone, Serialize, Deserialize)]` on `DisplayMessage`.
In `crates/workspace/src/lib.rs` add `pub mod session;` with the other modules.

- [ ] **Step 2: Write the failing test**

Create `crates/workspace/src/session.rs`:

```rust
//! Serializable session model + capture/restore for persistence.

use std::path::PathBuf;

use iced::widget::pane_grid::{self, Configuration};
use serde::{Deserialize, Serialize};

use crate::ai_chat::DisplayMessage;
use crate::Block;

pub const SESSION_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SerAxis {
    Horizontal,
    Vertical,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum BlockState {
    Terminal { cwd: Option<PathBuf>, scrollback_ansi: String, rows: u16, cols: u16 },
    Browser { url: String, history: Vec<String>, history_index: usize },
    AiChat { provider: String, model: String, messages: Vec<DisplayMessage>, input: String },
    Preview { path: PathBuf },
    Settings,
    HotkeyInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum PaneNode {
    Split { axis: SerAxis, ratio: f32, a: Box<PaneNode>, b: Box<PaneNode> },
    Leaf(BlockState),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WindowState { pub width: f32, pub height: f32 }

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TabState {
    pub title: String,
    pub focus: Option<usize>,
    pub maximized: Option<usize>,
    pub layout: PaneNode,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SessionState {
    pub version: u32,
    pub window: WindowState,
    pub active_tab: usize,
    pub tabs: Vec<TabState>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> SessionState {
        SessionState {
            version: SESSION_VERSION,
            window: WindowState { width: 900.0, height: 600.0 },
            active_tab: 1,
            tabs: vec![
                TabState {
                    title: "one".into(), focus: Some(0), maximized: None,
                    layout: PaneNode::Leaf(BlockState::Preview { path: "/tmp".into() }),
                },
                TabState {
                    title: "two".into(), focus: Some(1), maximized: Some(0),
                    layout: PaneNode::Split {
                        axis: SerAxis::Vertical, ratio: 0.5,
                        a: Box::new(PaneNode::Leaf(BlockState::Browser {
                            url: "https://example.com".into(),
                            history: vec!["https://example.com".into()], history_index: 0,
                        })),
                        b: Box::new(PaneNode::Leaf(BlockState::AiChat {
                            provider: "openai".into(), model: "gpt-4o".into(),
                            messages: vec![], input: "hi".into(),
                        })),
                    },
                },
            ],
        }
    }

    #[test]
    fn session_state_json_round_trip() {
        let s = sample();
        let json = serde_json::to_string(&s).unwrap();
        let back: SessionState = serde_json::from_str(&json).unwrap();
        assert_eq!(s, back);
    }

    #[test]
    fn pane_node_round_trips_through_configuration_preserving_structure() {
        // Leaf -> bare Pane; Split preserves axis + ratio + structure.
        let node = PaneNode::Split {
            axis: SerAxis::Horizontal, ratio: 0.25,
            a: Box::new(PaneNode::Leaf(BlockState::Settings)),
            b: Box::new(PaneNode::Leaf(BlockState::HotkeyInfo)),
        };
        let mut make = |_bs: &BlockState| Block::new_hotkey_info();
        let cfg = build_configuration(&node, &mut make);
        match cfg {
            Configuration::Split { axis, ratio, .. } => {
                assert_eq!(axis, pane_grid::Axis::Horizontal);
                assert!((ratio - 0.25).abs() < 1e-6);
            }
            _ => panic!("expected split"),
        }
    }
}
```

- [ ] **Step 3: Run test to verify it fails**

Run: `cargo test -p workspace session::`
Expected: FAIL (`build_configuration` not found).

- [ ] **Step 4: Write minimal implementation**

Add to `session.rs` (above the tests):

```rust
impl From<&SerAxis> for pane_grid::Axis {
    fn from(a: &SerAxis) -> Self {
        match a {
            SerAxis::Horizontal => pane_grid::Axis::Horizontal,
            SerAxis::Vertical => pane_grid::Axis::Vertical,
        }
    }
}

fn axis_to_ser(axis: pane_grid::Axis) -> SerAxis {
    match axis {
        pane_grid::Axis::Horizontal => SerAxis::Horizontal,
        pane_grid::Axis::Vertical => SerAxis::Vertical,
    }
}

/// Build an iced pane_grid Configuration from a saved PaneNode tree.
pub fn build_configuration(
    node: &PaneNode,
    make_leaf: &mut dyn FnMut(&BlockState) -> Block,
) -> Configuration<Block> {
    match node {
        PaneNode::Leaf(bs) => Configuration::Pane(make_leaf(bs)),
        PaneNode::Split { axis, ratio, a, b } => Configuration::Split {
            axis: axis.into(),
            ratio: *ratio,
            a: Box::new(build_configuration(a, make_leaf)),
            b: Box::new(build_configuration(b, make_leaf)),
        },
    }
}

/// Capture a PaneNode tree from a live pane_grid State.
pub fn capture_pane_node(
    state: &pane_grid::State<Block>,
    capture_leaf: &mut dyn FnMut(&Block) -> BlockState,
) -> PaneNode {
    capture_node(state.layout(), state, capture_leaf)
}

fn capture_node(
    node: &pane_grid::Node,
    state: &pane_grid::State<Block>,
    capture_leaf: &mut dyn FnMut(&Block) -> BlockState,
) -> PaneNode {
    match node {
        pane_grid::Node::Pane(pane) => {
            let block = state.get(*pane).expect("layout pane exists");
            PaneNode::Leaf(capture_leaf(block))
        }
        pane_grid::Node::Split { axis, ratio, a, b, .. } => PaneNode::Split {
            axis: axis_to_ser(*axis),
            ratio: *ratio,
            a: Box::new(capture_node(a, state, capture_leaf)),
            b: Box::new(capture_node(b, state, capture_leaf)),
        },
    }
}
```

- [ ] **Step 5: Run test to verify it passes**

Run: `cargo test -p workspace session::`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/workspace/src/session.rs crates/workspace/src/lib.rs crates/workspace/src/ai_chat.rs crates/workspace/Cargo.toml
git commit -m "feat(workspace): session serde model + pane-tree conversion"
```

---

### Task 3: Atomic file I/O + corruption backup

**Files:**
- Modify: `crates/workspace/src/session.rs`
- Test: `crates/workspace/src/session.rs` (`#[cfg(test)]`)

**Interfaces:**
- Consumes: `SessionState`, `SESSION_VERSION` (Task 2).
- Produces:
  - `pub fn session_path() -> PathBuf`
  - `pub fn save_to_path(state: &SessionState, path: &Path) -> std::io::Result<()>`
  - `pub fn load_from_path(path: &Path) -> Option<SessionState>` (returns `None` on missing/parse-error/version-mismatch, backing up a bad file to `<path>.bak`)

- [ ] **Step 1: Write the failing test**

Add imports at top of `session.rs`: `use std::path::Path;` (PathBuf already imported). Add tests:

```rust
    #[test]
    fn save_then_load_round_trips() {
        let dir = std::env::temp_dir().join(format!("alterm-sess-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("session.json");
        let s = sample();
        save_to_path(&s, &path).unwrap();
        let back = load_from_path(&path).expect("loadable");
        assert_eq!(s, back);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn corrupt_file_returns_none_and_backs_up() {
        let dir = std::env::temp_dir().join(format!("alterm-sess-bad-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("session.json");
        std::fs::write(&path, b"{ not valid json").unwrap();
        assert!(load_from_path(&path).is_none());
        assert!(dir.join("session.json.bak").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn version_mismatch_returns_none() {
        let dir = std::env::temp_dir().join(format!("alterm-sess-ver-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("session.json");
        let mut s = sample();
        s.version = 999;
        save_to_path(&s, &path).unwrap();
        assert!(load_from_path(&path).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p workspace session::tests::save`
Expected: FAIL (`save_to_path` not found).

- [ ] **Step 3: Write minimal implementation**

Add to `session.rs`:

```rust
/// Path to the session file: `<config_dir>/session.json`.
pub fn session_path() -> PathBuf {
    alterm_config::AppConfig::config_dir().join("session.json")
}

/// Write the session atomically (temp file + rename).
pub fn save_to_path(state: &SessionState, path: &Path) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(state)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, json.as_bytes())?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

/// Load the session. Returns `None` (and backs up the file to `*.bak`) on a
/// missing file, parse error, or version mismatch.
pub fn load_from_path(path: &Path) -> Option<SessionState> {
    let bytes = std::fs::read(path).ok()?;
    match serde_json::from_slice::<SessionState>(&bytes) {
        Ok(state) if state.version == SESSION_VERSION => Some(state),
        _ => {
            let bak = path.with_extension("json.bak");
            let _ = std::fs::rename(path, &bak);
            None
        }
    }
}
```

The `workspace` crate already depends on `alterm-config` (`alterm-config = { workspace = true }` in `crates/workspace/Cargo.toml`) — no Cargo change needed. Reference the config type as `alterm_config::AppConfig` (this is how `block.rs`/`settings_panel.rs` already use it).

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p workspace session::`
Expected: PASS (all session tests).

- [ ] **Step 5: Commit**

```bash
git add crates/workspace/src/session.rs crates/workspace/Cargo.toml
git commit -m "feat(workspace): atomic session file I/O with corruption backup"
```

---

### Task 4: Capture non-terminal blocks (and terminal stub)

**Files:**
- Modify: `crates/workspace/src/session.rs`, `crates/workspace/src/block.rs`
- Test: `crates/workspace/src/session.rs` (`#[cfg(test)]`)

**Interfaces:**
- Consumes: `BlockState` (Task 2), `capture_pane_node` (Task 2).
- Produces:
  - `Block::to_block_state(&self) -> BlockState` (in `block.rs`)
  - `pub fn capture(tabs: &[Tab], active_tab: usize, window: WindowState) -> SessionState`

- [ ] **Step 1: Write the failing test**

Add to `session.rs` tests:

```rust
    use crate::Tab;

    #[test]
    fn capture_reads_browser_and_preview_blocks() {
        // Build a tab whose single pane is a browser, then capture.
        let mut tab = Tab::new().unwrap();
        // Replace the single pane's block with a browser.
        let pane = *tab.panes.iter().next().unwrap().0;
        *tab.panes.get_mut(pane).unwrap() = Block::new_browser("https://example.com");
        let session = capture(&[tab], 0, WindowState { width: 800.0, height: 600.0 });
        assert_eq!(session.version, SESSION_VERSION);
        assert_eq!(session.tabs.len(), 1);
        match &session.tabs[0].layout {
            PaneNode::Leaf(BlockState::Browser { url, .. }) => {
                assert!(url.contains("example.com"));
            }
            other => panic!("expected browser leaf, got {other:?}"),
        }
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p workspace capture_reads_browser`
Expected: FAIL (`capture` / `to_block_state` not found).

- [ ] **Step 3: Write minimal implementation**

In `crates/workspace/src/block.rs`, add a method on `impl Block` (uses `crate::session::BlockState`):

```rust
    /// Snapshot this block's restorable state for session persistence.
    pub fn to_block_state(&self) -> crate::session::BlockState {
        use crate::session::BlockState;
        match self {
            Block::Terminal { state, .. } => BlockState::Terminal {
                cwd: None,                      // filled in Phase B
                scrollback_ansi: String::new(), // filled in Phase C
                rows: state.rows() as u16,
                cols: state.cols() as u16,
            },
            Block::Browser { state } => BlockState::Browser {
                url: state.url.clone(),
                history: state.history.clone(),
                history_index: state.history_index,
            },
            Block::AIChat { state } => BlockState::AiChat {
                provider: state.provider_name.clone(),
                model: state.model_name.clone(),
                messages: state.messages.clone(),
                input: state.input.clone(),
            },
            Block::Preview { state } => BlockState::Preview { path: state.path.clone() },
            Block::Settings { .. } => BlockState::Settings,
            Block::HotkeyInfo => BlockState::HotkeyInfo,
        }
    }
```

In `session.rs`, add `capture`, using `crate::grid::panes_in_spatial_order` for focus/maximized indices:

```rust
use crate::grid::panes_in_spatial_order;
use crate::Tab;

/// Snapshot all tabs into a SessionState.
pub fn capture(tabs: &[Tab], active_tab: usize, window: WindowState) -> SessionState {
    let tab_states = tabs.iter().map(capture_tab).collect();
    SessionState { version: SESSION_VERSION, window, active_tab, tabs: tab_states }
}

fn capture_tab(tab: &Tab) -> TabState {
    let order = panes_in_spatial_order(&tab.panes);
    let index_of = |p: pane_grid::Pane| order.iter().position(|q| *q == p);
    let focus = tab.focus.and_then(index_of);
    let maximized = tab.panes.maximized().and_then(index_of);
    let mut capture_leaf = |block: &Block| block.to_block_state();
    let layout = capture_pane_node(&tab.panes, &mut capture_leaf);
    TabState { title: tab.title.clone(), focus, maximized, layout }
}
```

(`Tab` fields `title`, `panes`, `focus` are already public; `capture_pane_node` is `pub` from Task 2.)

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p workspace session::`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/workspace/src/session.rs crates/workspace/src/block.rs
git commit -m "feat(workspace): capture tabs/panes into SessionState"
```

---

### Task 5: Restore tabs from a SessionState

**Files:**
- Modify: `crates/workspace/src/session.rs`, `crates/workspace/src/block.rs`
- Test: `crates/workspace/src/session.rs` (`#[cfg(test)]`)

**Interfaces:**
- Consumes: `build_configuration` (Task 2), `BlockState`, `SessionState` (Task 2/4), `panes_in_spatial_order` (Task 4 import).
- Produces:
  - `Block::from_state(bs: &BlockState, config: &alterm_config::AppConfig) -> Block` (in `block.rs`) — Phase A spawns a default terminal for `Terminal` (cwd/scrollback added in B/C). Config is threaded through to rebuild `Settings` panes.
  - `pub struct RestoredSession { pub tabs: Vec<Tab>, pub active_tab: usize, pub window: WindowState }`
  - `pub fn restore(state: SessionState) -> RestoredSession`

- [ ] **Step 1: Write the failing test**

Add to `session.rs` tests:

```rust
    #[test]
    fn restore_rebuilds_tabs_and_focus() {
        let s = SessionState {
            version: SESSION_VERSION,
            window: WindowState { width: 1000.0, height: 700.0 },
            active_tab: 0,
            tabs: vec![TabState {
                title: "restored".into(),
                focus: Some(1),
                maximized: None,
                layout: PaneNode::Split {
                    axis: SerAxis::Vertical, ratio: 0.5,
                    a: Box::new(PaneNode::Leaf(BlockState::Preview { path: "/tmp".into() })),
                    b: Box::new(PaneNode::Leaf(BlockState::HotkeyInfo)),
                },
            }],
        };
        let restored = restore(s, &alterm_config::AppConfig::default());
        assert_eq!(restored.active_tab, 0);
        assert_eq!(restored.window.width, 1000.0);
        assert_eq!(restored.tabs.len(), 1);
        let tab = &restored.tabs[0];
        assert_eq!(tab.title, "restored");
        assert_eq!(tab.panes.len(), 2);
        // focus index 1 maps to the second pane in spatial order.
        let order = crate::grid::panes_in_spatial_order(&tab.panes);
        assert_eq!(tab.focus, Some(order[1]));
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p workspace restore_rebuilds`
Expected: FAIL (`restore` / `from_state` not found).

- [ ] **Step 3: Write minimal implementation**

In `block.rs`, add (Phase A terminal = default spawn; B/C extend this):

```rust
    /// Reconstruct a block from its persisted state. `config` is needed to
    /// rebuild a `Settings` pane.
    pub fn from_state(bs: &crate::session::BlockState, config: &alterm_config::AppConfig) -> Block {
        use crate::session::BlockState;
        match bs {
            BlockState::Terminal { rows, cols, .. } => {
                Block::new_terminal((*rows).max(1), (*cols).max(1))
                    .unwrap_or_else(|_| Block::new_hotkey_info())
            }
            BlockState::Browser { url, history, history_index } => {
                let mut block = Block::new_browser(url);
                if let Block::Browser { state } = &mut block {
                    if !history.is_empty() {
                        state.history = history.clone();
                        state.history_index = (*history_index).min(history.len() - 1);
                    }
                }
                block
            }
            BlockState::AiChat { provider, model, messages, input } => {
                let mut block = Block::new_ai_chat(provider.clone(), model.clone());
                if let Block::AIChat { state } = &mut block {
                    state.messages = messages.clone();
                    state.input = input.clone();
                }
                block
            }
            BlockState::Preview { path } => Block::new_preview(&path.to_string_lossy()),
            BlockState::Settings => Block::new_settings(config.clone()),
            BlockState::HotkeyInfo => Block::new_hotkey_info(),
        }
    }
```

(`Block::new_settings` takes an owned `alterm_config::AppConfig`, hence `config.clone()`.)

In `session.rs`, add:

```rust
pub struct RestoredSession {
    pub tabs: Vec<Tab>,
    pub active_tab: usize,
    pub window: WindowState,
}

/// Rebuild live tabs from a SessionState. `config` is needed to reconstruct
/// Settings panes.
pub fn restore(state: SessionState, config: &alterm_config::AppConfig) -> RestoredSession {
    let tabs = state.tabs.into_iter().map(|ts| restore_tab(ts, config)).collect();
    RestoredSession { tabs, active_tab: state.active_tab, window: state.window }
}

fn restore_tab(ts: TabState, config: &alterm_config::AppConfig) -> Tab {
    let mut make_leaf = |bs: &BlockState| Block::from_state(bs, config);
    let cfg = build_configuration(&ts.layout, &mut make_leaf);
    let panes = pane_grid::State::with_configuration(cfg);

    let order = panes_in_spatial_order(&panes);
    let focus = ts.focus.and_then(|i| order.get(i).copied());

    let mut tab = Tab::from_parts(ts.title, panes, focus);
    if let Some(i) = ts.maximized {
        if let Some(p) = order.get(i).copied() {
            tab.panes.maximize(p);
        }
    }
    tab
}
```

Add a constructor to `crates/workspace/src/tab.rs` so `session` can build a `Tab` with an existing state (the `id` is assigned fresh from the counter):

```rust
    /// Build a Tab from already-constructed parts (used by session restore).
    pub fn from_parts(title: String, panes: pane_grid::State<Block>, focus: Option<pane_grid::Pane>) -> Self {
        Tab {
            id: NEXT_TAB_ID.fetch_add(1, Ordering::Relaxed),
            title,
            panes,
            focus,
        }
    }
```

The Step-1 test already calls `restore(s, &alterm_config::AppConfig::default())`. `AppConfig` implements `Default` (manual impl in the config crate), so no extra construction is needed. The `session.rs` test module needs `use alterm_config::AppConfig;` (or reference it fully-qualified, as the test does).

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p workspace session::`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/workspace/src/session.rs crates/workspace/src/block.rs crates/workspace/src/tab.rs
git commit -m "feat(workspace): restore tabs/panes from SessionState"
```

---

### Task 6: Lifecycle wiring in the app (startup, periodic, close)

**Files:**
- Modify: `alterm/src/main.rs`

**Interfaces:**
- Consumes: `workspace::session::{capture, restore, load_from_path, save_to_path, session_path, WindowState, SessionState}` (Tasks 2–5).

This is integration: no cargo unit test (needs the GUI). It ends with a build + `cargo test` (existing suite) + a manual smoke test.

- [ ] **Step 1: Import session APIs**

Add `session` to the `use workspace::{ ... }` items (or `use workspace::session;`). `config` and `iced` are already in scope.

- [ ] **Step 2: Startup restore in `Alterm::new`**

Find `Alterm::new` (constructs the initial `tabs`/`active_tab`/`window_width`/`window_height`). Replace the initial tab creation so that, when `config.session.restore` is true and a session loads, it rebuilds from it:

```rust
        let (tabs, active_tab, window_width, window_height) = {
            let default_session = || (vec![Tab::new().expect("default tab")], 0usize, 900.0f32, 600.0f32);
            let restored = if config.session.restore {
                session::load_from_path(&session::session_path())
                    .map(|state| session::restore(state, &config))
                    .filter(|r| !r.tabs.is_empty())
            } else {
                None
            };
            match restored {
                Some(r) => {
                    let active = r.active_tab.min(r.tabs.len() - 1); // clamp to valid range
                    (r.tabs, active, r.window.width, r.window.height)
                }
                None => default_session(),
            }
        };
```
Keep the rest of `new` (palette, etc.) intact, using these values. (If `new` returns `(Self, Task)`, after building self also arrange for browser webviews of restored browser panes — see Step 5.)

- [ ] **Step 3: Add Save/Close messages + handlers**

Add to the `Message` enum:
```rust
    SaveSession,
    WindowCloseRequested,
```
Add a helper on `impl Alterm`:
```rust
    fn save_session(&self) {
        let window = session::WindowState { width: self.window_width, height: self.window_height };
        let state = session::capture(&self.tabs, self.active_tab, window);
        if let Err(e) = session::save_to_path(&state, &session::session_path()) {
            log::warn!("Failed to save session: {e}");
        }
    }
```
Handle the messages in `update`:
```rust
            Message::SaveSession => {
                if self.config.session.restore {
                    self.save_session();
                }
            }
            Message::WindowCloseRequested => {
                if self.config.session.restore {
                    self.save_session();
                }
                return iced::exit();
            }
```

- [ ] **Step 4: Wire the subscription + close behavior**

In `subscription`, add a 30s timer to the batch:
```rust
        let save = iced::time::every(Duration::from_secs(30)).map(|_| Message::SaveSession);
```
and include `save` in `Subscription::batch([tick, events, save])`.
In the `event::listen_with` closure, add a match arm:
```rust
                    Event::Window(iced::window::Event::CloseRequested) => {
                        return Some(Message::WindowCloseRequested);
                    }
```
In the builder chain at the bottom of `main()`, add `.exit_on_close_request(false)` to the `iced::application(...)` chain (before `.run()`).

- [ ] **Step 5: Create webviews for restored browser panes**

After the app state is built in `new` (and the window/parent handle exists), browser webviews must be created for restored browser panes, mirroring `create_browser_webview`. Because webview creation needs `parent_xid` (only available after the window opens), do it lazily: in the existing `Message::Tick`/first-frame path or when `parent_xid` becomes available, for each tab and each `Block::Browser` pane lacking a webview (`!webview_manager::exists(webview_key(tab.id, pane))`), call the same creation logic with the browser's URL, then `update_webview_visibility()`. Implement a `fn ensure_browser_webviews(&mut self)` that scans all tabs and creates any missing webview using `webview_key(tab.id, pane)`, and call it once `parent_xid` is set. Use the browser's `state.url` for the URL.

- [ ] **Step 6: Build and fix compile errors**

Run: `cargo build -p alterm`
Expected: builds. Fix borrow/type issues (e.g., clamp `active_tab`, ensure `Duration` is imported — it is, used by `tick`).

- [ ] **Step 7: Run the full test suite**

Run: `cargo test`
Expected: PASS (config + workspace + existing).

- [ ] **Step 8: Manual smoke test**

Run: `cargo run -p alterm`. Open a few tabs/panes (terminal, browser to a URL, AI chat with a message, a file preview), arrange a split layout, then close the window. Relaunch: verify tabs, layout/ratios, active tab, browser URL, AI conversation, and preview path are restored. (Terminals reopen as fresh shells in the default dir for now — cwd/scrollback come in Phases B/C.) Also verify: delete `~/.config/alterm/session.json` → clean default launch; set `[session] restore = false` in config → no restore.

- [ ] **Step 9: Commit**

```bash
git add alterm/src/main.rs
git commit -m "feat(alterm): save/restore session lifecycle (startup, 30s timer, close)"
```

---

## Phase B — Terminal working directory

### Task 7: Capture & restore terminal cwd

**Files:**
- Modify: `crates/terminal/src/pty.rs`, `crates/workspace/src/block.rs`
- Test: `crates/terminal/src/pty.rs` (`#[cfg(test)]`), `crates/workspace/src/block.rs` test for `working_dir` shape

**Interfaces:**
- Produces:
  - `PtyHandle::child_pid(&self) -> Option<u32>`
  - `PtyHandle::spawn_in(rows, cols, cwd: Option<&std::path::Path>) -> Result<(Self, Receiver<TerminalEvent>), String>` (and `spawn` delegates with `None`)
  - `read_proc_cwd(pid: u32) -> Option<PathBuf>` (Linux; `None` elsewhere)
  - `Block::working_dir(&self) -> Option<PathBuf>`
  - `Block::new_terminal_in(rows, cols, cwd: Option<&Path>) -> Result<Block, String>`

- [ ] **Step 1: Write the failing test (pty)**

Add to `crates/terminal/src/pty.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(target_os = "linux")]
    #[test]
    fn read_proc_cwd_of_self_matches_current_dir() {
        let pid = std::process::id();
        let cwd = read_proc_cwd(pid).expect("own cwd readable");
        let expected = std::env::current_dir().unwrap().canonicalize().unwrap();
        assert_eq!(cwd.canonicalize().unwrap(), expected);
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p terminal read_proc_cwd`
Expected: FAIL (`read_proc_cwd` not found).

- [ ] **Step 3: Implement pty changes**

In `pty.rs`:

```rust
use std::path::{Path, PathBuf};

/// Read a process's current working directory via `/proc/<pid>/cwd` (Linux only).
pub fn read_proc_cwd(pid: u32) -> Option<PathBuf> {
    #[cfg(target_os = "linux")]
    {
        std::fs::read_link(format!("/proc/{pid}/cwd")).ok()
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = pid;
        None
    }
}
```

Refactor `spawn` into `spawn_in`, accepting an optional cwd, and set it on the command builder when present and the path exists:

```rust
    pub fn spawn(rows: u16, cols: u16) -> Result<(Self, mpsc::Receiver<TerminalEvent>), String> {
        Self::spawn_in(rows, cols, None)
    }

    pub fn spawn_in(rows: u16, cols: u16, cwd: Option<&Path>)
        -> Result<(Self, mpsc::Receiver<TerminalEvent>), String>
    {
        // ... existing body up to building `cmd` ...
        let mut cmd = CommandBuilder::new(&shell);
        cmd.env("TERM", /* unchanged */);
        cmd.env("SHELL", &shell);
        if let Some(dir) = cwd {
            if dir.is_dir() {
                cmd.cwd(dir);
            }
        }
        // ... rest unchanged ...
    }
```

Add a pid accessor:
```rust
    /// PID of the child shell, if available.
    pub fn child_pid(&self) -> Option<u32> {
        self.child.process_id()
    }
```

- [ ] **Step 4: Implement block changes**

In `block.rs`, add a cwd-aware constructor and a `working_dir` accessor. The `Block::Terminal` variant must be able to reach the `PtyHandle` — it already holds `pty: PtyHandle`. Add:

```rust
    /// Create a terminal whose shell starts in `cwd` (if given and valid).
    pub fn new_terminal_in(rows: u16, cols: u16, cwd: Option<&std::path::Path>) -> Result<Self, String> {
        let (pty, events) = PtyHandle::spawn_in(rows, cols, cwd)?;
        let state = TerminalState::new(rows as usize, cols as usize);
        let palette = AnsiPalette::default();
        Ok(Block::Terminal {
            state, pty, events, palette,
            cursor_visible: true, blink_count: 0, dirty: true, cached_grid: None,
        })
    }

    /// The terminal's current working directory, if determinable.
    pub fn working_dir(&self) -> Option<std::path::PathBuf> {
        match self {
            Block::Terminal { pty, .. } => pty.child_pid().and_then(terminal::read_proc_cwd),
            _ => None,
        }
    }
```
(Match the exact field list of the `Terminal` variant in `new_terminal` — copy it verbatim. Confirm `terminal::read_proc_cwd` is re-exported; if `terminal` crate root doesn't re-export it, use `terminal::pty::read_proc_cwd` or add a re-export.)

- [ ] **Step 5: Wire cwd into capture and restore**

In `block.rs` `to_block_state`, set the terminal cwd:
```rust
            Block::Terminal { state, .. } => BlockState::Terminal {
                cwd: self.working_dir(),
                scrollback_ansi: String::new(), // Phase C
                rows: state.rows() as u16,
                cols: state.cols() as u16,
            },
```
In `block.rs` `from_state`, spawn in the saved cwd:
```rust
            BlockState::Terminal { cwd, rows, cols, .. } => {
                let dir = cwd.as_deref();
                Block::new_terminal_in((*rows).max(1), (*cols).max(1), dir)
                    .unwrap_or_else(|_| Block::new_hotkey_info())
            }
```

- [ ] **Step 6: Run tests**

Run: `cargo test -p terminal read_proc_cwd && cargo test -p workspace && cargo build -p alterm`
Expected: PASS / builds.

- [ ] **Step 7: Manual smoke test**

`cargo run -p alterm`: in a terminal pane `cd` into a deep directory, close, relaunch → the restored terminal opens in that directory (run `pwd`).

- [ ] **Step 8: Commit**

```bash
git add crates/terminal/src/pty.rs crates/workspace/src/block.rs
git commit -m "feat(terminal): capture & restore terminal working directory"
```

---

## Phase C — Terminal styled scrollback

### Task 8: `scrollback_ansi` encoder

**Files:**
- Modify: `crates/terminal/src/term.rs`
- Test: `crates/terminal/src/term.rs` (`#[cfg(test)]`)

**Interfaces:**
- Produces: `TerminalState::scrollback_ansi(&self, max_lines: usize) -> String`

- [ ] **Step 1: Write the failing test**

Add to `term.rs`:

```rust
#[cfg(test)]
mod scrollback_tests {
    use super::*;

    #[test]
    fn scrollback_ansi_captures_text_and_caps_lines() {
        let mut t = TerminalState::new(24, 80);
        // Write red "hello", reset, newline, then "world".
        t.process_output(b"\x1b[31mhello\x1b[0m\r\nworld\r\n");
        let out = t.scrollback_ansi(1000);
        assert!(out.contains("hello"));
        assert!(out.contains("world"));
        // Contains an SGR color escape for the red text.
        assert!(out.contains("\x1b[") && out.contains("31"));
        // Line cap is respected.
        let capped = t.scrollback_ansi(1);
        assert!(capped.lines().count() <= 1 + 1); // allow trailing newline
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p terminal scrollback_ansi`
Expected: FAIL (`scrollback_ansi` not found).

- [ ] **Step 3: Implement the encoder**

Add to `term.rs`. Walk from the oldest available history line down to the last non-empty line, capped at `max_lines`. For each row, iterate columns; when a cell's fg/bg/flags differ from the previous, emit an SGR sequence; append the cell char; end each row with `\r\n`. Map alacritty `Color` to SGR using the existing pattern from `gpu-renderer` (Named → 30–37/90–97 or default `39`/`49`; Indexed(n) → `38;5;n`/`48;5;n`; Spec(rgb) → `38;2;r;g;b`/`48;2;...`). Include bold (`1`), italic (`3`), underline (`4`); reset with `\x1b[0m` at the start of each changed run and at the end.

```rust
    /// Encode up to `max_lines` of scrollback (history + visible) as ANSI text
    /// for session persistence. Preserves fg/bg colors and bold/italic/underline.
    pub fn scrollback_ansi(&self, max_lines: usize) -> String {
        use alacritty_terminal::term::cell::Flags;
        use alacritty_terminal::index::{Line, Point, Column};

        let grid = self.term.grid();
        let total = grid.total_lines();           // history + screen
        let screen = grid.screen_lines();
        // Topmost line index is -(history) ; bottom is screen-1.
        let history = total.saturating_sub(screen);
        let first = -(history as i32);
        let last = screen as i32 - 1;
        let start = (last - (max_lines as i32) + 1).max(first);

        let cols = grid.columns();
        let mut out = String::new();
        for line in start..=last {
            // (emit cells for this line, tracking style; see below)
            let mut last_style: Option<(/*fg*/ String, /*bg*/ String, bool, bool, bool)> = None;
            let mut line_buf = String::new();
            let mut last_nonblank = 0usize;
            for col in 0..cols {
                let cell = &grid[Point::new(Line(line), Column(col))];
                let fg = sgr_color(cell.fg, true);
                let bg = sgr_color(cell.bg, false);
                let bold = cell.flags.contains(Flags::BOLD);
                let italic = cell.flags.contains(Flags::ITALIC);
                let underline = cell.flags.intersects(Flags::ALL_UNDERLINES);
                let style = (fg, bg, bold, italic, underline);
                if last_style.as_ref() != Some(&style) {
                    line_buf.push_str("\x1b[0m");
                    line_buf.push_str(&sgr_seq(&style));
                    last_style = Some(style);
                }
                line_buf.push(cell.c);
                if cell.c != ' ' { last_nonblank = line_buf.len(); }
            }
            line_buf.push_str("\x1b[0m");
            // trim trailing blanks but keep at least the styled content
            out.push_str(line_buf.trim_end_matches(' '));
            out.push_str("\r\n");
        }
        out
    }
```

Add the helpers `sgr_color(color, is_fg) -> String` (returns the numeric SGR fragment, e.g. `"38;2;255;0;0"` or `"31"` or empty for default) and `sgr_seq(style) -> String` (assembles `\x1b[<codes>m` from fg/bg/bold/italic/underline). Use `alacritty_terminal::vte::ansi::Color` / `NamedColor` variants for the match (the same `Color` type returned by `cell.fg`). Keep it deterministic and avoid emitting an escape for default colors (use `39`/`49`).

(Implementer note: the exact `Color` enum path is `alacritty_terminal::vte::ansi::{Color, NamedColor}`; confirm against `gpu-renderer/src/colors.rs` which already matches on it.)

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p terminal scrollback_ansi`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/terminal/src/term.rs
git commit -m "feat(terminal): encode scrollback as ANSI for persistence"
```

---

### Task 9: Capture & replay scrollback

**Files:**
- Modify: `crates/workspace/src/block.rs`
- Test: manual (replay is visual)

**Interfaces:**
- Consumes: `TerminalState::scrollback_ansi` (Task 8); `SCROLLBACK_PERSIST_LINES`.

- [ ] **Step 1: Add the cap constant + capture scrollback**

In `block.rs`, add `pub const SCROLLBACK_PERSIST_LINES: usize = 1000;`. In `to_block_state`, fill the terminal scrollback:
```rust
            Block::Terminal { state, .. } => BlockState::Terminal {
                cwd: self.working_dir(),
                scrollback_ansi: state.scrollback_ansi(SCROLLBACK_PERSIST_LINES),
                rows: state.rows() as u16,
                cols: state.cols() as u16,
            },
```

- [ ] **Step 2: Replay scrollback on restore**

In `from_state`, after spawning the terminal, inject the saved scrollback into its `TerminalState` (which renders it as history) before the shell's prompt is processed:
```rust
            BlockState::Terminal { cwd, rows, cols, scrollback_ansi } => {
                let dir = cwd.as_deref();
                let mut block = Block::new_terminal_in((*rows).max(1), (*cols).max(1), dir)
                    .unwrap_or_else(|_| Block::new_hotkey_info());
                if !scrollback_ansi.is_empty() {
                    if let Block::Terminal { state, .. } = &mut block {
                        state.process_output(scrollback_ansi.as_bytes());
                        // Separate restored history from the live shell prompt.
                        state.process_output(b"\r\n");
                    }
                }
                block
            }
```
(`TerminalState::process_output` already exists and feeds the VTE parser, not the PTY, so this renders as on-screen history without sending anything to the shell.)

- [ ] **Step 3: Build + test**

Run: `cargo build -p alterm && cargo test`
Expected: builds, all pass.

- [ ] **Step 4: Manual smoke test**

`cargo run -p alterm`: run several commands in a terminal (e.g. `ls`, `echo` with colored output via `ls --color`), close, relaunch → the restored terminal shows the prior output (with colors) as history above a fresh prompt, and is cd'd to the right directory.

- [ ] **Step 5: Commit**

```bash
git add crates/workspace/src/block.rs
git commit -m "feat(workspace): capture & replay terminal scrollback on restore"
```

---

## Self-Review

**Spec coverage:**
- JSON `session.json` in config dir, version, atomic write, corruption backup → Task 3. ✓
- `[session] restore` toggle, default on, serde default → Task 1. ✓
- Model `SessionState/TabState/PaneNode/BlockState`, `DisplayMessage` serde, `SerAxis` → Task 2. ✓
- Exact-layout (tree+ratios) capture/restore via `PaneNode` ↔ `Configuration` → Tasks 2, 4, 5. ✓
- Browser url+history, AI provider/model/messages/input, Preview path, Settings/HotkeyInfo → Tasks 4, 5. ✓
- Window size, active tab, focus, maximized → Tasks 4, 5. ✓
- Lifecycle: startup restore, 30s periodic, close hook, `exit_on_close_request(false)` → Task 6. ✓
- Terminal cwd via `/proc`, spawn-with-cwd, graceful fallback → Task 7. ✓
- Styled scrollback capture + replay, 1000-line cap → Tasks 8, 9. ✓
- Restored browser webview creation → Task 6 Step 5. ✓
- Testing (JSON round-trip, tree conversion, corruption, scrollback encoder, cwd reader) → Tasks 2,3,5,7,8. ✓
- Phasing A/B/C → Tasks 1–6 / 7 / 8–9. ✓

**Placeholder scan:** No "TBD"/"handle edge cases" without code. The one judgment-call narration is Task 6 Step 5 (lazy browser-webview creation tied to `parent_xid` availability) — flagged explicitly with the concrete approach (`ensure_browser_webviews` scanning all tabs, keyed via `webview_key(tab.id, pane)`), not left vague. Acceptable for an integration step.

**Type consistency:** `BlockState`/`PaneNode`/`SerAxis` defined in Task 2 and used unchanged in 4/5/7/9. `from_state(bs, config)` signature settled in Task 5 (config-threaded) and reused in 7/9. `to_block_state(&self)` consistent across 4/7/9. `spawn_in`/`new_terminal_in`/`working_dir`/`child_pid`/`read_proc_cwd` names consistent across 7. `scrollback_ansi(max_lines)` consistent across 8/9. `Tab::from_parts` defined in Task 5 and used by restore.
