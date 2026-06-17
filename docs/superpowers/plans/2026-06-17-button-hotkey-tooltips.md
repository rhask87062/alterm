# Sidebar Button Hover Tooltips with Hotkeys — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Show a hover tooltip on each sidebar icon button naming the window type and its keyboard shortcut, and add real hotkeys for the buttons that lack one.

**Architecture:** Extend the central `Action` keybinding registry (`crates/workspace/src/keybindings.rs`) with six new variants, route them through the existing `match_shortcut` → `dispatch_action` path in `alterm/src/main.rs`, surface them in the command palette (automatic via `all_palette_actions`) and the in-app hotkey reference pane, and wrap each sidebar button in iced's `tooltip` widget with text derived from the registry.

**Tech Stack:** Rust, iced 0.14 (`wgpu`/`canvas`/`svg`), Cargo workspace with a `workspace` crate and an `alterm` binary crate.

## Global Constraints

- iced version: `0.14` (workspace dependency — do not bump).
- Keybinding scheme: `Ctrl+Shift+<key>`; new combos must not collide with existing ones (taken letters: `t w d e x z p a c v` and `,`).
- Single source of truth: shortcut strings and labels live only in `Action::shortcut_hint()` / `Action::label()`; never hardcode a shortcut string in the sidebar or palette.
- `Action` is matched exhaustively in `label()`, `shortcut_hint()`, and `dispatch_action()` — every new variant must get an arm in all three or the build fails.
- `crates/workspace/src/sidebar.rs` and `keybindings.rs` are in the same `workspace` crate; the sidebar may call `Action` methods directly (no new cross-crate dependency).

---

## File Structure

- `crates/workspace/src/keybindings.rs` — add 6 `Action` variants, their `label()`/`shortcut_hint()` arms, `match_shortcut` rows, `all_palette_actions()` entries, and a `#[cfg(test)] mod tests`.
- `alterm/src/main.rs` — add 6 `dispatch_action` arms; add a `WINDOWS` section and a `Search` row to `hotkey_info_view`.
- `crates/workspace/src/sidebar.rs` — add a `with_tooltip` helper and wrap all 7 buttons; add a `tooltip_box_style` container style.

---

## Task 1: Add new keybinding actions and shortcut matching

**Files:**
- Modify: `crates/workspace/src/keybindings.rs`
- Test: `crates/workspace/src/keybindings.rs` (inline `#[cfg(test)] mod tests`)

**Interfaces:**
- Consumes: existing `Action` enum, `match_shortcut(key: &Key, mods: &Modifiers) -> Option<Action>`, `all_palette_actions() -> Vec<Action>`.
- Produces: new variants `Action::NewTerminal`, `Action::NewBrowser`, `Action::NewPreview`, `Action::ShowHotkeyInfo`, `Action::ToggleTheme`, `Action::Search`, each with a working `label()`, `shortcut_hint()`, and (except none — all six) a `match_shortcut` mapping. These are consumed by Task 2 (`dispatch_action`), Task 3 (info pane), and Task 4 (sidebar tooltips).

- [ ] **Step 1: Write the failing test**

Add at the very end of `crates/workspace/src/keybindings.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn ctrl_shift(ch: &str) -> Option<Action> {
        let key = Key::Character(ch.into());
        let mods = Modifiers::CTRL | Modifiers::SHIFT;
        match_shortcut(&key, &mods)
    }

    #[test]
    fn new_window_shortcuts_match() {
        assert_eq!(ctrl_shift("n"), Some(Action::NewTerminal));
        assert_eq!(ctrl_shift("b"), Some(Action::NewBrowser));
        assert_eq!(ctrl_shift("o"), Some(Action::NewPreview));
        assert_eq!(ctrl_shift("h"), Some(Action::ShowHotkeyInfo));
        assert_eq!(ctrl_shift("l"), Some(Action::ToggleTheme));
        assert_eq!(ctrl_shift("f"), Some(Action::Search));
    }

    #[test]
    fn new_actions_have_hints_and_labels() {
        for action in [
            Action::NewTerminal,
            Action::NewBrowser,
            Action::NewPreview,
            Action::ShowHotkeyInfo,
            Action::ToggleTheme,
            Action::Search,
        ] {
            assert!(!action.shortcut_hint().is_empty());
            assert!(!action.label().is_empty());
        }
        assert_eq!(Action::NewPreview.shortcut_hint(), "Ctrl+Shift+O");
        assert_eq!(Action::Search.shortcut_hint(), "Ctrl+Shift+F");
    }

    #[test]
    fn new_actions_are_in_palette() {
        let actions = all_palette_actions();
        for a in [
            Action::NewTerminal,
            Action::NewBrowser,
            Action::NewPreview,
            Action::ShowHotkeyInfo,
            Action::ToggleTheme,
            Action::Search,
        ] {
            assert!(actions.contains(&a), "missing {a:?} in palette actions");
        }
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p workspace keybindings 2>&1 | tail -20`
Expected: compile error — `no variant named NewTerminal/NewBrowser/NewPreview/ShowHotkeyInfo/ToggleTheme/Search found for enum Action`.

