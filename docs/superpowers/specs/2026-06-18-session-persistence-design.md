# Design: Session Persistence

**Date:** 2026-06-18
**Status:** Approved (ready for implementation plan)

## Goal

When Alterm is closed (or crashes) and relaunched, it restores the previous
session **exactly**: window size, all tabs (titles + order), the active tab, and
per-tab the exact pane layout (split tree + ratios + focus + maximized state),
with each pane's content restored to where it was.

In this app a "window" is a **pane** within a tab; each tab owns its own
`pane_grid::State<Block>` (`crates/workspace/src/tab.rs`).

## Scope — what is restored, per pane type

- **Terminal** → a fresh shell spawned in its previous **working directory**, with
  the previous **~1000 lines of styled scrollback** re-rendered as history above
  the new prompt.
- **Browser** → reopened to its URL, with back/forward `history` + `history_index`.
- **AI chat** → same `provider_name` / `model_name` + full `messages` conversation
  and `input` buffer.
- **Preview** → same `path`.
- **Settings / HotkeyInfo** panes → recreated (these hold no restorable state;
  Settings is rebuilt from the current `Config`).

**Hard limit (unavoidable):** live child processes cannot be revived. A restored
terminal shows replayed scrollback text with a brand-new shell beneath it.

## Decisions (from brainstorming)

1. Maximum-fidelity restore of all pane types (terminals incl. cwd + scrollback,
   browser page, AI conversation, preview path).
2. Scrollback: **capped at a fixed `SCROLLBACK_PERSIST_LINES = 1000` per terminal,
   styled** (ANSI colors preserved). Bounded file size. (Distinct from the live
   buffer size; this is only how much history is persisted.)
3. Save timing: **periodic (every ~30s) + on clean close** — survives crashes
   with ≤30s loss. No per-keystroke saving.
4. Control: a **`[session] restore` config toggle** (default `true`). No CLI flag.
5. **Corrupt/old/failed session always falls back to a fresh start** (never blocks
   launch); the bad file is backed up first.
6. cwd capture is **Linux-first** (`/proc/<pid>/cwd`); macOS/Windows degrade
   gracefully to "no cwd" → shell opens in the home/default directory.

## Session data model

A serde model mirroring app state, serialized as **JSON** to
`~/.config/alterm/session.json` (JSON, not TOML, because the layout tree and
scrollback strings are array/string-heavy):

```text
SessionState {
    version: u32,                  // schema version for forward-compat
    window:  WindowState { width: f32, height: f32 },
    active_tab: usize,
    tabs:    Vec<TabState>,
}

TabState {
    title:     String,
    focus:     Option<usize>,      // index of focused pane in spatial order
    maximized: Option<usize>,      // index of maximized pane, if any
    layout:    PaneNode,           // exact split tree + ratios
}

PaneNode =                          // mirrors iced pane_grid Node
    | Split { axis: Axis, ratio: f32, a: Box<PaneNode>, b: Box<PaneNode> }
    | Leaf(BlockState)

BlockState =
    | Terminal { cwd: Option<PathBuf>, scrollback_ansi: String, rows: u16, cols: u16 }
    | Browser  { url: String, history: Vec<String>, history_index: usize }
    | AiChat   { provider: String, model: String, messages: Vec<DisplayMessage>, input: String }
    | Preview  { path: PathBuf }
    | Settings
    | HotkeyInfo
```

`PaneNode` mirrors iced's layout `Node`, so restore reconstructs the **saved tree**
(not the auto-grid from the window-placement feature) — exact split ratios are
preserved. `Axis` is serialized as a small enum (`Horizontal`/`Vertical`).
`DisplayMessage` (already `{ role, content, model }` in `ai_chat.rs`) gains
`Serialize`/`Deserialize`.

## Capture (live app → model)

Walk each tab's `panes.layout()` (the `Node` tree) → `PaneNode`, reading each
leaf pane's `Block` into a `BlockState`. New accessors required:

- `PtyHandle::child_pid() -> Option<u32>` (via `portable_pty::Child::process_id`).
- `Block::working_dir() -> Option<PathBuf>` — for a `Terminal`, reads the symlink
  `/proc/<pid>/cwd` on Linux; returns `None` on other platforms or on error.
- `TerminalState::scrollback_ansi(max_lines: usize) -> String` — walk the grid
  (history + visible viewport, capped at `max_lines`), emitting cell text plus
  SGR escape sequences for color/style changes and `\r\n` between rows; trailing
  blank lines trimmed.
- Browser / AI / Preview read their existing public `State` fields directly.

Spatial pane order (for `focus`/`maximized` indices) reuses
`grid::panes_in_spatial_order` from the window-placement feature.

## Restore (model → live app)

For each `TabState`, build a `pane_grid::Configuration<Block>` from its `PaneNode`
(`Split { axis, ratio, a, b }` → `Configuration::Split`; `Leaf(bs)` →
`Configuration::Pane(block_from_state(bs))`), then
`State::with_configuration(...)`. Constructing each `Block` from `BlockState`:

