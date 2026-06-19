# Terminal Search + Lua Hook Trigger Points — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship a find-bar terminal search (`Ctrl+Shift+F`) with live highlighting, and wire three Lua hook trigger points (`on_startup`, `on_new_terminal`, `on_theme_change`) into the running app.

**Architecture:** Search matching is delegated to alacritty's native `RegexSearch`/`RegexIter` in the `terminal` crate, exposed through a neutral `SearchMatch` type so alacritty types never reach the app. The GPU widget gains a per-cell highlight tier reusing its existing selection-drawing path. App state (`SearchState`) and a bottom-anchored find-bar overlay live in `alterm/src/main.rs`. Lua hooks reuse the existing `LuaHooks` host; this plan only adds invocation sites.

**Tech Stack:** Rust (Cargo workspace), `alacritty_terminal 0.26.0-rc1`, `iced 0.13`, `mlua` (Lua 5.4).

## Global Constraints

- No new external crate dependencies. Route `SearchMatch` / `build_search_pattern` to the app via re-exports from the `workspace` crate; route `CellHighlight` / `RenderGrid` via the `gpu_renderer` crate (both already dependencies of `alterm`).
- Substring matching is the default; regex is opt-in. Case-insensitive is the default, implemented by prefixing the pattern with `(?i)`.
- `on_new_terminal` fires ONLY for terminals the user creates after launch — never for session-restored terminals or the initial launch terminal.
- Highlight draw precedence: current-match → match → selection → cursor → normal.
- TDD where a pure unit exists; otherwise `cargo build` + manual verification. Commit after every task.
- End every commit message with:
  `Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>`
- Spec: `docs/superpowers/specs/2026-06-19-search-and-lua-hooks-design.md`.

---

## File Structure

- `crates/terminal/src/term.rs` — add `SearchMatch`, `build_search_pattern`, `escape_regex`, `TerminalState::search_all`, `TerminalState::scroll_to_line`. (Tasks 1–3)
- `crates/gpu-renderer/src/grid.rs` — add `CellHighlight`, `RenderCell.highlight` field, `highlight_colors`. (Task 4)
- `crates/gpu-renderer/src/widget.rs` — draw the highlight tier. (Task 5)
- `crates/workspace/src/block.rs` — add `Block::search`, `Block::scroll_to_search_match`, `Block::display_offset`. (Task 6)
- `crates/workspace/src/lib.rs` — re-export `SearchMatch`, `build_search_pattern`. (Task 6)
- `alterm/src/main.rs` — `wrap_index`, `SearchState`, `Alterm.search`, messages, `search_input_id`, update handlers, key guard, find bar + highlight application. (Tasks 7–10)
- `crates/config/src/hooks.rs` — add `load_str`; unit tests for the three hooks. (Task 11)
- `alterm/src/main.rs` + `config/hooks.lua.example` — hook invocation sites + example script. (Task 12)

---

## Task 1: `build_search_pattern` + `escape_regex` (terminal crate)

**Files:**
- Modify: `crates/terminal/src/term.rs` (append free functions near the bottom, before `#[cfg(test)]`)
- Test: `crates/terminal/src/term.rs` (in the existing `#[cfg(test)] mod tests`)

**Interfaces:**
- Produces: `pub fn build_search_pattern(query: &str, regex_mode: bool, case_sensitive: bool) -> String`; `fn escape_regex(s: &str) -> String`

- [ ] **Step 1: Write the failing tests**

Add to the `#[cfg(test)] mod tests` block in `crates/terminal/src/term.rs`:

```rust
#[test]
fn build_pattern_escapes_literal_metachars() {
    assert_eq!(build_search_pattern("a.b", false, true), "a\\.b");
    assert_eq!(build_search_pattern("[E0432]", false, true), "\\[E0432\\]");
}

#[test]
fn build_pattern_adds_case_insensitive_prefix() {
    assert_eq!(build_search_pattern("error", false, false), "(?i)error");
    assert_eq!(build_search_pattern("error", false, true), "error");
}

#[test]
fn build_pattern_regex_mode_passthrough() {
    assert_eq!(build_search_pattern("\\d+", true, true), "\\d+");
    assert_eq!(build_search_pattern("ab", true, false), "(?i)ab");
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p terminal build_pattern`
Expected: FAIL — `cannot find function build_search_pattern in this scope`.

- [ ] **Step 3: Write the implementation**

Add near the bottom of `crates/terminal/src/term.rs` (above `#[cfg(test)]`):

```rust
// ---------------------------------------------------------------------------
// Search pattern helpers
// ---------------------------------------------------------------------------

/// Backslash-escape regex metacharacters so a query matches literally.
fn escape_regex(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if matches!(
            c,
            '\\' | '.' | '^' | '$' | '*' | '+' | '?' | '(' | ')' | '[' | ']' | '{' | '}' | '|'
        ) {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// Build the final regex string for the search engine.
///
/// - `regex_mode == false`: the query is escaped so it matches literally.
/// - `case_sensitive == false`: a `(?i)` prefix is added for case-insensitive
///   matching.
pub fn build_search_pattern(query: &str, regex_mode: bool, case_sensitive: bool) -> String {
    let body = if regex_mode {
        query.to_string()
    } else {
        escape_regex(query)
    };
    if case_sensitive {
        body
    } else {
        format!("(?i){body}")
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p terminal build_pattern`
Expected: PASS (3 tests).

- [ ] **Step 5: Commit**

