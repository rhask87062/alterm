# Design: Terminal Search + Lua Hook Trigger Points

**Date:** 2026-06-19
**Branch:** `feat/search-and-lua-hooks`
**Status:** Approved (design)

## Summary

Two features that finish half-built functionality in Alterm:

1. **Terminal Search** — wire up the already-bound `Ctrl+Shift+F` / `Action::Search`
   shortcut (currently logs *"Search — not yet implemented"*) into a real
   find-bar overlay with live match highlighting and next/prev navigation.
2. **Lua Hook Trigger Points** — the Lua host (`crates/config/src/hooks.rs`) loads
   `~/.config/alterm/hooks.lua` at launch but never invokes any hook. Add three
   discrete-event trigger points so user scripts actually run.

Both are scoped to be self-contained, follow existing patterns, and ship with
unit tests.

---

## Part A — Terminal Search

### Goals

- Find-bar overlay pinned to the bottom of the **focused terminal pane**.
- Substring matching by default; an optional **regex** toggle and a **case**
  toggle.
- Searches the visible viewport **plus** scrollback history.
- Highlights all matches; the current match is visually distinct; navigation
  scrolls the terminal so the current match is on screen.
- Match counter (`current / total`); `0 / 0` when there is no match or the regex
  is invalid (no error popup).

### Architecture & component boundaries

Matching is delegated to alacritty's native search (`alacritty_terminal 0.26.0-rc1`
exposes `term::search::{RegexSearch, RegexIter}` and `Match = RangeInclusive<Point>`).
The work is split so each layer has one job and alacritty types do **not** leak
into the app layer.

#### 1. `crates/terminal/src/term.rs` — search engine (pure-ish, testable)

A neutral match type so callers never touch alacritty `Point`:

```rust
/// A search hit in grid coordinates. `line` is alacritty grid-line space:
/// 0 = top of the active screen, negative = scrollback history.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchMatch {
    pub start_line: i32,
    pub start_col: usize,
    pub end_line: i32,
    pub end_col: usize,
}
```

Pattern policy as a pure function (single source of truth, unit-tested):

```rust
/// Build the final regex string for the search engine.
/// - `regex_mode == false`: escape regex metacharacters so `query` matches
///   literally.
/// - `case_sensitive == false`: prefix `(?i)` for case-insensitive matching.
pub fn build_search_pattern(query: &str, regex_mode: bool, case_sensitive: bool) -> String;
```

Escaping is a small dependency-free helper that backslash-escapes the regex
metacharacters `\ . ^ $ * + ? ( ) [ ] { } |`.

Search execution on `TerminalState`:

```rust
/// Find all matches across history + viewport for the already-built pattern.
/// Returns Err(message) when the pattern fails to compile (invalid regex).
pub fn search_all(&self, pattern: &str) -> Result<Vec<SearchMatch>, String>;
```

`search_all` builds a `RegexSearch::new(pattern)` (mapping `Err` →
`Err(String)`), then drives a `RegexIter` from the topmost stored line
`Line(-(history_size))` to the bottom-right of the active screen, collecting
each `Match` (converted to `SearchMatch`). Matches are returned top→bottom.

Viewport scrolling so a match is visible:

```rust
/// Center the given grid line in the viewport (clamped to the available
/// history/active range). Sets the display offset accordingly.
pub fn scroll_to_line(&mut self, target_line: i32);
```

This keeps the offset math (`display_offset = clamp(rows/2 - target_line, 0,
history_size)`, applied as a `Scroll::Delta` relative to the current offset) in
the terminal crate where it can be reasoned about and tested.

#### 2. `crates/workspace/src/block.rs` — thin pass-throughs

```rust
pub fn search(&self, pattern: &str) -> Result<Vec<SearchMatch>, String>; // delegates to state; non-terminal → Ok(vec![])
pub fn scroll_to_search_match(&mut self, m: &SearchMatch);               // calls state.scroll_to_line(m.start_line); sets dirty; refresh_cache()
pub fn display_offset(&self) -> usize;                                   // for highlight mapping
```

`scroll_to_search_match` reuses the existing dirty/`refresh_cache()` discipline
(mirrors `Block::scroll`), so the cached `RenderGrid` is rebuilt at the new
offset — no cache-coherence special-casing needed in the view.

#### 3. `crates/gpu-renderer` — per-cell highlight tier

`RenderCell` gains a highlight marker; the widget draws it as a new tier in its
existing fg/bg precedence.

```rust
// grid.rs
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CellHighlight { #[default] None, Match, Current }
// RenderCell gains: pub highlight: CellHighlight  (cell_to_render / blank_cell set None)
```

