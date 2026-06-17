# Sidebar Button Hover Tooltips with Hotkeys

**Date:** 2026-06-17
**Branch:** `feature/button-hotkey-tooltips`

## Goal

When the user hovers over a sidebar icon button, show a tooltip naming the
window/block type and the keyboard shortcut that performs the same action — so
the user can learn the hotkey and stop reaching for the mouse over time.

## Background

The sidebar (`crates/workspace/src/sidebar.rs`) is a vertical column of icon
buttons that split the focused pane with a new block type. Each button emits a
`SidebarAction`, which `main.rs` maps to a `Message`. Keyboard shortcuts live in
`crates/workspace/src/keybindings.rs` as the `Action` enum, with `label()`,
`shortcut_hint()`, and `match_shortcut()` providing a single source of truth that
both `main.rs` and the command palette already consume.

Today only 2 of the 7 sidebar buttons have a keyboard shortcut. The other 5 must
get new bindings so every tooltip can show a real hotkey.

## Decisions

1. **Scope:** tooltips on all 7 sidebar buttons (Terminal, AI, Browser, Preview,
   Settings, Info, Theme). Pane title-bar buttons are out of scope.
2. **Add hotkeys for the 5 buttons that lack one** (chosen mnemonics, all
   currently unused — no conflicts with existing `Ctrl+Shift` combos):

   | Button        | Action (existing Message)      | Existing hotkey | New hotkey     |
   |---------------|--------------------------------|-----------------|----------------|
   | Terminal      | `SidebarNewTerminal`           | —               | `Ctrl+Shift+N` |
   | AI            | `ToggleAIChat`                 | `Ctrl+Shift+A`  | (unchanged)    |
   | Browser       | `OpenBrowser`                  | —               | `Ctrl+Shift+B` |
   | Preview       | `OpenPreview`                  | —               | `Ctrl+Shift+F` |
   | Settings      | `OpenSettings`                 | `Ctrl+Shift+,`  | (unchanged)    |
   | Info          | `ShowHotkeyInfo`               | —               | `Ctrl+Shift+H` |
   | Theme         | `ToggleTheme`                  | —               | `Ctrl+Shift+L` |

3. **Tooltip content:** name + shortcut on a single line, e.g.
   `New Terminal  (Ctrl+Shift+N)`.
4. **Tooltip position:** to the **left** of each button (the sidebar is docked on
   the right edge), with a small styled background box using iced's `tooltip`
   widget.

## Architecture

### Keybindings (`crates/workspace/src/keybindings.rs`)

Add five variants to `Action`:
`NewTerminal`, `NewBrowser`, `NewPreview`, `ShowHotkeyInfo`, `ToggleTheme`.

Extend each of the existing match arms:
- `label()` — human-readable names ("New Terminal", "New Browser", "New File
  Preview", "Keyboard Shortcuts", "Toggle Theme").
- `shortcut_hint()` — the strings from the table above.
- `match_shortcut()` — in the `Ctrl+Shift` letter block add `n → NewTerminal`,
  `b → NewBrowser`, `f → NewPreview`, `h → ShowHotkeyInfo`, `l → ToggleTheme`.
- `all_palette_actions()` — include the five new actions so they appear in the
  command palette and hotkey-reference pane (keeping a single source of truth).

### Dispatch (`alterm/src/main.rs::dispatch_action`)

Add arms routing the new actions to the same Messages the sidebar already emits:
- `Action::NewTerminal => self.update(Message::SidebarNewTerminal)`
- `Action::NewBrowser => self.update(Message::OpenBrowser)`
- `Action::NewPreview => self.update(Message::OpenPreview)`
- `Action::ShowHotkeyInfo => self.update(Message::ShowHotkeyInfo)`
- `Action::ToggleTheme => self.update(Message::ToggleTheme)`

### Hotkey reference pane (`alterm/src/main.rs::hotkey_info_view`)

Add a new `WINDOWS` category section listing the five new-window actions
(`NewTerminal`, `NewBrowser`, `NewPreview`, `ShowHotkeyInfo`, `ToggleTheme`) so
the in-app shortcut list stays accurate. The existing `ToggleAIChat` and
`OpenSettings` rows stay in the `TOOLS` section. Because rows are built from a
filtered list + `shortcut_hint()`, this means adding a `windows_actions`
`matches!` filter and one more `build_section("WINDOWS", ...)` call.

### Sidebar tooltips (`crates/workspace/src/sidebar.rs`)

- Import `iced::widget::tooltip` and `tooltip::Position`.
- Add a small helper `with_tooltip(button_element, text) -> Element` that wraps a
  built button in a `tooltip` positioned `Left`, with a `container`-styled
  background box (rounded border, theme-aware colors matching the existing
  sidebar styling) and a short gap.
- In `sidebar_view`, compute each tooltip string from the corresponding `Action`
  (`format!("{}  ({})", action.label(), action.shortcut_hint())`) and wrap every
  button. The theme-toggle button's label is dynamic ("Switch to Dark/Light"),
  so its tooltip text is built inline rather than from `Action::label()`.
- `sidebar.rs` and `keybindings.rs` are in the same `workspace` crate, so the
  sidebar can call `Action::shortcut_hint()`/`label()` directly — no new
  cross-crate dependency.

## Data Flow

Unchanged at runtime: buttons still emit `SidebarAction` → `Message`. The new
keybindings flow through the existing `subscription` → `match_shortcut` →
`dispatch_action` path. Tooltips are pure view-layer overlays with no state.

## Error Handling

No new fallible paths. `match_shortcut` returns `None` for unmatched combos as
before. Tooltips render only on hover and have no failure modes.

## Testing

- `cargo build` / `cargo clippy` clean.
- Unit test in `keybindings.rs`: assert `match_shortcut` returns the correct new
  `Action` for each of `Ctrl+Shift+{N,B,F,H,L}`, and that `shortcut_hint()` for
  each new action is non-empty and matches the documented string.
- Manual verification: hover each sidebar button → tooltip shows the right text
  to the left of the button; press each new hotkey → the corresponding window
  type opens / theme toggles; confirm new shortcuts appear in the Info pane.

## Out of Scope (YAGNI)

- User-configurable / remappable keybindings.
- Tooltips on pane title-bar or other buttons.
- Animating or delaying tooltip appearance beyond iced defaults.