```bash
git add crates/terminal/src/term.rs
git commit -m "feat(search): add build_search_pattern + regex escaping

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 2: `SearchMatch` + `TerminalState::search_all` (terminal crate)

**Files:**
- Modify: `crates/terminal/src/term.rs` (add `SearchMatch` struct near `TermSize`; add `search_all` method in `impl TerminalState`)
- Test: `crates/terminal/src/term.rs` (tests module)

**Interfaces:**
- Consumes: `build_search_pattern` (Task 1)
- Produces: `pub struct SearchMatch { pub start_line: i32, pub start_col: usize, pub end_line: i32, pub end_col: usize }`; `pub fn search_all(&self, pattern: &str) -> Result<Vec<SearchMatch>, String>`

- [ ] **Step 1: Write the failing tests**

Add to the tests module in `crates/terminal/src/term.rs`:

```rust
#[test]
fn search_all_finds_literal_substring() {
    let mut t = TerminalState::new(24, 80);
    t.process_output(b"error here\n");
    let pat = build_search_pattern("error", false, true);
    let m = t.search_all(&pat).unwrap();
    assert_eq!(m.len(), 1);
    assert_eq!(m[0].start_line, 0);
    assert_eq!(m[0].start_col, 0);
    assert_eq!(m[0].end_col, 4); // "error" spans cols 0..=4
}

#[test]
fn search_all_respects_case() {
    let mut t = TerminalState::new(24, 80);
    t.process_output(b"ERROR\n");
    assert_eq!(t.search_all(&build_search_pattern("error", false, false)).unwrap().len(), 1);
    assert_eq!(t.search_all(&build_search_pattern("error", false, true)).unwrap().len(), 0);
}

#[test]
fn search_all_invalid_regex_is_err() {
    let t = TerminalState::new(24, 80);
    assert!(t.search_all("(").is_err());
}

#[test]
fn search_all_finds_match_in_scrollback() {
    let mut t = TerminalState::new(24, 80);
    t.process_output(b"needle\n");
    for _ in 0..40 {
        t.process_output(b"x\n");
    }
    let m = t.search_all(&build_search_pattern("needle", false, true)).unwrap();
    assert_eq!(m.len(), 1);
    assert!(m[0].start_line < 0, "match should be in scrollback history");
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p terminal search_all`
Expected: FAIL — `no method named search_all` / `cannot find type SearchMatch`.

- [ ] **Step 3: Write the implementation**

Add the struct just below the `TermSize` block in `crates/terminal/src/term.rs`:

```rust
/// A search hit in grid-line coordinates. `*_line` is alacritty grid space:
/// 0 = top of the active screen, negative = scrollback history.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchMatch {
    pub start_line: i32,
    pub start_col: usize,
    pub end_line: i32,
    pub end_col: usize,
}
```

Add this method inside `impl TerminalState` (e.g. after `scrollback_ansi`):

```rust
    /// Find every match of `pattern` across scrollback history + the active
    /// screen. Returns matches top-to-bottom. `Err` if the pattern fails to
    /// compile (invalid regex).
    pub fn search_all(&self, pattern: &str) -> Result<Vec<SearchMatch>, String> {
        use alacritty_terminal::index::Direction;
        use alacritty_terminal::term::search::{RegexIter, RegexSearch};

        let mut regex = RegexSearch::new(pattern).map_err(|e| e.to_string())?;

        let rows = self.rows() as i32;
        let cols = self.cols();
        if rows == 0 || cols == 0 {
            return Ok(Vec::new());
        }
        let history = self.term.grid().history_size() as i32;
        let start = Point::new(Line(-history), Column(0));
        let end = Point::new(Line(rows - 1), Column(cols - 1));

        let matches = RegexIter::new(start, end, Direction::Right, &self.term, &mut regex)
            .map(|m| {
                let s = *m.start();
                let e = *m.end();
                SearchMatch {
                    start_line: s.line.0,
                    start_col: s.column.0,
                    end_line: e.line.0,
                    end_col: e.column.0,
                }
            })
            .collect();
        Ok(matches)
    }
```

Note: `Line`, `Column`, `Point` are already imported at the top of `term.rs`
(`use alacritty_terminal::index::{Column, Line, Point};`). No new top-level imports needed.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p terminal search_all`
Expected: PASS (4 tests).

- [ ] **Step 5: Commit**

```bash
git add crates/terminal/src/term.rs
git commit -m "feat(search): SearchMatch + TerminalState::search_all over history+screen

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 3: `TerminalState::scroll_to_line` (terminal crate)

**Files:**
- Modify: `crates/terminal/src/term.rs` (method in `impl TerminalState`)
- Test: `crates/terminal/src/term.rs` (tests module)

**Interfaces:**
- Produces: `pub fn scroll_to_line(&mut self, target_line: i32)`

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn scroll_to_line_keeps_visible_line_put() {
    let mut t = TerminalState::new(24, 80);
    t.process_output(b"hello\n");
    t.scroll_to_line(0); // line 0 already on screen
    assert_eq!(t.display_offset(), 0);
}

#[test]
fn scroll_to_line_brings_history_line_into_view() {
    let mut t = TerminalState::new(24, 80);
    for _ in 0..100 {
        t.process_output(b"line\n");
    }
    t.scroll_to_line(-50);
    let off = t.display_offset() as i32;
    let rows = 24;
    // -50 must be within the visible window [-off, rows-1-off].
    assert!(-off <= -50 && -50 <= rows - 1 - off, "target line not visible");
    assert!(off > 0);
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p terminal scroll_to_line`
Expected: FAIL — `no method named scroll_to_line`.

- [ ] **Step 3: Write the implementation**

Add inside `impl TerminalState` (near `scroll`):

```rust
    /// Scroll so `target_line` (grid-line coordinate; negative = history) is
    /// visible. No-op when it is already on screen; otherwise centers it,
    /// clamped to the available history.
    pub fn scroll_to_line(&mut self, target_line: i32) {
        let rows = self.rows() as i32;
        let offset = self.display_offset() as i32;
        let top = -offset;
        let bottom = rows - 1 - offset;
        if target_line >= top && target_line <= bottom {
            return; // already visible
        }
        let history = self.term.grid().history_size() as i32;
        let desired = (rows / 2 - target_line).clamp(0, history);
        let delta = desired - offset;
        if delta != 0 {
            self.scroll(delta);
        }
    }
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p terminal scroll_to_line`
Expected: PASS (2 tests).

- [ ] **Step 5: Commit**

```bash
git add crates/terminal/src/term.rs
git commit -m "feat(search): TerminalState::scroll_to_line centers off-screen matches

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 4: `CellHighlight` + `RenderCell.highlight` + `highlight_colors` (gpu-renderer)

**Files:**
- Modify: `crates/gpu-renderer/src/grid.rs`
- Test: `crates/gpu-renderer/src/grid.rs` (add a `#[cfg(test)] mod tests`)