In `widget.rs` the draw loop's precedence becomes:
**current-match → match → selection → cursor → normal.** Highlight colors are
theme-aware via `grid.light_mode` (amber-family background with readable
foreground; the current match uses a stronger accent than other matches). No
shader or canvas-API change — it reuses `frame.fill_rectangle` exactly like the
existing selection highlight.

#### 4. `alterm/src/main.rs` — state, messages, find bar, wiring

State on `Alterm`:

```rust
struct SearchState {
    pane: pane_grid::Pane,      // focused terminal pane being searched
    tab_id: u64,                // owning tab (so highlight never bleeds across tabs)
    query: String,
    regex: bool,                // default false (substring)
    case_sensitive: bool,       // default false
    matches: Vec<terminal::term::SearchMatch>,
    current: usize,             // 0-based; displayed as current + 1
}
// Alterm gains: search: Option<SearchState>
```

Messages:

```rust
SearchOpen,                 // Ctrl+Shift+F when a terminal pane is focused
SearchQueryChanged(String),
SearchToggleRegex,
SearchToggleCase,
SearchNext,                 // Enter / › button (wraps)
SearchPrev,                 // ‹ button (wraps)
SearchClose,                // Esc / ✕ button
```

`Action::Search` (in `dispatch_action`) maps to `Message::SearchOpen` instead of
the current debug-log stub.

### Data flow

1. `Ctrl+Shift+F` → `SearchOpen`: if the focused pane is a terminal, create
   `SearchState` and focus the find-bar text input (dedicated widget id, like
   the rename field). If not a terminal, no-op (debug log).
2. Keystroke → `SearchQueryChanged` → `build_search_pattern(query, regex, case)`
   → `block.search(pattern)`:
   - `Ok(matches)`: store, set `current = 0`, scroll to the first match.
   - `Ok(empty)` / `Err(invalid regex)`: store empty matches → counter shows
     `0/0`, no highlight, no scroll.
3. `SearchToggleRegex` / `SearchToggleCase` → flip flag → recompute as in (2).
4. `SearchNext` / `SearchPrev` → move `current` with wraparound →
   `block.scroll_to_search_match(&matches[current])`.
5. `SearchClose` → `search = None` (matches/highlights disappear on next render).

### Highlight rendering

At **every** site in `view()` that builds a `TerminalView` for the searched pane
(currently `main.rs:1812`, plus the secondary render path at `:1353`), when the
pane/tab matches the active `SearchState`:

1. `let mut grid = block.render_grid(light_mode);` (owned clone of the cache).
2. Map each `SearchMatch` to viewport cells: `viewport_row = line + grid.display_offset`;
   keep cells with `0 <= viewport_row < grid.rows`. For multi-line matches, fill
   `start_col..=` on the first line, full width on interior lines, `..=end_col`
   on the last. Set `CellHighlight::Match`, and `CellHighlight::Current` for the
   `current` match.
3. `TerminalView::new(grid)…` as today.

Because navigation goes through `block.scroll_to_search_match` (which refreshes
the cache), `grid.display_offset` is always consistent with the stored matches.

### Find bar UI

A slim, theme-aware container pinned to the bottom of the focused terminal pane
(overlaid like the existing palette/context-menu overlays):

```
🔍  [ query……………… ]   3/12   ‹  ›   Aa   .*   ✕
```

- text input (focused on open) → `SearchQueryChanged`, `on_submit` →
  `SearchNext`
- `current/total` counter (`0/0` when none)
- `‹` `›` → `SearchPrev` / `SearchNext`
- `Aa` toggle → `SearchToggleCase` (highlighted when active)
- `.*` toggle → `SearchToggleRegex` (highlighted when active)
- `✕` → `SearchClose`

### Key routing

While `search.is_some()`, terminal key routing (the `key_to_bytes` → PTY path in
the key handler/subscription) must be suppressed so typing lands in the find bar
and `Esc` closes search — mirroring the **existing guard** used for the command
palette and inline rename. `Esc` maps to `SearchClose`.

### Error handling

- Invalid regex → `search_all` returns `Err`; treated as zero matches. No panic,
  no popup.
- Empty query → zero matches; bar stays open.
- Pane closed / tab switched while search open → `SearchState.tab_id`/`pane`
  guard prevents stale highlight; if the searched pane no longer exists, close
  search.

---

## Part B — Lua Hook Trigger Points