- [ ] **Step 3: Add the six variants to the `Action` enum**

In `crates/workspace/src/keybindings.rs`, in `pub enum Action`, add after the `ToggleAIChat` line (within the `CommandPalette`/`OpenSettings`/`ToggleAIChat` group):

```rust
    NewTerminal,
    NewBrowser,
    NewPreview,
    ShowHotkeyInfo,
    ToggleTheme,
    Search,
```

- [ ] **Step 4: Add `label()` arms**

In `impl Action`'s `label()` match, add before the closing brace of the match:

```rust
            Action::NewTerminal => "New Terminal",
            Action::NewBrowser => "New Browser",
            Action::NewPreview => "New File Preview",
            Action::ShowHotkeyInfo => "Keyboard Shortcuts",
            Action::ToggleTheme => "Toggle Theme",
            Action::Search => "Search",
```

- [ ] **Step 5: Add `shortcut_hint()` arms**

In `shortcut_hint()`'s match, add before the closing brace:

```rust
            Action::NewTerminal => "Ctrl+Shift+N",
            Action::NewBrowser => "Ctrl+Shift+B",
            Action::NewPreview => "Ctrl+Shift+O",
            Action::ShowHotkeyInfo => "Ctrl+Shift+H",
            Action::ToggleTheme => "Ctrl+Shift+L",
            Action::Search => "Ctrl+Shift+F",
```

- [ ] **Step 6: Add `match_shortcut` letter rows**

In `match_shortcut`, inside the `if mods.control() && mods.shift()` block's `match ch.as_str()`, add these arms alongside the existing letters (before the `_ => {}` arm):

```rust
                "n" => return Some(Action::NewTerminal),
                "b" => return Some(Action::NewBrowser),
                "o" => return Some(Action::NewPreview),
                "h" => return Some(Action::ShowHotkeyInfo),
                "l" => return Some(Action::ToggleTheme),
                "f" => return Some(Action::Search),
```

- [ ] **Step 7: Add variants to `all_palette_actions()`**

In `all_palette_actions()`'s `vec![...]`, add before the closing `]`:

```rust
        Action::NewTerminal,
        Action::NewBrowser,
        Action::NewPreview,
        Action::ShowHotkeyInfo,
        Action::ToggleTheme,
        Action::Search,
```

- [ ] **Step 8: Run tests to verify they pass**

Run: `cargo test -p workspace keybindings 2>&1 | tail -20`
Expected: `test result: ok.` with `new_window_shortcuts_match`, `new_actions_have_hints_and_labels`, `new_actions_are_in_palette` passing.

Note: the build may now warn about a non-exhaustive `match` in `alterm/src/main.rs::dispatch_action` — that is expected and fixed in Task 2. `cargo test -p workspace` builds only the `workspace` crate, so it should pass cleanly here.

- [ ] **Step 9: Commit**

```bash
git add crates/workspace/src/keybindings.rs
git commit -m "feat: add keybindings for new-window sidebar actions and reserved search"
```

---

## Task 2: Route new actions through dispatch

**Files:**
- Modify: `alterm/src/main.rs` (`dispatch_action`, around lines 499–594)

**Interfaces:**
- Consumes: `Action::{NewTerminal, NewBrowser, NewPreview, ShowHotkeyInfo, ToggleTheme, Search}` from Task 1; existing `Message::{SidebarNewTerminal, OpenBrowser, OpenPreview, ShowHotkeyInfo, ToggleTheme}`.
- Produces: a buildable binary where pressing each new hotkey performs the same action as the corresponding sidebar button (and `Search` is a logged no-op).