**Interfaces:**
- Produces: `pub enum CellHighlight { None, Match, Current }` (derives `Default` = `None`); `RenderCell.highlight: CellHighlight`; `pub fn highlight_colors(kind: CellHighlight, light_mode: bool) -> Option<([f32; 4], [f32; 4])>` returning `(bg, fg)`.
- Consumed by: Task 5 (widget draw), Task 10 (highlight application).

- [ ] **Step 1: Write the failing tests**

Append to `crates/gpu-renderer/src/grid.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn highlight_defaults_to_none() {
        assert_eq!(CellHighlight::default(), CellHighlight::None);
    }

    #[test]
    fn highlight_colors_distinguish_current_from_match() {
        assert!(highlight_colors(CellHighlight::None, false).is_none());
        let m = highlight_colors(CellHighlight::Match, false).unwrap();
        let c = highlight_colors(CellHighlight::Current, false).unwrap();
        assert_ne!(m.0, c.0, "current and match backgrounds must differ");
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p gpu-renderer highlight`
Expected: FAIL — `cannot find type CellHighlight` / `function highlight_colors`.

- [ ] **Step 3: Write the implementation**

In `crates/gpu-renderer/src/grid.rs`, add the enum above `RenderCell`:

```rust
/// Search-highlight state for a single cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CellHighlight {
    #[default]
    None,
    Match,
    Current,
}
```

Add the field to `RenderCell` (after `is_cursor`):

```rust
    /// Search highlight state for this cell.
    pub highlight: CellHighlight,
```

Set it in the two constructors. In `cell_to_render`, change the returned struct literal's tail to include `highlight: CellHighlight::None`:

```rust
    RenderCell { c: cell.c, fg, bg, bold, italic, underline, is_cursor, highlight: CellHighlight::None }
```

In `blank_cell`:

```rust
    RenderCell { c: ' ', fg, bg, bold: false, italic: false, underline: false, is_cursor: false, highlight: CellHighlight::None }
```

Add the color helper near the bottom (above the `#[cfg(test)]` module):

```rust
/// Background/foreground colors for a search-highlighted cell, or `None` when
/// the cell is not highlighted. Returns `(bg, fg)` as normalized RGBA.
pub fn highlight_colors(kind: CellHighlight, light_mode: bool) -> Option<([f32; 4], [f32; 4])> {
    match kind {
        CellHighlight::None => None,
        CellHighlight::Match => Some(if light_mode {
            ([1.0, 0.92, 0.55, 1.0], [0.0, 0.0, 0.0, 1.0])
        } else {
            ([0.40, 0.32, 0.0, 1.0], [1.0, 1.0, 1.0, 1.0])
        }),
        CellHighlight::Current => Some(if light_mode {
            ([1.0, 0.66, 0.16, 1.0], [0.0, 0.0, 0.0, 1.0])
        } else {
            ([0.95, 0.60, 0.0, 1.0], [0.0, 0.0, 0.0, 1.0])
        }),
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p gpu-renderer highlight`
Expected: PASS (2 tests).

- [ ] **Step 5: Commit**

```bash
git add crates/gpu-renderer/src/grid.rs
git commit -m "feat(search): per-cell CellHighlight + highlight_colors in render grid

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 5: Draw the highlight tier in the GPU widget

**Files:**
- Modify: `crates/gpu-renderer/src/widget.rs` (the per-cell draw loop, ~lines 263–277)

**Interfaces:**
- Consumes: `CellHighlight`, `highlight_colors` (Task 4)

- [ ] **Step 1: Apply the edit**

Replace this block in `crates/gpu-renderer/src/widget.rs`:

```rust
                // Determine fg/bg: selection overrides cursor which overrides normal.
                let (fg_color, bg_color) = if selected {
                    (sel_fg, sel_bg)
                } else if cell.is_cursor {
                    let fg = rgba_to_color(&cell.fg);
                    let bg = rgba_to_color(&cell.bg);
                    (bg, fg)
                } else {
                    (rgba_to_color(&cell.fg), rgba_to_color(&cell.bg))
                };

                // Draw cell background if it differs from the default bg
                // (or always for cursor/selected cells).
                if selected || cell.is_cursor || !colors_approx_equal(bg_color, default_bg) {
                    frame.fill_rectangle(top_left, cell_size, bg_color);
                }
```

with:

```rust
                // Determine fg/bg. Precedence: search highlight > selection >
                // cursor > normal.
                let hl = crate::grid::highlight_colors(cell.highlight, self.grid.light_mode);
                let (fg_color, bg_color) = if let Some((bg, fg)) = hl {
                    (rgba_to_color(&fg), rgba_to_color(&bg))
                } else if selected {
                    (sel_fg, sel_bg)
                } else if cell.is_cursor {
                    let fg = rgba_to_color(&cell.fg);
                    let bg = rgba_to_color(&cell.bg);
                    (bg, fg)
                } else {
                    (rgba_to_color(&cell.fg), rgba_to_color(&cell.bg))
                };

                // Draw cell background if it differs from the default bg
                // (or always for highlighted/cursor/selected cells).
                if hl.is_some() || selected || cell.is_cursor || !colors_approx_equal(bg_color, default_bg) {
                    frame.fill_rectangle(top_left, cell_size, bg_color);
                }
```

- [ ] **Step 2: Verify it compiles**

Run: `cargo build -p gpu-renderer`
Expected: builds with no errors.

- [ ] **Step 3: Commit**

```bash
git add crates/gpu-renderer/src/widget.rs
git commit -m "feat(search): render search highlight tier in terminal widget

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 6: Block search pass-throughs + workspace re-exports

**Files:**
- Modify: `crates/workspace/src/block.rs` (methods in `impl Block`)
- Modify: `crates/workspace/src/lib.rs` (re-exports)
- Test: `crates/workspace/src/block.rs` (tests module — add one if absent)

