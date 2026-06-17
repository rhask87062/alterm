# Design: Balanced Auto-Grid Window Placement

**Date:** 2026-06-17
**Status:** Approved (ready for implementation plan)

## Goal

When a user opens a new window in the active tab, it should fill the tab as a
balanced, roughly-square grid in a down-then-right cascade — instead of the
current behavior, where every new window splits the focused pane to the right.

In this app a "window" is a **pane** within a tab. Each tab owns its own
`pane_grid::State<Block>` (`crates/workspace/src/tab.rs`), so the grid behavior
is per-tab.

## Background / current behavior

There is no dedicated "new window" command. Several actions create a new block,
and **all of them** currently do `tab.panes.split(Axis::Vertical, focused, block)`
— i.e. always split the focused pane to the right. The split sites in
`alterm/src/main.rs` are:

- `Message::SplitHorizontal` (Split Right, `Ctrl+Shift+D`) — `Axis::Vertical`
- `Message::SplitVertical` (Split Down, `Ctrl+Shift+E`) — `Axis::Horizontal`
- `Message::SplitPaneRight(pane)` / `SplitPaneDown(pane)` — per-pane title-bar buttons
- New AI Chat (`ToggleAIChat`), New Browser, New Terminal, New Preview — each
  `split(Axis::Vertical, focused, block)`

Webviews (browser panes) are stored in `crates/browser/src/webview_manager.rs`
in a `thread_local! HashMap<u64, WebView>` keyed by `pane_to_id(pane)`, where
`pane_to_id` parses iced's opaque `Pane(N)` debug id (`main.rs:134`).

## Decisions (from brainstorming)

1. **Build order first:** this (window placement) ships before session
   persistence. Session persistence is tracked separately and is out of scope here.
2. **Balanced grid that wraps rows** to keep panes roughly square — not an
   unbounded 2-row strip.
3. **On close:** use iced's default `close()` (the freed space is absorbed by the
   neighboring pane). No reflow on close.
4. **Manual splits fold into the grid:** Split Right / Split Down (and the
   per-pane title-bar split buttons) no longer split directionally — they add a
   window to the balanced grid like every other "new window" action.
5. **Wide-first bias** for the grid shape — terminals need width, so add columns
   before rows (N=2 is two panes side by side, each full-height/half-width). Easy
   to flip to tall-first later if desired.
6. **Accepted consequence:** because each add rebuilds the layout with even
   split ratios, any manual drag-resizing the user did is reset whenever a window
   is opened.

## Design

### Unified "add a window" path

All nine split sites collapse into a single helper on the app, conceptually:

```
fn add_window(&mut self, block: Block) -> focus on the new pane
```

- New Terminal / Browser / AI Chat / Preview call it with the appropriate block.
- Split Right / Split Down and the per-pane title-bar split buttons also call it
  (they become aliases for "new window"; their directional meaning is dropped).
  The two hotkeys/buttons are now redundant — keep both bindings for muscle
  memory, but they do the same thing. (Optional later cleanup: collapse the two
  title-bar buttons into a single "+" / new-window button. Out of scope here.)
- The newly added window receives focus.

### Grid shape and fill order

For **N** windows:

```
cols = ceil(sqrt(N))
rows = ceil(N / cols)
```

This is **wide-first** (cols >= rows) — terminals need width, so columns are added
before rows. Windows are filled **row-major**: fill a row left->right, then move to
the next row down. The newest window is the last slot in fill order.

Resulting shapes:

```
N=1   N=2    N=3      N=4      N=5        N=6        N=7..9 (3x3, partial->full)
+-+   +-+-+  +-+-+    +-+-+    +-+-+-+    +-+-+-+    +-+-+-+
|1|   |1|2|  |1|2|    |1|2|    |1|2|3|    |1|2|3|    |1|2|3|
+-+   +-+-+  +-+-+    +-+-+    +-+-+-+    +-+-+-+    +-+-+-+
            |3| |    |3|4|    |4|5| |    |4|5|6|    |4|5|6|
            +-+-+    +-+-+    +-+-+-+    +-+-+-+    +-+-+-+
                                                   |7|8|9|
                                                   +-+-+-+
```

### Rebuild mechanism

A balanced grid cannot be produced by iced's incremental single-leaf splits, so
`add_window` **rebuilds the active tab's layout tree** on each call:

1. **Read current order:** walk `tab.panes.layout()` (the `Node` tree) in spatial
   order (left->right within a row, top->bottom across rows) to get the existing
   panes in grid order. (Do *not* rely on `State::iter()` — it iterates a HashMap
   and is not spatially ordered.)
2. **Drain owned blocks:** for each pane in that order, take ownership of its
   `Block` (e.g. `mem::replace` the block out via `get_mut`, using the
   `Block::HotkeyInfo` unit variant as a throwaway placeholder, or repeated
   `State::close`). Append the new block to the ordered list.
3. **Compute shape:** `cols = ceil(sqrt(N))`, `rows = ceil(N / cols)`.
4. **Build configuration:** construct a `pane_grid::Configuration` — an outer
   `Horizontal` split stacking the rows top->bottom, each row an inner `Vertical`
   split across its columns left->right, with even ratios. A row with a single
   window is just that pane (no inner split); the last row may have fewer columns
   than the rest.
5. **Install:** `tab.panes = pane_grid::State::with_configuration(config)`.
6. **Re-key webviews:** the new `Configuration` mints fresh `Pane` ids. Build an
   old->new id map (old id in spatial order -> new id in the same spatial order)
   and call a new `webview_manager::rekey(old, new)` for each browser pane so
   existing webviews follow their block. Remap `tab.focus` to the new pane id of
   the newly-added window.
7. `resize_all_panes()` as today.

`webview_manager::rekey(old_id, new_id)` is a small new function: remove the entry
at `old_id` and reinsert it at `new_id` in the `WEBVIEWS` map (no-op if absent).

### Maximize interaction

If the active tab has a maximized pane when `add_window` is called, call
`tab.panes.restore()` before rebuilding so the grid is computed against the full
layout.

### On close

`Message::ClosePane` keeps using iced's `State::close()` — the freed region is
absorbed by the neighboring pane; no rebuild. The layout may look momentarily
unbalanced after a close, and rebalances on the next `add_window`. Continue to
`webview_manager::destroy()` the closed pane's webview as today.

## Testing

The grid math and tree construction are pure and testable without a GPU/window:

- `grid_dims(n) -> (rows, cols)` for n = 1..=12 (and 0 guarded), where
  `cols = ceil(sqrt(n))` and `rows = ceil(n / cols)`.
- Fill-order -> the ordered list of `(row, col)` slots is row-major and matches the
  N=1..9 shapes above.
- `build_configuration(blocks)` produces a tree whose leaf count equals the input
  count and whose spatial traversal order equals the input order.
- Drain-and-rebuild round trip preserves block count and order (using lightweight
  block stand-ins where PTY spawning is undesirable in tests).

These live alongside the new grid module (e.g. a `grid.rs` in `crates/workspace`,
or a focused module in the workspace crate) so they don't require the GUI binary.

## Out of scope

- Session persistence (saving/restoring tabs and panes across launches) — tracked
  separately as the next feature.
- Reflow-on-close.
- Collapsing the redundant title-bar split buttons into a single button.
- Preserving manual resize ratios across rebuilds.