- **Terminal**: spawn the shell **with the saved cwd** — extend `PtyHandle::spawn`
  to accept an optional working directory (set it on the `CommandBuilder`); if the
  cwd is `None` or no longer exists, fall back to the default. After creating the
  `TerminalState`, inject `scrollback_ansi` via the existing
  `TerminalState::process_output` so the saved history renders before the fresh
  shell's first prompt.
- **Browser**: `Block::new_browser(url)` with restored `history`/`history_index`;
  the wry webview is created **after** the layout is installed (same post-rebuild
  timing as `add_window` / `create_browser_webview`), keyed via the per-tab
  `webview_key` from the window-placement feature.
- **AiChat / Preview / Settings / HotkeyInfo**: reconstruct from their `BlockState`.

Then set `tab.focus`, `tab.panes.maximize(...)` if applicable, `active_tab`, and
the window size. Webviews are created for all restored browser panes after all
tabs are built; only the active tab's webviews are made visible (reuse
`update_webview_visibility`).

## Lifecycle & triggers

- **Startup** (`Alterm::new`): if `config.session.restore` and a loadable
  `session.json` exists → rebuild tabs/window from it; otherwise create the
  default single tab as today.
- **Periodic save**: add an `iced::time::every(Duration::from_secs(30))`
  subscription producing `Message::SaveSession`; its handler captures and writes
  the full session (including scrollback).
- **On close**: set `.exit_on_close_request(false)` on the `iced::application`
  builder; the existing event subscription matches
  `Event::Window(window::Event::CloseRequested)` → `Message::WindowCloseRequested`;
  the handler does a final full save, then returns `iced::exit()`.
- **Atomic writes**: write to `session.json.tmp` then rename over `session.json`,
  so a crash mid-write cannot corrupt the saved session.

## Config, safety & privacy

- Add `SessionConfig { restore: bool }` to `Config` as
  `#[serde(default)] pub session: SessionConfig`, with `restore` defaulting to
  `true` (via `#[serde(default = "default_true")]`). Existing `config.toml` files
  lacking a `[session]` section still parse.
- **Corruption / version-mismatch / any load error** → rename the file to
  `session.json.bak` and start a fresh default session. Launch never fails because
  of session state.
- **Privacy:** `session.json` stores cwd paths, browser URLs/history, and full AI
  conversation text in plaintext under `~/.config/alterm`. It does **not** store
  API keys (those remain in `config.toml`).

## Module structure

- New `crates/workspace/src/session.rs` — the serde model (`SessionState`,
  `TabState`, `PaneNode`, `BlockState`, `WindowState`), capture (`live → model`),
  restore (`model → Vec<Tab>` + window/active metadata), and file
  load/save/backup helpers (atomic write, corruption backup). Lives next to
  `Block`/`Tab`/`grid`.
- New accessors on `Block` (`working_dir`), `TerminalState` (`scrollback_ansi`),
  `PtyHandle` (`child_pid`, cwd arg on `spawn`), and `Serialize`/`Deserialize` on
  `DisplayMessage`.
- New `SessionConfig` in `crates/config/src/lib.rs`.
- `alterm/src/main.rs` — wires startup restore, the 30s save timer, the
  `CloseRequested` save+exit hook, and `.exit_on_close_request(false)`.

## Testing

Pure / unit-testable without a GUI:

- `SessionState` JSON round-trip (serialize → deserialize → equal) for a sample
  with multiple tabs and every `BlockState` variant.
- `PaneNode` ↔ `pane_grid::Configuration` conversion preserves structure, axes,
  and ratios (build a tree, convert both ways, assert).
- `TerminalState::scrollback_ansi`: feed known bytes (incl. a color sequence) into
  a `TerminalState`, capture, and assert the text and a color escape appear, and
  that line count is capped.
- `BlockState` capture for Browser / Preview / AiChat (constructible without a
  PTY) round-trips their fields.
- Corruption fallback: `session::load` on a garbage/old-version file returns
  "no session" and leaves a `.bak` file.
- cwd: test the `/proc/<pid>/cwd` reader against the current process's own pid
  (Linux-gated test).

## Implementation phasing (for the plan)

Each phase leaves a working, shippable app:

- **Phase A — foundation:** session model + config toggle + file load/save/backup
  + lifecycle (startup restore, 30s timer, close hook) + layout/tabs/window/focus
  restore + Browser / Preview / AIChat / Settings / HotkeyInfo capture+restore.
  (Terminals restore as fresh shells in the default dir, no scrollback, in this
  phase.)
- **Phase B — terminal cwd:** capture `/proc` cwd and spawn restored shells there.
- **Phase C — terminal styled scrollback:** `scrollback_ansi` capture + replay
  (highest risk, isolated last).

## Out of scope

- Reviving live processes or running commands.
- Restoring deep webview page state beyond the URL (scroll position, login
  sessions, form contents) — depends on wry profile persistence; not pursued here.
- cwd restoration on macOS/Windows (degrades to default dir).
- Encrypting the session file.