Three discrete-event hooks. Each calls a same-named global function in
`hooks.lua` **only if defined** (guarded by `LuaHooks::has_hook`). Every call is
wrapped so a hook error is logged and never crashes or blocks the app. Hook
infrastructure (`call_hook`, `call_hook_with`, `has_hook`) already exists; this
adds the invocation sites.

| Hook | Fires | Signature | Return-value effect |
|------|-------|-----------|---------------------|
| `on_startup()` | once, immediately after `hooks.lua` loads at launch | no args | if it returns a string → logged as a startup notice (`[hooks] on_startup: …`) |
| `on_new_terminal()` | each terminal pane the user creates **after** launch | no args | if it returns a string → written to the new PTY as input, newline-terminated so it runs |
| `on_theme_change(theme)` | on dark/light toggle | new theme name (`"dark"`/`"light"`) | if it returns a string → logged |

### Wiring points (exact)

- **`on_startup`** — in `Alterm::new()` (`main.rs:~368`), right after the
  existing `hooks.load_file(...)` match. Read the result before `hooks` is moved
  into the struct: `if let Some(msg) = hooks.call_hook("on_startup") { log::info!(...) }`.
- **`on_new_terminal`** — a shared helper
  `Alterm::fire_on_new_terminal(&mut self, pane)` invoked from the terminal-
  creation handlers: `Message::NewTab` (`:1003`), `Message::SplitHorizontal |
  SplitVertical` (`:911`), `Message::SplitPaneRight | SplitPaneDown` (`:957`),
  and `Message::SidebarNewTerminal` (`:1143`). The helper first reads the string
  (`let input = self.hooks.call_hook("on_new_terminal")` — immutable borrow ends)
  then, if `Some`, looks up the new terminal block and calls
  `write_input(format!("{input}\n").as_bytes())`.
  **Explicitly does NOT fire** for session-restored terminals or the initial
  launch terminal in `new()` (those are covered conceptually by `on_startup`),
  keeping behavior predictable and avoiding a write race during construction.
- **`on_theme_change`** — in the `Message::ToggleTheme` handler (`:1730`), after
  the theme flips: `if let Some(msg) = self.hooks.call_hook_with("on_theme_change",
  new_theme_name) { log::info!(...) }`.

### Documentation artifact

Add a commented example at `config/hooks.lua.example` that exercises all three
hooks, so the feature is discoverable:

```lua
-- ~/.config/alterm/hooks.lua
function on_startup()
  return "Alterm " .. alterm.version .. " ready on " .. alterm.platform
end

function on_new_terminal()
  return "echo Welcome to Alterm"   -- runs in each new shell
end

function on_theme_change(theme)
  -- e.g. write the theme somewhere other tools can watch
  return "theme is now " .. theme
end
```

---

## Testing

### Unit tests

- **`terminal` crate**
  - `build_search_pattern`: literal escaping of metacharacters; `(?i)` prefix on
    case-insensitive; no prefix when case-sensitive; regex-mode passthrough.
  - `search_all`: substring hit/miss; case-sensitive vs insensitive;
    regex pattern (e.g. `\d+`); invalid regex → `Err`; a match located in
    scrollback (feed > viewport rows of output, assert negative `start_line`).
  - `scroll_to_line`: centers an active-screen line and a history line; clamps at
    boundaries.
- **`gpu-renderer` crate**
  - default `RenderCell.highlight == CellHighlight::None`; a small helper that
    maps a match into a grid sets `Match`/`Current` on the expected cells.
- **`config` crate (`hooks.rs`)**
  - a loaded script exposing `on_startup`/`on_new_terminal`/`on_theme_change`
    is detected via `has_hook` and returns the expected strings;
  - the no-hook case returns `None`;
  - `call_hook_with` passes the theme argument through.
- **`workspace::keybindings`** — existing tests already cover the `Search`
  action binding; no change required.

### Manual verification

- Find bar: open with `Ctrl+Shift+F`; live substring search highlights all
  matches; counter updates; `›`/`‹`/Enter navigate with wraparound and scroll the
  view; `Aa` and `.*` toggles change results; invalid regex shows `0/0`; `Esc`
  closes and clears.
- Hooks: drop in the sample `hooks.lua`; confirm the startup log line, that a new
  terminal runs the `on_new_terminal` command, and that toggling the theme logs
  the `on_theme_change` line.

---

## Out of scope (this pass)

- Search in non-terminal panes (AI chat, preview, browser).
- `format_tab_title` Lua hook (per-frame cost; needs caching/change-detection —
  deferred deliberately).
- Recomputing matches live as new terminal output arrives while the bar is open
  (matches refresh on the next query/flag edit; documented limitation).
- Persisted search history.
```