**Interfaces:**
- Consumes: `terminal::term::{SearchMatch, search_all, scroll_to_line, display_offset}` (Tasks 2–3)
- Produces: `Block::search(&self, &str) -> Result<Vec<SearchMatch>, String>`; `Block::scroll_to_search_match(&mut self, &SearchMatch)`; `Block::display_offset(&self) -> usize`; `workspace::SearchMatch`; `workspace::build_search_pattern`.

- [ ] **Step 1: Write the failing tests**

Add to `crates/workspace/src/block.rs` (create the `#[cfg(test)] mod tests` block if none exists; otherwise append):

```rust
#[cfg(test)]
mod search_tests {
    use super::*;

    #[test]
    fn terminal_block_search_finds_matches() {
        let mut b = Block::new_terminal(24, 80).unwrap();
        if let Block::Terminal { state, .. } = &mut b {
            state.process_output(b"hello world\n");
        }
        let pat = terminal::term::build_search_pattern("world", false, true);
        let matches = b.search(&pat).unwrap();
        assert_eq!(matches.len(), 1);
    }

    #[test]
    fn non_terminal_block_search_is_empty() {
        let b = Block::new_hotkey_info();
        assert!(b.search("anything").unwrap().is_empty());
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p workspace search`
Expected: FAIL — `no method named search`.

- [ ] **Step 3: Write the implementation**

Add to `impl Block` in `crates/workspace/src/block.rs` (near `scroll`):

```rust
    /// Find all search matches in this block's terminal (empty for non-terminals).
    /// `Err` only on invalid regex pattern.
    pub fn search(&self, pattern: &str) -> Result<Vec<terminal::term::SearchMatch>, String> {
        match self {
            Block::Terminal { state, .. } => state.search_all(pattern),
            _ => Ok(Vec::new()),
        }
    }

    /// Scroll this terminal so the given match is visible (no-op for non-terminals).
    pub fn scroll_to_search_match(&mut self, m: &terminal::term::SearchMatch) {
        if let Block::Terminal { state, dirty, .. } = self {
            state.scroll_to_line(m.start_line);
            *dirty = true;
        }
        self.refresh_cache();
    }

    /// Current scroll offset of this terminal (0 for non-terminals).
    pub fn display_offset(&self) -> usize {
        match self {
            Block::Terminal { state, .. } => state.display_offset(),
            _ => 0,
        }
    }
```

If `block.rs` does not already import the terminal crate by name, the fully-qualified
`terminal::term::...` paths above work as-is (the crate is a dependency). Add re-exports
to `crates/workspace/src/lib.rs` (next to the other `pub use` lines):

```rust
pub use terminal::term::{build_search_pattern, SearchMatch};
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p workspace search`
Expected: PASS (2 tests).

- [ ] **Step 5: Commit**

```bash
git add crates/workspace/src/block.rs crates/workspace/src/lib.rs
git commit -m "feat(search): Block search/scroll pass-throughs + workspace re-exports

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 7: App search state scaffolding + `wrap_index`

**Files:**
- Modify: `alterm/src/main.rs` (imports; `SearchState` struct; `Alterm.search` field + init; `search_input_id`; `wrap_index`)
- Test: `alterm/src/main.rs` (existing `#[cfg(test)] mod tests`)