- [ ] **Step 1: Confirm the build currently fails (non-exhaustive match)**

Run: `cargo build -p alterm 2>&1 | tail -20`
Expected: error `E0004` — non-exhaustive patterns: `Action::NewTerminal`, `Action::NewBrowser`, … not covered in `dispatch_action`.

- [ ] **Step 2: Add the dispatch arms**

In `fn dispatch_action`, add these arms before the closing `}` of the `match action {` (after the `Action::ToggleAIChat => ...` arm):

```rust
            Action::NewTerminal => self.update(Message::SidebarNewTerminal),
            Action::NewBrowser => self.update(Message::OpenBrowser),
            Action::NewPreview => self.update(Message::OpenPreview),
            Action::ShowHotkeyInfo => self.update(Message::ShowHotkeyInfo),
            Action::ToggleTheme => self.update(Message::ToggleTheme),
            Action::Search => {
                log::debug!("Search — not yet implemented");
                Task::none()
            }
```

- [ ] **Step 3: Verify the build succeeds**

Run: `cargo build -p alterm 2>&1 | tail -20`
Expected: builds with no errors (warnings about unused are fine).

- [ ] **Step 4: Manual verification**

Run: `cargo run -p alterm` (or the project's normal launch). Then:
- Press `Ctrl+Shift+N` → a new terminal pane opens.
- Press `Ctrl+Shift+B` → a browser pane opens.
- Press `Ctrl+Shift+O` → a file preview pane opens.
- Press `Ctrl+Shift+H` → the keyboard-shortcuts pane opens.
- Press `Ctrl+Shift+L` → theme toggles light/dark.
- Press `Ctrl+Shift+F` → nothing visible happens; with `RUST_LOG=debug` the log shows `Search — not yet implemented`.

- [ ] **Step 5: Commit**

```bash
git add alterm/src/main.rs
git commit -m "feat: dispatch new-window hotkeys and reserved search stub"
```

---

## Task 3: Surface new shortcuts in the hotkey reference pane

**Files:**
- Modify: `alterm/src/main.rs` (`hotkey_info_view`, around lines 2848–2975)

**Interfaces:**
- Consumes: `all_palette_actions()`, `build_section(category, actions, extra_rows, accent, shortcut_color, label_color)`, and the new `Action` variants.
- Produces: the Info pane shows a `WINDOWS` section listing the 5 new-window actions, and `Search` appears in the `TERMINAL` section.

- [ ] **Step 1: Add a `windows_actions` filter**

In `hotkey_info_view`, just after the existing `tool_actions` filter (the `matches!(a, Action::ToggleAIChat | Action::CommandPalette | Action::OpenSettings)` block), add:

```rust
    let windows_actions: Vec<&Action> = all_actions.iter().filter(|a| matches!(a,
        Action::NewTerminal | Action::NewBrowser | Action::NewPreview |
        Action::ShowHotkeyInfo | Action::ToggleTheme
    )).collect();
```

- [ ] **Step 2: Add `Search` to the `terminal_actions` filter**

Change the existing `terminal_actions` filter to include `Action::Search`:

```rust
    let terminal_actions: Vec<&Action> = all_actions.iter().filter(|a| matches!(a,
        Action::Copy | Action::Paste | Action::ScrollPageUp | Action::ScrollPageDown | Action::Search
    )).collect();
```

- [ ] **Step 3: Add the `WINDOWS` section to the items list**

After the `AI & TOOLS` `items.extend(build_section("AI & TOOLS", &tool_actions, ...))` call and before the `TERMINAL` section, add:

```rust
    // Windows section (new-block / tool-window shortcuts)
    items.extend(build_section(
        "WINDOWS",
        &windows_actions,
        &[],
        accent, shortcut_color, label_color,
    ));
```

- [ ] **Step 4: Verify the build succeeds**

Run: `cargo build -p alterm 2>&1 | tail -20`
Expected: builds with no errors.

- [ ] **Step 5: Manual verification**

Run the app, open the Info pane (`Ctrl+Shift+H` or the sidebar info button). Confirm:
- A `WINDOWS` heading lists: New Terminal `Ctrl+Shift+N`, New Browser `Ctrl+Shift+B`, New File Preview `Ctrl+Shift+O`, Keyboard Shortcuts `Ctrl+Shift+H`, Toggle Theme `Ctrl+Shift+L`.
- The `TERMINAL` section now includes a `Search  Ctrl+Shift+F` row.
- Open the command palette (`Ctrl+Shift+P`) and confirm the new commands are searchable (e.g. type "browser").

- [ ] **Step 6: Commit**

```bash
git add alterm/src/main.rs
git commit -m "feat: list new-window and search shortcuts in hotkey info pane"
```

---

## Task 4: Add hover tooltips to sidebar buttons

**Files:**
- Modify: `crates/workspace/src/sidebar.rs`

**Interfaces:**
- Consumes: `Action::{NewTerminal, NewAiChat→ToggleAIChat, NewBrowser, NewPreview, OpenSettings, ShowHotkeyInfo, ToggleTheme}` methods `label()`/`shortcut_hint()`; existing `sidebar_svg_button`, `sidebar_svg_button_with_icon_size`, `sidebar_button`, `sidebar_view`.
- Produces: each of the 7 sidebar buttons is wrapped in a left-positioned tooltip showing `"<label>  (<shortcut>)"`.

- [ ] **Step 1: Add imports**

At the top of `crates/workspace/src/sidebar.rs`, change the widget import line to add `tooltip`, and import `Action`. Current:

```rust
use iced::widget::{button, column, container, svg, text};
```

Replace with:

```rust
use iced::widget::{button, column, container, svg, text, tooltip};
use crate::keybindings::Action;
```

- [ ] **Step 2: Add the tooltip helper and box style**

Add these two functions near the other helpers in `sidebar.rs` (e.g. just below `sidebar_button`):

```rust
/// Tooltip text for a sidebar button: "Label  (Ctrl+Shift+X)".
fn tip_text(action: Action) -> String {
    format!("{}  ({})", action.label(), action.shortcut_hint())
}

/// Wrap a built sidebar button in a left-positioned hover tooltip.
fn with_tooltip<'a, M: 'a>(content: Element<'a, M>, hint: String) -> Element<'a, M> {
    tooltip(
        content,
        container(text(hint).size(12))
            .padding(Padding::from([4, 8]))
            .style(tooltip_box_style),
        tooltip::Position::Left,
    )
    .gap(6)
    .into()
}

/// Styled background box for sidebar tooltips (theme-aware).
fn tooltip_box_style(theme: &Theme) -> iced::widget::container::Style {
    let light = is_light_theme(theme);
    iced::widget::container::Style {
        background: Some(Background::Color(if light {
            Color::from_rgb(0.20, 0.20, 0.24)
        } else {
            Color::from_rgb(0.16, 0.16, 0.20)
        })),
        text_color: Some(Color::from_rgb(0.95, 0.95, 0.95)),
        border: Border {
            color: if light {
                Color::from_rgb(0.35, 0.35, 0.40)
            } else {
                Color::from_rgb(0.30, 0.30, 0.36)
            },
            width: 1.0,
            radius: 4.0.into(),
        },
        ..Default::default()
    }
}
```

- [ ] **Step 3: Wrap each button in `sidebar_view`**

In `sidebar_view`, wrap each button as it is built. Replace each button binding with a tooltipped version. For the SVG/text buttons, wrap the returned `Element`:

```rust
    let terminal_btn = with_tooltip(
        sidebar_svg_button(
            &theme_svg(include_bytes!("../../../assets/icons/sidebar/terminal.svg"), light_mode),
            Some(map(SidebarAction::NewTerminal)),
            btn_size,
        ),
        tip_text(Action::NewTerminal),
    );
    let ai_btn = with_tooltip(
        sidebar_button("AI", Some(map(SidebarAction::NewAiChat)), btn_size),
        tip_text(Action::ToggleAIChat),
    );
    let web_btn = with_tooltip(
        sidebar_svg_button(
            &theme_svg(include_bytes!("../../../assets/icons/sidebar/browser.svg"), light_mode),
            Some(map(SidebarAction::NewBrowser)),
            btn_size,
        ),
        tip_text(Action::NewBrowser),
    );
    let preview_btn = with_tooltip(
        sidebar_svg_button_with_icon_size(
            &theme_svg(include_bytes!("../../../assets/icons/sidebar/folder.svg"), light_mode),
            Some(map(SidebarAction::NewPreview)),
            btn_size,
            24.0,
        ),
        tip_text(Action::NewPreview),
    );
    let settings_btn = with_tooltip(
        sidebar_svg_button(
            &theme_svg(include_bytes!("../../../assets/icons/sidebar/settings-svgrepo-com.svg"), light_mode),
            Some(map(SidebarAction::OpenSettings)),
            btn_size,
        ),
        tip_text(Action::OpenSettings),
    );
    let info_btn = with_tooltip(
        sidebar_svg_button(
            &theme_svg(include_bytes!("../../../assets/icons/sidebar/info.svg"), light_mode),
            Some(map(SidebarAction::ShowHotkeyInfo)),
            btn_size,
        ),
        tip_text(Action::ShowHotkeyInfo),
    );
```

And for the theme button (keep the existing `theme_icon_bytes` logic above it), wrap it:

```rust
    let theme_btn = with_tooltip(
        sidebar_svg_button(
            &theme_svg(theme_icon_bytes, light_mode),
            Some(map(SidebarAction::ToggleTheme)),
            btn_size,
        ),
        tip_text(Action::ToggleTheme),
    );
```

Leave the `top_buttons` / `bottom_buttons` `column![...]` assembly unchanged — the bindings are now tooltipped `Element`s, which `column!` accepts.

- [ ] **Step 4: Verify the build succeeds**

Run: `cargo build -p alterm 2>&1 | tail -20`
Expected: builds with no errors. If the compiler complains that `Padding`, `Border`, `Background`, `Color`, or `Theme` are unused-or-missing, confirm they are already imported at the top of `sidebar.rs` (they are used by the existing styles) — no new import beyond Step 1 should be required.

- [ ] **Step 5: Manual verification**

Run the app. Hover over each sidebar button and confirm a tooltip box appears to the **left** of the button with the correct text:
- Terminal → `New Terminal  (Ctrl+Shift+N)`
- AI → `Toggle AI Chat  (Ctrl+Shift+A)`
- Browser → `New Browser  (Ctrl+Shift+B)`
- Preview (folder) → `New File Preview  (Ctrl+Shift+O)`
- Settings → `Open Settings  (Ctrl+Shift+,)`
- Info → `Keyboard Shortcuts  (Ctrl+Shift+H)`
- Theme → `Toggle Theme  (Ctrl+Shift+L)`

Toggle the theme and re-hover to confirm the tooltip box is readable in both light and dark modes.

- [ ] **Step 6: Commit**

```bash
git add crates/workspace/src/sidebar.rs
git commit -m "feat: add hover tooltips with hotkeys to sidebar buttons"
```

---

## Final verification

- [ ] Run `cargo test -p workspace 2>&1 | tail -20` → all tests pass.
- [ ] Run `cargo clippy --workspace 2>&1 | tail -30` → no new warnings introduced by these changes.
- [ ] Run `cargo build -p alterm` → clean build.
- [ ] Manual pass: every new hotkey works, the Info pane and command palette list them, and every sidebar button shows its tooltip in both themes.

---

## Spec coverage check

- Tooltips on all 7 buttons → Task 4. ✓
- New hotkeys for Terminal/Browser/Preview/Info/Theme → Tasks 1–2. ✓
- Preview = `Ctrl+Shift+O` → Task 1 (`shortcut_hint`, `match_shortcut`). ✓
- Reserve `Ctrl+Shift+F` for search (stub, no button) → Task 1 (`Search` variant), Task 2 (no-op dispatch), Task 3 (TERMINAL row). ✓
- Tooltip content `Name  (shortcut)`, left-positioned, styled box → Task 4. ✓
- Command palette includes new actions → automatic via `all_palette_actions` (Task 1) + existing palette wiring; verified in Task 3 Step 5. ✓
- Info pane WINDOWS section + Search in TERMINAL → Task 3. ✓
- Unit tests for `match_shortcut` of `Ctrl+Shift+{N,B,O,H,L,F}` and hints → Task 1. ✓
- Out of scope (real search, remappable keys, other buttons' tooltips) → not implemented. ✓
