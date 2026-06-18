# Balanced Grid Window Placement Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make every "new window" action fill the active tab as a wide-first, roughly-square balanced grid (row-major fill) instead of always splitting the focused pane to the right.

**Architecture:** A new pure `grid` module in the `workspace` crate computes grid dimensions and builds an iced `pane_grid::Configuration` tree, and rebuilds a `pane_grid::State` from an ordered list of windows. `alterm/src/main.rs` gains one `add_window` helper that all nine split sites call; it drains the existing windows, appends the new one, rebuilds the layout, re-keys any browser webviews to their new pane ids, and focuses the new window. iced's opaque pane ids change on rebuild, so a two-phase webview remap carries existing webviews across.

**Tech Stack:** Rust, iced 0.14.2 (`iced_widget::pane_grid`), wry webviews (`crates/browser`), cargo test.

## Global Constraints

- iced version is `0.14` (resolved `iced_widget-0.14.2`). Use only APIs present there: `pane_grid::State::{with_configuration, layout, get_mut, maximized, restore}`, `pane_grid::Configuration::{Split, Pane}`, `Node::pane_regions(spacing: f32, min_size: f32, bounds: Size) -> BTreeMap<Pane, Rectangle>`.
- Grid shape: `cols = ceil(sqrt(N))`, `rows = ceil(N / cols)` (**wide-first**). Fill **row-major** (left→right across a row, then top→bottom across rows).
- Layout tree: outer `Axis::Horizontal` chain stacks rows top→bottom; each row is an inner `Axis::Vertical` chain across columns left→right; even ratios.
- `Block` (in `crates/workspace/src/block.rs`) is not `Clone`; move owned `Block`s out of a `State` with `mem::replace` using `Block::HotkeyInfo` as the throwaway placeholder.
- Webviews live in `crates/browser/src/webview_manager.rs` as `thread_local! HashMap<u64, WebView>`, keyed by `pane_to_id(pane)` (parses iced's `Pane(N)` debug id, `main.rs:134`).
- TDD: write the failing test first; commit after each green task.

---

### Task 1: Grid module scaffolding + `grid_dims`

**Files:**
- Create: `crates/workspace/src/grid.rs`
- Modify: `crates/workspace/src/lib.rs` (register module + re-export)
- Test: in `crates/workspace/src/grid.rs` (`#[cfg(test)]` module)

**Interfaces:**
- Produces: `pub fn grid_dims(n: usize) -> (usize /*rows*/, usize /*cols*/)`

- [ ] **Step 1: Write the failing test**

Create `crates/workspace/src/grid.rs` with:

```rust
//! Pure layout math for the balanced auto-grid window placement.
//!
//! Wide-first: columns are added before rows so terminals keep their width.
//! Windows fill row-major (left->right across a row, then top->bottom).

/// Compute `(rows, cols)` for a wide-first balanced grid of `n` windows.
///
/// `cols = ceil(sqrt(n))`, `rows = ceil(n / cols)`. Returns `(0, 0)` for `n == 0`.
pub fn grid_dims(n: usize) -> (usize, usize) {
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grid_dims_matches_spec() {
        assert_eq!(grid_dims(0), (0, 0));
        assert_eq!(grid_dims(1), (1, 1));
        assert_eq!(grid_dims(2), (1, 2)); // side by side
        assert_eq!(grid_dims(3), (2, 2));
        assert_eq!(grid_dims(4), (2, 2));
        assert_eq!(grid_dims(5), (2, 3));
        assert_eq!(grid_dims(6), (2, 3));
        assert_eq!(grid_dims(7), (3, 3));
        assert_eq!(grid_dims(9), (3, 3));
        assert_eq!(grid_dims(10), (3, 4));
    }
}
```

Register the module in `crates/workspace/src/lib.rs` by adding `pub mod grid;` next to the other `pub mod` lines, and `pub use grid::grid_dims;` next to the other re-exports.

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p workspace grid_dims_matches_spec`
Expected: FAIL (panics in `todo!()` / not yet implemented).

- [ ] **Step 3: Write minimal implementation**

Replace the `grid_dims` body:

```rust
pub fn grid_dims(n: usize) -> (usize, usize) {
    if n == 0 {
        return (0, 0);
    }
    let cols = (n as f64).sqrt().ceil() as usize;
    let rows = (n + cols - 1) / cols; // ceil(n / cols)
    (rows, cols)
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p workspace grid_dims_matches_spec`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/workspace/src/grid.rs crates/workspace/src/lib.rs
git commit -m "feat(workspace): add grid_dims for wide-first balanced grid"
```

---

### Task 2: `build_grid_config` — build the layout tree

**Files:**
- Modify: `crates/workspace/src/grid.rs`
- Test: `crates/workspace/src/grid.rs` (`#[cfg(test)]`)

**Interfaces:**
- Consumes: `grid_dims` (Task 1)
- Produces: `pub fn build_grid_config<T>(items: Vec<T>) -> iced::widget::pane_grid::Configuration<T>`

- [ ] **Step 1: Write the failing test**

At the top of `crates/workspace/src/grid.rs` add imports:

```rust
use iced::widget::pane_grid::{self, Configuration};
```

Add the function stub below `grid_dims`:

```rust
/// Build a wide-first, row-major balanced grid `Configuration` from `items`.
///
/// Items fill left->right across a row, then top->bottom across rows, with even
/// split ratios. Panics if `items` is empty.
pub fn build_grid_config<T>(items: Vec<T>) -> Configuration<T> {
    todo!()
}

/// Combine `configs` along `axis` into one `Configuration` with even ratios,
/// as a right-leaning chain ([a, b, c] -> Split(a, Split(b, c))).
fn combine<T>(configs: Vec<Configuration<T>>, axis: pane_grid::Axis) -> Configuration<T> {
    todo!()
}
```

Add these tests inside the `tests` module (they use a helper that flattens a config back to a leaf list in spatial DFS order — `a` before `b`):

```rust
    /// Collect leaf values of a Configuration in DFS order (a before b).
    fn leaves<T: Clone>(cfg: &Configuration<T>) -> Vec<T> {
        match cfg {
            Configuration::Pane(v) => vec![v.clone()],
            Configuration::Split { a, b, .. } => {
                let mut out = leaves(a);
                out.extend(leaves(b));
                out
            }
        }
    }

    #[test]
    fn single_item_is_a_bare_pane() {
        let cfg = build_grid_config(vec![1u32]);
        assert!(matches!(cfg, Configuration::Pane(1)));
    }

    #[test]
    fn two_items_split_into_one_row() {
        // N=2 -> 1 row, 2 cols -> a single Vertical split, items left->right.
        let cfg = build_grid_config(vec![1u32, 2]);
        match cfg {
            Configuration::Split { axis, ref a, ref b, .. } => {
                assert_eq!(axis, pane_grid::Axis::Vertical);
                assert!(matches!(**a, Configuration::Pane(1)));
                assert!(matches!(**b, Configuration::Pane(2)));
            }
            _ => panic!("expected a split"),
        }
    }

    #[test]
    fn outer_split_is_horizontal_for_multi_row() {
        // N=3 -> 2 rows: outer split must be Horizontal (rows stacked).
        let cfg = build_grid_config(vec![1u32, 2, 3]);
        assert!(matches!(cfg, Configuration::Split { axis, .. } if axis == pane_grid::Axis::Horizontal));
    }

    #[test]
    fn leaf_order_is_row_major_for_n_up_to_9() {
        for n in 1..=9usize {
            let items: Vec<u32> = (0..n as u32).collect();
            let cfg = build_grid_config(items.clone());
            assert_eq!(leaves(&cfg), items, "row-major order broken for n={n}");
        }
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p workspace -- grid::tests`
Expected: FAIL (`todo!()`).

- [ ] **Step 3: Write minimal implementation**

Replace the two stubs:

```rust
pub fn build_grid_config<T>(items: Vec<T>) -> Configuration<T> {
    assert!(!items.is_empty(), "build_grid_config requires at least one item");
    let (_rows, cols) = grid_dims(items.len());

    // Chunk items into rows of up to `cols` columns; each row is a Vertical chain.
    let mut row_configs: Vec<Configuration<T>> = Vec::new();
    let mut buf: Vec<Configuration<T>> = Vec::with_capacity(cols);
    for item in items {
        buf.push(Configuration::Pane(item));
        if buf.len() == cols {
            row_configs.push(combine(std::mem::take(&mut buf), pane_grid::Axis::Vertical));
        }
    }
    if !buf.is_empty() {
        row_configs.push(combine(buf, pane_grid::Axis::Vertical));
    }

    // Stack the rows top->bottom with a Horizontal chain.
    combine(row_configs, pane_grid::Axis::Horizontal)
}

fn combine<T>(configs: Vec<Configuration<T>>, axis: pane_grid::Axis) -> Configuration<T> {
    let mut iter = configs.into_iter().rev();
    let mut acc = iter.next().expect("combine requires at least one config");
    let mut count = 1usize;
    for cfg in iter {
        count += 1;
        let ratio = 1.0 / count as f32; // first element of a `count`-chain gets 1/count
        acc = Configuration::Split {
            axis,
            ratio,
            a: Box::new(cfg),
            b: Box::new(acc),
        };
    }
    acc
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p workspace -- grid::tests`
Expected: PASS (all grid tests including Task 1's).

- [ ] **Step 5: Commit**

```bash
git add crates/workspace/src/grid.rs
git commit -m "feat(workspace): build wide-first balanced grid Configuration"
```

---

### Task 3: `panes_in_spatial_order` + `rebuild_with_new`

**Files:**
- Modify: `crates/workspace/src/grid.rs`
- Test: `crates/workspace/src/grid.rs` (`#[cfg(test)]`)

**Interfaces:**
- Consumes: `build_grid_config` (Task 2)
- Produces:
  - `pub fn panes_in_spatial_order<T>(state: &pane_grid::State<T>) -> Vec<pane_grid::Pane>`
  - `pub struct RebuildInfo { pub remap: Vec<(pane_grid::Pane, pane_grid::Pane)>, pub new_pane: pane_grid::Pane }`
  - `pub fn rebuild_with_new<T>(state: &mut pane_grid::State<T>, new_item: T, placeholder: impl FnMut() -> T) -> RebuildInfo`

- [ ] **Step 1: Write the failing test**

Extend the imports at the top of `grid.rs`:

```rust
use iced::widget::pane_grid::{self, Configuration, Pane, State};
use iced::{Rectangle, Size};
```

Add the stubs below `build_grid_config`:

```rust
/// Panes sorted into row-major spatial order (top->bottom, then left->right).
pub fn panes_in_spatial_order<T>(state: &State<T>) -> Vec<Pane> {
    todo!()
}

/// Result of rebuilding a tab's layout into a balanced grid.
pub struct RebuildInfo {
    /// `(old_pane, new_pane)` for each pre-existing window, in spatial order.
    pub remap: Vec<(Pane, Pane)>,
    /// The pane holding the newly added window.
    pub new_pane: Pane,
}

/// Drain every window from `state` in spatial order, append `new_item`, and
/// replace `state` with a freshly built wide-first balanced grid.
///
/// `placeholder` produces throwaway values used to move owned items out of the
/// old state (for `Block`, pass `|| Block::HotkeyInfo`).
pub fn rebuild_with_new<T>(
    state: &mut State<T>,
    new_item: T,
    placeholder: impl FnMut() -> T,
) -> RebuildInfo {
    todo!()
}
```

Add these tests to the `tests` module:

```rust
    /// Spatially-ordered contents of a State<u32>.
    fn ordered_contents(state: &State<u32>) -> Vec<u32> {
        panes_in_spatial_order(state)
            .iter()
            .map(|p| *state.get(*p).unwrap())
            .collect()
    }

    #[test]
    fn rebuild_appends_and_preserves_order() {
        // Start with a single window holding 10.
        let (mut state, _first) = State::new(10u32);
        // Add 20, 30, 40 one at a time.
        for v in [20u32, 30, 40] {
            rebuild_with_new(&mut state, v, || 0u32);
        }
        assert_eq!(state.len(), 4);
        assert_eq!(ordered_contents(&state), vec![10, 20, 30, 40]);
    }

    #[test]
    fn rebuild_reports_new_pane_and_remap() {
        let (mut state, _first) = State::new(1u32);
        let info = rebuild_with_new(&mut state, 2u32, || 0u32);
        // One pre-existing window -> one remap pair.
        assert_eq!(info.remap.len(), 1);
        // new_pane holds the new item.
        assert_eq!(*state.get(info.new_pane).unwrap(), 2);
        // Each remap target still exists in the new state.
        for (_old, new) in &info.remap {
            assert!(state.get(*new).is_some());
        }
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p workspace -- grid::tests::rebuild`
Expected: FAIL (`todo!()`).

- [ ] **Step 3: Write minimal implementation**

Replace the two stubs:

```rust
pub fn panes_in_spatial_order<T>(state: &State<T>) -> Vec<Pane> {
    // spacing/min_size = 0 so tiny grids aren't distorted by clamping; the bounds
    // value is arbitrary because ordering is scale-invariant.
    let regions = state
        .layout()
        .pane_regions(0.0, 0.0, Size::new(1000.0, 1000.0));
    let mut entries: Vec<(Pane, Rectangle)> = regions.into_iter().collect();
    entries.sort_by(|(_, a), (_, b)| {
        a.y.partial_cmp(&b.y)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.x.partial_cmp(&b.x).unwrap_or(std::cmp::Ordering::Equal))
    });
    entries.into_iter().map(|(p, _)| p).collect()
}

pub fn rebuild_with_new<T>(
    state: &mut State<T>,
    new_item: T,
    mut placeholder: impl FnMut() -> T,
) -> RebuildInfo {
    let old_order = panes_in_spatial_order(state);

    let mut items: Vec<T> = Vec::with_capacity(old_order.len() + 1);
    for &pane in &old_order {
        let slot = state.get_mut(pane).expect("pane from layout must exist");
        items.push(std::mem::replace(slot, placeholder()));
    }
    items.push(new_item);

    *state = State::with_configuration(build_grid_config(items));

    let new_order = panes_in_spatial_order(state);
    let remap = old_order
        .iter()
        .copied()
        .zip(new_order.iter().copied())
        .collect();
    let new_pane = *new_order.last().expect("rebuilt grid has at least one pane");

    RebuildInfo { remap, new_pane }
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p workspace -- grid::tests`
Expected: PASS (all grid tests).

- [ ] **Step 5: Commit**

```bash
git add crates/workspace/src/grid.rs
git commit -m "feat(workspace): rebuild pane grid from ordered windows"
```

---

### Task 4: Webview remap helper

**Files:**
- Modify: `crates/browser/src/webview_manager.rs`
- Test: `crates/browser/src/webview_manager.rs` (`#[cfg(test)]`)

**Interfaces:**
- Produces:
  - `pub fn remap(mapping: &[(u64, u64)])` — re-key live webviews from old pane ids to new ones.
  - private `fn remap_map<V>(map: &mut HashMap<u64, V>, mapping: &[(u64, u64)])` (pure, tested).

- [ ] **Step 1: Write the failing test**

Add to `crates/browser/src/webview_manager.rs` (after the existing functions):

```rust
/// Re-key live webviews when pane ids change (e.g. after a layout rebuild).
///
/// `mapping` is a list of `(old_pane_id, new_pane_id)` pairs. Done in two phases
/// (remove all sources, then insert at targets) so overlapping ids can't clobber.
pub fn remap(mapping: &[(u64, u64)]) {
    WEBVIEWS.with(|wvs| {
        remap_map(&mut wvs.borrow_mut(), mapping);
    });
}

/// Pure two-phase key remap, extracted for testing.
fn remap_map<V>(map: &mut HashMap<u64, V>, mapping: &[(u64, u64)]) {
    todo!()
}

#[cfg(test)]
mod tests {
    use super::remap_map;
    use std::collections::HashMap;

    #[test]
    fn remap_moves_values_to_new_keys() {
        let mut m: HashMap<u64, u32> = HashMap::new();
        m.insert(5, 105);
        m.insert(8, 108);
        // 5 -> 0, 8 -> 2
        remap_map(&mut m, &[(5, 0), (8, 2)]);
        assert_eq!(m.get(&0), Some(&105));
        assert_eq!(m.get(&2), Some(&108));
        assert_eq!(m.get(&5), None);
        assert_eq!(m.get(&8), None);
    }

    #[test]
    fn remap_handles_swaps_without_clobbering() {
        let mut m: HashMap<u64, u32> = HashMap::new();
        m.insert(0, 100);
        m.insert(1, 101);
        // swap 0 <-> 1
        remap_map(&mut m, &[(0, 1), (1, 0)]);
        assert_eq!(m.get(&0), Some(&101));
        assert_eq!(m.get(&1), Some(&100));
    }

    #[test]
    fn remap_ignores_missing_and_identity() {
        let mut m: HashMap<u64, u32> = HashMap::new();
        m.insert(3, 103);
        remap_map(&mut m, &[(3, 3), (9, 4)]); // identity + missing source
        assert_eq!(m.get(&3), Some(&103));
        assert_eq!(m.get(&4), None);
        assert_eq!(m.len(), 1);
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p browser remap`
Expected: FAIL (`todo!()`).

- [ ] **Step 3: Write minimal implementation**

Replace the `remap_map` stub:

```rust
fn remap_map<V>(map: &mut HashMap<u64, V>, mapping: &[(u64, u64)]) {
    // Phase 1: remove every source (skip identity / missing).
    let mut moved: Vec<(u64, V)> = Vec::new();
    for &(old, new) in mapping {
        if old == new {
            continue;
        }
        if let Some(v) = map.remove(&old) {
            moved.push((new, v));
        }
    }
    // Phase 2: insert each value at its new key.
    for (new, v) in moved {
        map.insert(new, v);
    }
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p browser remap`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/browser/src/webview_manager.rs
git commit -m "feat(browser): add two-phase webview key remap"
```

---

### Task 5: Wire `add_window` into all split sites

**Files:**
- Modify: `alterm/src/main.rs` (add `add_window`; rewrite 9 split sites)

**Interfaces:**
- Consumes: `workspace::grid::{rebuild_with_new, RebuildInfo}` (Task 3), `webview_manager::remap` (Task 4)
- Produces: `fn add_window(&mut self, block: Block) -> pane_grid::Pane` on `impl Alterm`

This task is integration: it has no cargo unit test (it needs the GUI binary). It ends with a build check and a manual smoke test.

- [ ] **Step 1: Import the grid helpers**

In the `use workspace::{ ... }` block starting at `alterm/src/main.rs:15`, add `grid` to the imported items so `workspace::grid::rebuild_with_new` is reachable. Either add a line `use workspace::grid;` near the other `use` statements, or add `grid,` inside the existing `use workspace::{ ... }` list. Confirm `webview_manager` is already imported (it is — used throughout `main.rs`).

- [ ] **Step 2: Add the `add_window` helper**

Add this method to the `impl Alterm` block, immediately after `fn resize_all_panes(&mut self) { ... }` (ends near `main.rs:343`, just before line 345's definition — place the new method right after the closing brace of `resize_all_panes`):

```rust
    /// Add a new window (pane) to the active tab as a wide-first balanced grid.
    ///
    /// Rebuilds the active tab's layout from its existing windows plus `block`,
    /// re-keys any browser webviews to their new pane ids, focuses the new
    /// window, and returns its pane. All "new window" actions funnel through here.
    fn add_window(&mut self, block: Block) -> pane_grid::Pane {
        let tab = self.active_tab_mut();
        // Compute the grid against the full layout, not a maximized view.
        if tab.panes.maximized().is_some() {
            tab.panes.restore();
        }

        let info = grid::rebuild_with_new(&mut tab.panes, block, || Block::HotkeyInfo);
        tab.focus = Some(info.new_pane);

        // Carry existing webviews across to their new pane ids.
        let remap_ids: Vec<(u64, u64)> = info
            .remap
            .iter()
            .map(|(old, new)| (pane_to_id(*old), pane_to_id(*new)))
            .collect();
        webview_manager::remap(&remap_ids);

        self.resize_all_panes();
        info.new_pane
    }
```

- [ ] **Step 3: Rewrite the terminal split sites**

Replace the bodies of `Message::SplitHorizontal` (`main.rs:676`) and `Message::SplitVertical` (`main.rs:701`) so both just add a terminal window to the grid. Replace the whole pair of arms with:

```rust
            // Split Right / Split Down now both add a window to the balanced grid.
            Message::SplitHorizontal | Message::SplitVertical => {
                if let Ok(block) = Block::new_terminal(24, 80) {
                    self.add_window(block);
                }
            }
```

Replace the bodies of the per-pane title-bar splits `Message::SplitPaneRight(pane)` (`main.rs:764`) and `Message::SplitPaneDown(pane)` (`main.rs:784`) with:

```rust
            // Per-pane title-bar split buttons also add a window to the grid.
            Message::SplitPaneRight(_) | Message::SplitPaneDown(_) => {
                if let Ok(block) = Block::new_terminal(24, 80) {
                    self.add_window(block);
                }
            }
```

(The initial `24, 80` dimensions are placeholders; `resize_all_panes` inside `add_window` resizes the terminal to its real region.)

- [ ] **Step 4: Rewrite AI chat**

In `Message::ToggleAIChat` (`main.rs:907`), replace the focus/split block (the `let tab = self.active_tab_mut();` through the `self.resize_all_panes();` that follows the `if let Some((new_pane, _split))` — lines ~928–943) with:

```rust
                let new_pane = self.add_window(block);
                let focus_task = widget_focus(WidgetId::from(
                    format!("ai-chat-input-{:?}", new_pane),
                ));
                let fetch_task = self.update(Message::AIFetchModels(new_pane));
                return Task::batch([focus_task, fetch_task]);
```

So the arm reads: compute `provider_name`/`model_name`, `let block = Block::new_ai_chat(provider_name, model_name);`, then the replacement above.

- [ ] **Step 5: Rewrite Settings, Browser, Preview, HotkeyInfo**

`Message::OpenSettings` (`main.rs:1158`): keep the existing-pane dedup (lines 1159–1167) unchanged. Replace the creation block (lines 1169–1178, `let block = Block::new_settings(...)` through `self.resize_all_panes();`) with:

```rust
                let block = Block::new_settings(self.config.clone());
                self.add_window(block);
```

`Message::OpenBrowser` (`main.rs:1221`): replace the body (lines 1222–1243) with:

```rust
                let url = "https://www.google.com";
                let block = Block::new_browser(url);
                let new_pane = self.add_window(block);
                // Create the webview against the final (post-rebuild) pane id.
                self.create_browser_webview(new_pane, url);
                webview_manager::pump_gtk_events();
                self.resize_all_panes();
                return widget_focus(WidgetId::from(
                    format!("browser-url-input-{:?}", new_pane),
                ));
```

`Message::OpenPreview` (`main.rs:1282`): keep the `start_path`/`path_str` computation; replace the creation block (lines 1288–1297, `let block = Block::new_preview(&path_str);` through `self.resize_all_panes();`) with:

```rust
                let block = Block::new_preview(&path_str);
                self.add_window(block);
```

`Message::ShowHotkeyInfo` (`main.rs:1382`): keep the existing-pane dedup (lines 1383–1391) unchanged. Replace the creation block (lines 1393–1402, `let block = Block::new_hotkey_info();` through `self.resize_all_panes();`) with:

```rust
                let block = Block::new_hotkey_info();
                self.add_window(block);
```

- [ ] **Step 6: Build and fix compile errors**

Run: `cargo build -p alterm`
Expected: builds cleanly. Likely fixups:
- Remove now-unused imports if the compiler warns (do not remove `pane_grid` — `add_window` returns `pane_grid::Pane`).
- If any arm now has an unused variable warning (e.g. a leftover `tab` binding), delete the dead line.

- [ ] **Step 7: Run the full test suite**

Run: `cargo test`
Expected: PASS (workspace grid tests, browser remap tests, existing keybinding tests).

- [ ] **Step 8: Manual smoke test**

Run: `cargo run -p alterm` (or the project's run command).
Verify by pressing `Ctrl+Shift+N` (new terminal) repeatedly and watching the active tab:
- 2 windows → side by side (two columns, full height each).
- 3 windows → 2×2 grid with the bottom-right empty (windows 1,2 top row; 3 bottom-left).
- 4 → full 2×2. 5–6 → 2 rows × 3 cols. 7–9 → 3×3.
- The newest window is focused each time.
- Open a Browser (`Ctrl+Shift+B`), then open another terminal — the browser keeps rendering its page after the layout reflows (webview survived the remap).
- `Ctrl+Shift+D` (Split Right) and `Ctrl+Shift+E` (Split Down) both just add a window to the grid (no directional split).
- Close a pane (`Ctrl+Shift+X`) — the neighbor absorbs the space (no reflow); opening another window re-packs into a clean grid.

- [ ] **Step 9: Commit**

```bash
git add alterm/src/main.rs
git commit -m "feat(alterm): place new windows in a wide-first balanced grid"
```

---

## Self-Review

**Spec coverage:**
- Unified "add a window" path → Task 5 (`add_window`, all 9 sites rewired). ✓
- Wide-first `cols=ceil(√N)`, `rows=ceil(N/cols)` → Task 1 (`grid_dims`). ✓
- Row-major fill + tree (outer Horizontal rows, inner Vertical cols, even ratios) → Task 2 (`build_grid_config`/`combine`). ✓
- Rebuild on add: read spatial order, drain owned blocks via `HotkeyInfo` placeholder, `with_configuration`, focus new → Task 3 (`rebuild_with_new`) + Task 5. ✓
- Webview re-key across rebuild → Task 4 (`remap`) + Task 5 (id mapping via `pane_to_id`). Spec said `rekey(old,new)`; implemented as the safer two-phase batch `remap(&[(old,new)])`. ✓
- Maximize interaction (`restore()` before rebuild) → Task 5 (`add_window`). ✓
- On close = iced default (no reflow) → unchanged; `Message::ClosePane` is not touched. ✓
- Manual resize resets on add → inherent to rebuild; no code needed. ✓
- Testing: `grid_dims`, fill-order, config leaf-count/order, drain+rebuild round trip → Tasks 1–3 tests. ✓

**Placeholder scan:** No "TBD"/"handle edge cases"/vague steps; every code step shows full code. (`todo!()` appears only as the intentional red-test stub, replaced in the same task.) ✓

**Type consistency:** `grid_dims -> (rows, cols)`; `build_grid_config<T>(Vec<T>) -> Configuration<T>`; `panes_in_spatial_order<T>(&State<T>) -> Vec<Pane>`; `rebuild_with_new<T>(&mut State<T>, T, impl FnMut()->T) -> RebuildInfo { remap: Vec<(Pane,Pane)>, new_pane: Pane }`; `remap(&[(u64,u64)])`; `add_window(&mut self, Block) -> pane_grid::Pane`. Names/types match across tasks. ✓