**Interfaces:**
- Consumes: `workspace::{SearchMatch}` (Task 6)
- Produces: `struct SearchState`; `Alterm.search: Option<SearchState>`; `fn search_input_id() -> WidgetId`; `fn wrap_index(current: usize, len: usize, forward: bool) -> usize`. (The `Message::Search*` variants are added in Task 8, where they are handled — adding them here would leave `update()`'s match non-exhaustive.)

- [ ] **Step 1: Write the failing test**

Add to the existing `#[cfg(test)] mod tests` in `alterm/src/main.rs`:

```rust
#[test]
fn wrap_index_wraps_both_directions() {
    assert_eq!(wrap_index(0, 3, true), 1);
    assert_eq!(wrap_index(2, 3, true), 0); // wrap forward
    assert_eq!(wrap_index(0, 3, false), 2); // wrap backward
    assert_eq!(wrap_index(0, 0, true), 0); // empty is safe
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p alterm wrap_index`
Expected: FAIL — `cannot find function wrap_index`.

- [ ] **Step 3: Write the implementation**

Add `SearchMatch` to the `workspace` import list at the top of `main.rs`:

```rust
use workspace::{
    all_palette_actions, match_shortcut, sidebar_view, tab_bar_view, Action, Block, BrowserState,
    CommandPalette, PreviewState, SearchMatch, SettingsField, SettingsSection, SidebarAction, Tab,
    TabBarAction, CELL_HEIGHT,
};
```

Add the `SearchState` struct near `ContextMenuState`:

```rust
/// Active find-bar search state for one terminal pane.
struct SearchState {
    pane: pane_grid::Pane,
    tab_id: u64,
    query: String,
    regex: bool,
    case_sensitive: bool,
    matches: Vec<SearchMatch>,
    current: usize,
}

/// Widget id of the find-bar text field, so it can be focused on open.
fn search_input_id() -> WidgetId {
    WidgetId::from("terminal-search-input".to_string())
}

/// Next/previous index with wraparound; returns 0 for an empty set.
fn wrap_index(current: usize, len: usize, forward: bool) -> usize {
    if len == 0 {
        return 0;
    }
    if forward {
        (current + 1) % len
    } else {
        (current + len - 1) % len
    }
}
```

Add the field to the `Alterm` struct definition (after `rename_buffer` or any field):

```rust
    /// Active terminal find-bar search, if open.
    search: Option<SearchState>,
```

Initialize it in `Alterm::new()` where the `Alterm { ... }` literal is built (alongside `rename: None`):

```rust
            search: None,
```

- [ ] **Step 4: Run test + build to verify**

Run: `cargo test -p alterm wrap_index`
Expected: the `wrap_index` test PASSES, and the crate compiles. There will be
`dead_code` warnings for the as-yet-unused `SearchState`, the `search` field, and
`search_input_id` — that is expected and resolved in Tasks 8–10.

- [ ] **Step 5: Commit**

```bash
git add alterm/src/main.rs
git commit -m "feat(search): app search state, messages, wrap_index helper

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 8: Search messages, update handlers + dispatch wiring

**Files:**
- Modify: `alterm/src/main.rs` (`Message` variants; `dispatch_action` `Action::Search` arm; new `update()` arms; three helper methods on `impl Alterm`)

**Interfaces:**
- Consumes: `SearchState`, `wrap_index`, `search_input_id` (Task 7); `Block::{search, scroll_to_search_match}`, `workspace::build_search_pattern` (Task 6)
- Produces: `Message::{SearchOpen, SearchQueryChanged(String), SearchToggleRegex, SearchToggleCase, SearchNext, SearchPrev, SearchClose}` (added and handled here); `Alterm::{recompute_search, step_search, scroll_to_current_match}`.

- [ ] **Step 1: Add the `Message` variants**

Add to the `Message` enum (e.g. after the `ShowHotkeyInfo` line):

```rust
    // Terminal search (find bar)
    SearchOpen,
    SearchQueryChanged(String),
    SearchToggleRegex,
    SearchToggleCase,
    SearchNext,
    SearchPrev,
    SearchClose,
```

- [ ] **Step 2: Replace the `Action::Search` stub**

In `dispatch_action`, replace:

```rust
            Action::Search => {
                log::debug!("Search — not yet implemented");
                Task::none()
            }
```

with:

```rust
            Action::Search => self.update(Message::SearchOpen),
```

- [ ] **Step 3: Add the helper methods**

Add to `impl Alterm` (near `cancel_rename`):

```rust
    /// Recompute matches for the current query and jump to the first match.
    fn recompute_search(&mut self) -> Task<Message> {
        let (pattern, pane, tab_id, empty) = match self.search.as_ref() {
            Some(s) => (
                workspace::build_search_pattern(&s.query, s.regex, s.case_sensitive),
                s.pane,
                s.tab_id,
                s.query.is_empty(),
            ),
            None => return Task::none(),
        };
        let matches = if empty {
            Vec::new()
        } else {
            self.tabs
                .iter()
                .find(|t| t.id == tab_id)
                .and_then(|t| t.panes.get(pane))
                .map_or(Vec::new(), |b| b.search(&pattern).unwrap_or_default())
        };
        if let Some(s) = self.search.as_mut() {
            s.matches = matches;
            s.current = 0;
        }
        self.scroll_to_current_match();
        Task::none()
    }

    /// Move the current match index and scroll it into view.
    fn step_search(&mut self, forward: bool) {
        if let Some(s) = self.search.as_mut() {
            if s.matches.is_empty() {
                return;
            }
            s.current = wrap_index(s.current, s.matches.len(), forward);
        }
        self.scroll_to_current_match();
    }

    /// Scroll the searched pane so the current match is visible.
    fn scroll_to_current_match(&mut self) {
        let (tab_id, pane, m) = match self.search.as_ref() {
            Some(s) if !s.matches.is_empty() => (s.tab_id, s.pane, s.matches[s.current].clone()),
            _ => return,
        };
        if let Some(tab) = self.tabs.iter_mut().find(|t| t.id == tab_id) {
            if let Some(block) = tab.panes.get_mut(pane) {
                block.scroll_to_search_match(&m);
            }
        }
    }
```

- [ ] **Step 4: Add the `update()` arms**

Add these arms to the `match message` in `update()` (anywhere among the other arms):

```rust
            Message::SearchOpen => {
                let pane = match self.active_tab().focus {
                    Some(p) => p,
                    None => return Task::none(),
                };
                let is_term = self
                    .active_tab()
                    .panes
                    .get(pane)
                    .map_or(false, |b| b.is_terminal());
                if !is_term {
                    log::debug!("Search: focused pane is not a terminal");
                    return Task::none();
                }
                let tab_id = self.active_tab().id;
                self.search = Some(SearchState {
                    pane,
                    tab_id,
                    query: String::new(),
                    regex: false,
                    case_sensitive: false,
                    matches: Vec::new(),
                    current: 0,
                });
                return text_input::focus(search_input_id());
            }
            Message::SearchQueryChanged(q) => {
                if let Some(s) = self.search.as_mut() {
                    s.query = q;
                }
                return self.recompute_search();
            }
            Message::SearchToggleRegex => {
                if let Some(s) = self.search.as_mut() {
                    s.regex = !s.regex;
                }
                return self.recompute_search();
            }
            Message::SearchToggleCase => {
                if let Some(s) = self.search.as_mut() {
                    s.case_sensitive = !s.case_sensitive;
                }
                return self.recompute_search();
            }
            Message::SearchNext => {
                self.step_search(true);
                return Task::none();
            }
            Message::SearchPrev => {
                self.step_search(false);
                return Task::none();
            }
            Message::SearchClose => {
                self.search = None;
                return Task::none();
            }
```

- [ ] **Step 5: Verify it compiles**

Run: `cargo build -p alterm`
Expected: builds with no errors; the Task 7 `dead_code` warnings for `SearchState`
and the `search` field are now gone.

- [ ] **Step 6: Commit**

```bash
git add alterm/src/main.rs
git commit -m "feat(search): messages + update handlers + dispatch Action::Search

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 9: Key routing guard for the open find bar

**Files:**
- Modify: `alterm/src/main.rs` (`Message::KeyboardInput` handler, after the rename guard at ~line 1623)

**Interfaces:**
- Consumes: `Alterm.search` (Task 7); `Message::{SearchClose, SearchNext}` (Task 8)

- [ ] **Step 1: Insert the guard**

In the `Message::KeyboardInput(key, modified_key, modifiers)` arm, immediately
after the rename guard block (the one that ends with `return Task::none(); }`
for `self.rename.is_some()`) and before the `if self.palette.visible {` block,
insert:

```rust
                // While the find bar is open it owns the keyboard: Escape closes,
                // Enter jumps to the next match, and other keys are handled by the
                // text_input (or swallowed) rather than running shortcuts / a PTY.
                if self.search.is_some() {
                    match &key {
                        Key::Named(Named::Escape) => return self.update(Message::SearchClose),
                        Key::Named(Named::Enter) => return self.update(Message::SearchNext),
                        _ => {
                            // Let Ctrl+Shift+F toggle the find bar off.
                            if let Some(Action::Search) = match_shortcut(&key, &modifiers) {
                                return self.update(Message::SearchClose);
                            }
                            return Task::none();
                        }
                    }
                }
```

- [ ] **Step 2: Verify it compiles**

Run: `cargo build -p alterm`
Expected: builds with no errors.

- [ ] **Step 3: Commit**

```bash
git add alterm/src/main.rs
git commit -m "feat(search): route keyboard to find bar while open (Esc/Enter/toggle)

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 10: Find-bar view + highlight application

**Files:**
- Modify: `alterm/src/main.rs` (imports; `view()` terminal-pane branch + `active_tab_id` capture; two new free functions `apply_search_highlights`, `search_bar_view`)

**Interfaces:**
- Consumes: `SearchState` (Task 7); `gpu_renderer::grid::{CellHighlight, RenderGrid}`, `SearchMatch` (Tasks 4/6)
- Produces: `fn apply_search_highlights(grid: &mut RenderGrid, matches: &[SearchMatch], current: usize)`; `fn search_bar_view<'a>(s: &'a SearchState) -> Element<'a, Message>`

- [ ] **Step 1: Add imports**

At the top of `main.rs`, after `use gpu_renderer::widget::TerminalView;`:

```rust
use gpu_renderer::grid::{CellHighlight, RenderGrid};
```

- [ ] **Step 2: Add the highlight-application helper**

Add near the other free functions (e.g. above `lerp_color`):

```rust
/// Stamp search highlights onto a render grid for the searched pane.
fn apply_search_highlights(grid: &mut RenderGrid, matches: &[SearchMatch], current: usize) {
    let offset = grid.display_offset as i32;
    let last_col = grid.cols.saturating_sub(1);
    for (i, m) in matches.iter().enumerate() {
        let kind = if i == current {
            CellHighlight::Current
        } else {
            CellHighlight::Match
        };
        for line in m.start_line..=m.end_line {
            let row = line + offset;
            if row < 0 || row as usize >= grid.rows {
                continue;
            }
            let row = row as usize;
            let col_start = if line == m.start_line { m.start_col } else { 0 };
            let col_end = if line == m.end_line { m.end_col.min(last_col) } else { last_col };
            for col in col_start..=col_end {
                if let Some(cell) = grid.cells.get_mut(row).and_then(|r| r.get_mut(col)) {
                    cell.highlight = kind;
                }
            }
        }
    }
}
```

- [ ] **Step 3: Add the find-bar view**

Add near the other view helpers (e.g. above `title_bar_button`):

```rust
/// Build the bottom-anchored find bar overlaid on the searched terminal pane.
fn search_bar_view<'a>(s: &'a SearchState) -> Element<'a, Message> {
    let counter = if s.matches.is_empty() {
        "0/0".to_string()
    } else {
        format!("{}/{}", s.current + 1, s.matches.len())
    };
    let case_label = if s.case_sensitive { "[Aa]" } else { "Aa" };
    let regex_label = if s.regex { "[.*]" } else { ".*" };

    let input = text_input("Find", &s.query)
        .id(search_input_id())
        .on_input(Message::SearchQueryChanged)
        .on_submit(Message::SearchNext)
        .size(13)
        .padding(Padding::from([2, 6]))
        .width(Length::Fixed(220.0));

    let bar = row![
        text("\u{1F50D}").size(13),
        input,
        text(counter).size(12),
        title_bar_button("\u{2039}", Message::SearchPrev),
        title_bar_button("\u{203A}", Message::SearchNext),
        title_bar_button(case_label, Message::SearchToggleCase),
        title_bar_button(regex_label, Message::SearchToggleRegex),
        title_bar_button("\u{00D7}", Message::SearchClose),
    ]
    .spacing(6)
    .align_y(iced::Alignment::Center);

    let is_focused = true;
    let inner = container(bar)
        .padding(6)
        .style(move |t: &Theme| title_bar_style(t, is_focused));

    // Transparent outer container pins the bar to the bottom-right without
    // capturing pointer events over the rest of the pane (NOT wrapped in
    // `opaque`, so the terminal stays interactive).
    container(inner)
        .width(Fill)
        .height(Fill)
        .align_x(iced::alignment::Horizontal::Right)
        .align_y(iced::alignment::Vertical::Bottom)
        .padding(8)
        .into()
}
```

- [ ] **Step 4: Capture the active tab id for the pane closure**

In `view()`, next to the other captured locals (near `let pane_labels = &tab.pane_labels;`), add:

```rust
        let active_tab_id = tab.id;
```

- [ ] **Step 5: Integrate into the terminal-pane branch**

Replace the `Block::Terminal { .. } => { ... }` arm of the pane-grid content
`match block` (currently builds `grid`, `terminal_view`, and returns
`terminal_view.view(...)`) with:

```rust
                    Block::Terminal { .. } => {
                        let mut grid = block.render_grid(light_mode);
                        let searching = self
                            .search
                            .as_ref()
                            .map_or(false, |s| s.tab_id == active_tab_id && s.pane == pane);
                        if searching {
                            if let Some(s) = &self.search {
                                apply_search_highlights(&mut grid, &s.matches, s.current);
                            }
                        }
                        let terminal_view = TerminalView::new(grid)
                            .with_font_size(self.config.appearance.font_size)
                            .with_font_family(self.terminal_font_family);
                        let term_el = terminal_view.view(
                            Message::TerminalSelected,
                            move |pos| Message::ContextMenuOpen(pane, pos),
                        );
                        if searching {
                            if let Some(s) = &self.search {
                                let bar = search_bar_view(s);
                                stack![term_el, bar].into()
                            } else {
                                term_el
                            }
                        } else {
                            term_el
                        }
                    }
```

- [ ] **Step 6: Verify it compiles**

Run: `cargo build -p alterm`
Expected: builds with no errors.

- [ ] **Step 7: Manual verification**

Run: `cargo run -p alterm --release`
Then:
- Run a command that produces output (e.g. `ls -la`, or `cargo --help`).
- Press `Ctrl+Shift+F`; the find bar appears bottom-right and is focused.
- Type a substring present in the output → matches highlight, counter shows `n/total`, view scrolls to the first match.
- `›` / `‹` / Enter move between matches with wraparound; the current match is a distinct color.
- Toggle `Aa` and `.*`; results update. Type an invalid regex with `.*` on (e.g. `(`) → counter shows `0/0`, no crash.
- `Esc` or `×` closes the bar and clears highlights.

- [ ] **Step 8: Commit**

```bash
git add alterm/src/main.rs
git commit -m "feat(search): bottom find bar overlay + match highlighting in view

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 11: `LuaHooks::load_str` + hook contract tests

**Files:**
- Modify: `crates/config/src/hooks.rs` (refactor `load_file` to share an exec path; add `load_str`; tests)
- Test: `crates/config/src/hooks.rs` (`#[cfg(test)] mod tests`)

**Interfaces:**
- Produces: `pub fn load_str(&mut self, src: &str) -> Result<(), String>` (used by tests and available for callers).
- Consumed by: Task 12 wiring relies on `call_hook`, `call_hook_with`, `has_hook` (already present) behaving per these tests.

- [ ] **Step 1: Write the failing tests**

Add to `crates/config/src/hooks.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn loaded(src: &str) -> LuaHooks {
        let mut h = LuaHooks::new();
        h.load_str(src).unwrap();
        h
    }

    #[test]
    fn on_startup_returns_string() {
        let h = loaded("function on_startup() return 'hi ' .. alterm.version end");
        let out = h.call_hook("on_startup").unwrap();
        assert!(out.starts_with("hi "));
    }

    #[test]
    fn on_new_terminal_detected_and_returns() {
        let h = loaded("function on_new_terminal() return 'echo hello' end");
        assert!(h.has_hook("on_new_terminal"));
        assert_eq!(h.call_hook("on_new_terminal").unwrap(), "echo hello");
    }

    #[test]
    fn on_theme_change_receives_arg() {
        let h = loaded("function on_theme_change(t) return 'now ' .. t end");
        assert_eq!(h.call_hook_with("on_theme_change", "light").unwrap(), "now light");
    }

    #[test]
    fn missing_hook_returns_none() {
        let h = loaded("x = 1");
        assert!(h.call_hook("on_startup").is_none());
        assert!(!h.has_hook("on_startup"));
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p alterm-config hooks::tests`
(If the package name differs, use the name from `crates/config/Cargo.toml`'s
`[package] name`; per the codebase it is `alterm-config`.)
Expected: FAIL — `no method named load_str`.

- [ ] **Step 3: Write the implementation**

In `crates/config/src/hooks.rs`, refactor `load_file` to delegate to a shared
exec path and add `load_str`. Replace the body of `load_file` so it reads the
file then calls `load_str`:

```rust
    pub fn load_file(&mut self, path: &Path) -> Result<bool, String> {
        if !path.exists() {
            return Ok(false);
        }
        let source = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
        self.load_str(&source)?;
        Ok(true)
    }

    /// Load hooks from a Lua source string. Seeds globals, then executes.
    pub fn load_str(&mut self, src: &str) -> Result<(), String> {
        self.setup_globals()?;
        self.lua
            .load(src)
            .exec()
            .map_err(|e| format!("Lua error: {e}"))?;
        self.loaded = true;
        Ok(())
    }
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p alterm-config hooks::tests`
Expected: PASS (4 tests).

- [ ] **Step 5: Commit**

```bash
git add crates/config/src/hooks.rs
git commit -m "feat(hooks): LuaHooks::load_str + contract tests for the three hooks

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 12: Wire hook trigger points + example script

**Files:**
- Modify: `alterm/src/main.rs` (`new()` on_startup; `ToggleTheme` on_theme_change; `fire_on_new_terminal` helper + 3 call sites)
- Create: `config/hooks.lua.example`

**Interfaces:**
- Consumes: `LuaHooks::{call_hook, call_hook_with}` (existing); `Block::write_input` (existing)
- Produces: `Alterm::fire_on_new_terminal(&mut self, pane: pane_grid::Pane)`

- [ ] **Step 1: Fire `on_startup` in `new()`**

In `Alterm::new()`, replace the existing hooks-load match:

```rust
        let mut hooks = LuaHooks::new();
        match hooks.load_file(&AppConfig::hooks_path()) {
            Ok(true) => log::info!("Lua hooks loaded from {:?}", AppConfig::hooks_path()),
            Ok(false) => log::debug!("No hooks.lua found; Lua hooks disabled"),
            Err(e) => log::warn!("Failed to load hooks.lua: {e}"),
        }
```

with:

```rust
        let mut hooks = LuaHooks::new();
        match hooks.load_file(&AppConfig::hooks_path()) {
            Ok(true) => log::info!("Lua hooks loaded from {:?}", AppConfig::hooks_path()),
            Ok(false) => log::debug!("No hooks.lua found; Lua hooks disabled"),
            Err(e) => log::warn!("Failed to load hooks.lua: {e}"),
        }
        if let Some(msg) = hooks.call_hook("on_startup") {
            log::info!("[hooks] on_startup: {msg}");
        }
```

- [ ] **Step 2: Fire `on_theme_change` in `ToggleTheme`**

Replace the `Message::ToggleTheme` arm:

```rust
            Message::ToggleTheme => {
                let new_theme = theme_partner(&self.config.appearance.theme).to_string();
                self.config.appearance.theme = new_theme;
                if let Err(e) = self.config.save(&AppConfig::config_path()) {
                    log::error!("Failed to save theme: {e}");
                }
                // Sync any open settings panes so their working copy matches.
                for tab in &mut self.tabs {
                    for (_pane, block) in tab.panes.iter_mut() {
                        if let Block::Settings { state } = block {
                            state.config.appearance.theme = self.config.appearance.theme.clone();
                        }
                    }
                }
            }
```

with the same body plus the hook call at the end (the settings-sync loop's
mutable borrow of `self.tabs` has ended, so borrowing `self.hooks` and
`self.config` here is fine):

```rust
            Message::ToggleTheme => {
                let new_theme = theme_partner(&self.config.appearance.theme).to_string();
                self.config.appearance.theme = new_theme;
                if let Err(e) = self.config.save(&AppConfig::config_path()) {
                    log::error!("Failed to save theme: {e}");
                }
                // Sync any open settings panes so their working copy matches.
                for tab in &mut self.tabs {
                    for (_pane, block) in tab.panes.iter_mut() {
                        if let Block::Settings { state } = block {
                            state.config.appearance.theme = self.config.appearance.theme.clone();
                        }
                    }
                }
                if let Some(msg) = self
                    .hooks
                    .call_hook_with("on_theme_change", &self.config.appearance.theme)
                {
                    log::info!("[hooks] on_theme_change: {msg}");
                }
            }
```

- [ ] **Step 3: Add the `fire_on_new_terminal` helper**

Add to `impl Alterm` (near `save_session`):

```rust
    /// Fire the `on_new_terminal` Lua hook for a freshly created terminal pane
    /// in the active tab. If the hook returns a string, it is written to the new
    /// PTY as a newline-terminated command. No-op when the hook is undefined.
    fn fire_on_new_terminal(&mut self, pane: pane_grid::Pane) {
        let Some(input) = self.hooks.call_hook("on_new_terminal") else {
            return;
        };
        if input.is_empty() {
            return;
        }
        if let Some(block) = self.active_tab_mut().panes.get_mut(pane) {
            block.write_input(format!("{input}\n").as_bytes());
        }
    }
```

- [ ] **Step 4: Call it from the real terminal-creation sites**

`add_window` returns the newly created `pane_grid::Pane`. `Message::SidebarNewTerminal`
and `Action::NewTerminal` both delegate to `Message::SplitHorizontal`, so the hook
must be fired in the THREE base creation arms only — adding it to
`SidebarNewTerminal` as well would double-fire.

In `crates/.../alterm/src/main.rs`, replace the `SplitHorizontal | SplitVertical` arm:

```rust
            Message::SplitHorizontal | Message::SplitVertical => {
                if let Ok(block) = Block::new_terminal(24, 80) {
                    self.add_window(block);
                }
            }
```

with:

```rust
            Message::SplitHorizontal | Message::SplitVertical => {
                if let Ok(block) = Block::new_terminal(24, 80) {
                    let pane = self.add_window(block);
                    self.fire_on_new_terminal(pane);
                }
            }
```

Replace the `SplitPaneRight(_) | SplitPaneDown(_)` arm:

```rust
            Message::SplitPaneRight(_) | Message::SplitPaneDown(_) => {
                if let Ok(block) = Block::new_terminal(24, 80) {
                    self.add_window(block);
                }
            }
```

with:

```rust
            Message::SplitPaneRight(_) | Message::SplitPaneDown(_) => {
                if let Ok(block) = Block::new_terminal(24, 80) {
                    let pane = self.add_window(block);
                    self.fire_on_new_terminal(pane);
                }
            }
```

Replace the `NewTab` arm:

```rust
            Message::NewTab => {
                if let Ok(new_tab) = Tab::new() {
                    self.tabs.push(new_tab);
                    self.active_tab = self.tabs.len() - 1;
                }
            }
```

with:

```rust
            Message::NewTab => {
                if let Ok(new_tab) = Tab::new() {
                    self.tabs.push(new_tab);
                    self.active_tab = self.tabs.len() - 1;
                    if let Some(pane) = self.active_tab().focus {
                        self.fire_on_new_terminal(pane);
                    }
                }
            }
```

Do NOT add any call in `Alterm::new()`, the session-restore path, or the
`ToggleAIChat` / `OpenBrowser` / `OpenPreview` arms (those create non-terminal
panes) — launch and restored terminals must not fire the hook (Global Constraints).

- [ ] **Step 5: Create the example script**

Create `config/hooks.lua.example`:

```lua
-- Alterm Lua hooks — copy to ~/.config/alterm/hooks.lua to enable.
--
-- A global `alterm` table is available: alterm.version, alterm.platform, alterm.home.
-- Each function below is optional; define only the ones you want.

-- Runs once at launch. Returned string is logged.
function on_startup()
  return "Alterm " .. alterm.version .. " ready on " .. alterm.platform
end

-- Runs for every terminal you open after launch (not restored/launch terminals).
-- Returned string is run in the new shell.
function on_new_terminal()
  -- return "echo Welcome to Alterm"
  return ""
end

-- Runs when you toggle the theme. `theme` is "dark" or "light". Returned string is logged.
function on_theme_change(theme)
  return "theme is now " .. theme
end
```

- [ ] **Step 6: Verify it compiles + manual check**

Run: `cargo build -p alterm`
Expected: builds with no errors.

Manual:
- `cp config/hooks.lua.example ~/.config/alterm/hooks.lua`, uncomment the
  `on_new_terminal` echo line.
- `cargo run -p alterm` with `RUST_LOG=info` → startup log shows
  `[hooks] on_startup: Alterm …`.
- Open a new terminal (`Ctrl+Shift+N`) → the shell runs `echo Welcome to Alterm`.
- Toggle theme (`Ctrl+Shift+L`) → log shows `[hooks] on_theme_change: theme is now …`.

- [ ] **Step 7: Commit**

```bash
git add alterm/src/main.rs config/hooks.lua.example
git commit -m "feat(hooks): fire on_startup / on_new_terminal / on_theme_change + example

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Final verification

- [ ] Run the full workspace test suite:

Run: `cargo test`
Expected: all tests pass (new: terminal search/pattern/scroll, gpu-renderer highlight, workspace block search, alterm wrap_index, config hooks).

- [ ] Run a release build:

Run: `cargo build --release`
Expected: clean build.

- [ ] Manual smoke test of both features per Task 10 Step 7 and Task 12 Step 6.
