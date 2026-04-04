# Phase 2: Workspace — Tabs, Splits, Drag-and-Drop

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Multi-pane tiling workspace with tabs, drag-to-split, resize handles, a widget sidebar, keyboard shortcuts, block zoom, and a command palette — turning the single-terminal Phase 1 into a full workspace.

**Architecture:** iced's built-in `pane_grid` widget provides the tiling layout, drag-and-drop, and resize handles out of the box. We wrap it with a tab system (hand-rolled tab bar with + button) and a widget sidebar. Each pane holds a "Block" — currently just terminals, but the Block abstraction allows AI chat, browser, and file preview to be added in later phases. A new `workspace` crate manages the Mux (tab/pane state tree).

**Tech Stack:** iced 0.14 (`pane_grid`, `canvas`, keyboard/mouse events), existing terminal/gpu-renderer crates

---

## File Structure

```
altermative/
├── alterm/src/
│   └── main.rs                    # Rewritten: App now manages workspace
├── crates/
│   ├── workspace/                 # NEW: workspace management
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs             # Public API
│   │       ├── block.rs           # Block enum (Terminal, placeholder for AI/Browser)
│   │       ├── tab.rs             # Tab struct (holds pane_grid::State)
│   │       ├── sidebar.rs         # Widget sidebar component
│   │       ├── tab_bar.rs         # Tab bar component
│   │       ├── command_palette.rs # Command palette overlay
│   │       └── keybindings.rs     # Keyboard shortcut registry
│   ├── terminal/src/              # Existing (minor updates)
│   └── gpu-renderer/src/          # Existing (minor updates)
```

---

### Task 1: Workspace Crate Scaffold + Block Abstraction

**Files:**
- Create: `crates/workspace/Cargo.toml`
- Create: `crates/workspace/src/lib.rs`
- Create: `crates/workspace/src/block.rs`
- Modify: `Cargo.toml` (add workspace member)
- Modify: `alterm/Cargo.toml` (add workspace dep)

- [ ] **Step 1: Create workspace crate Cargo.toml**

```toml
# crates/workspace/Cargo.toml
[package]
name = "workspace"
version.workspace = true
edition.workspace = true

[dependencies]
iced.workspace = true
terminal = { path = "../terminal", package = "terminal" }
gpu-renderer = { path = "../gpu-renderer", package = "gpu-renderer" }
log.workspace = true
```

- [ ] **Step 2: Define the Block abstraction**

```rust
// crates/workspace/src/block.rs
use terminal::{PtyHandle, TerminalState, TerminalEvent};
use gpu_renderer::colors::AnsiPalette;
use tokio::sync::mpsc;

/// A Block is the content unit inside a pane — currently only Terminal,
/// but will grow to include AI Chat, Browser, File Preview.
pub enum Block {
    Terminal {
        state: TerminalState,
        pty: PtyHandle,
        pty_rx: mpsc::Receiver<TerminalEvent>,
        palette: AnsiPalette,
        cursor_visible: bool,
        blink_count: u32,
    },
}

impl Block {
    pub fn new_terminal(rows: u16, cols: u16) -> Result<Self, String> {
        let state = TerminalState::new(rows as usize, cols as usize);
        let (pty, pty_rx) = PtyHandle::spawn(rows, cols)?;
        Ok(Block::Terminal {
            state,
            pty,
            pty_rx,
            palette: AnsiPalette::default(),
            cursor_visible: true,
            blink_count: 0,
        })
    }

    /// Drain PTY output and process it
    pub fn tick(&mut self) {
        if let Block::Terminal { state, pty_rx, cursor_visible, blink_count, .. } = self {
            // Drain PTY
            while let Ok(event) = pty_rx.try_recv() {
                match event {
                    TerminalEvent::PtyOutput(data) => state.process_output(&data),
                    _ => {}
                }
            }
            // Blink cursor
            *blink_count += 1;
            if *blink_count % 62 == 0 {
                *cursor_visible = !*cursor_visible;
            }
        }
    }

    /// Write input to the block's PTY
    pub fn write_input(&mut self, data: &[u8]) {
        if let Block::Terminal { pty, .. } = self {
            let _ = pty.write(data);
        }
    }

    /// Resize the block
    pub fn resize(&mut self, rows: u16, cols: u16) {
        if let Block::Terminal { state, pty, .. } = self {
            state.resize(rows as usize, cols as usize);
            let _ = pty.resize(rows, cols);
        }
    }

    /// Get a display title for the pane's title bar
    pub fn title(&self) -> String {
        match self {
            Block::Terminal { .. } => "Terminal".to_string(),
        }
    }
}
```

- [ ] **Step 3: Create lib.rs**

```rust
// crates/workspace/src/lib.rs
pub mod block;
pub use block::Block;
```

- [ ] **Step 4: Add workspace crate to Cargo.toml workspace members and alterm deps**

Add `"crates/workspace"` to `[workspace.members]` in root `Cargo.toml`.

Add to `alterm/Cargo.toml`:
```toml
workspace = { path = "../crates/workspace", package = "workspace" }
```

Also add `workspace` path dep to root `[workspace.dependencies]`.

- [ ] **Step 5: Verify it compiles**

Run: `cargo check --workspace`

- [ ] **Step 6: Commit**

```bash
git add -A && git commit -m "feat: workspace crate with Block abstraction"
```

---

### Task 2: Pane Grid Integration — Multi-Terminal Splits

**Files:**
- Modify: `alterm/src/main.rs`

Rewrite main.rs to use iced's `pane_grid` widget for tiling layout:

- [ ] **Step 1: Rewrite App to use pane_grid**

The App struct should now hold:
- `panes: pane_grid::State<Block>` — the tiling layout state with Block data per pane
- `focus: Option<pane_grid::Pane>` — which pane is focused

On startup, create one terminal pane:
```rust
let block = Block::new_terminal(24, 80).expect("Failed to spawn terminal");
let (panes, first_pane) = pane_grid::State::new(block);
```

The view function should use `PaneGrid::new()`:
```rust
fn view(&self) -> Element<Message> {
    let pane_grid = PaneGrid::new(&self.panes, |pane, block, is_maximized| {
        let title_bar = pane_grid::TitleBar::new(text(block.title()))
            .controls(view_controls(pane, self.panes.len(), is_maximized))
            .padding(4);

        let content = render_block(block); // renders terminal via TerminalView

        pane_grid::Content::new(content)
            .title_bar(title_bar)
    })
    .on_click(Message::PaneClicked)
    .on_drag(Message::PaneDragged)
    .on_resize(10, Message::PaneResized)
    .spacing(2)
    .width(Length::Fill)
    .height(Length::Fill);

    pane_grid.into()
}
```

- [ ] **Step 2: Add pane grid messages**

```rust
enum Message {
    Tick,
    KeyboardInput(Key, Modifiers),
    PaneClicked(pane_grid::Pane),
    PaneDragged(pane_grid::DragEvent),
    PaneResized(pane_grid::ResizeEvent),
    SplitHorizontal,
    SplitVertical,
    ClosePane,
    MaximizeToggle,
    Renderer(RendererMessage),
    PasteText(String),
}
```

- [ ] **Step 3: Handle pane grid events in update**

- `PaneClicked(pane)` → set focus
- `PaneDragged(DragEvent::Dropped { pane, target })` → `self.panes.drop(pane, target)`
- `PaneResized(ResizeEvent { split, ratio })` → `self.panes.resize(split, ratio)`
- `SplitHorizontal` → split focused pane vertically (new pane right), spawn new terminal
- `SplitVertical` → split focused pane horizontally (new pane below), spawn new terminal
- `ClosePane` → close focused pane (if more than 1 pane exists)
- `MaximizeToggle` → toggle maximize on focused pane

- [ ] **Step 4: Update Tick to process all panes**

```rust
Message::Tick => {
    for (_, block) in self.panes.iter_mut() {
        block.tick();
    }
}
```

- [ ] **Step 5: Route keyboard input to focused pane only**

```rust
Message::KeyboardInput(key, mods) => {
    if let Some(focused) = self.focus {
        if let Some(block) = self.panes.get_mut(focused) {
            if let Some(bytes) = key_to_bytes(&key, &mods) {
                block.write_input(&bytes);
            }
        }
    }
}
```

- [ ] **Step 6: Add keyboard shortcuts for splits**

In the KeyboardInput handler, check for:
- `Ctrl+Shift+D` → SplitHorizontal (new pane right)
- `Ctrl+Shift+E` → SplitVertical (new pane below)
- `Ctrl+Shift+X` → ClosePane
- `Ctrl+Shift+Z` → MaximizeToggle
- `Ctrl+Shift+Arrow` → navigate between panes using `self.panes.adjacent()`

- [ ] **Step 7: Verify it compiles and runs**

Run: `cargo run --bin alterm`

Expected: Window opens with one terminal pane. Press Ctrl+Shift+D to split, see two independent terminals. Drag title bars to rearrange. Drag split borders to resize.

- [ ] **Step 8: Commit**

```bash
git add -A && git commit -m "feat: pane_grid tiling layout with splits, drag-drop, resize"
```

---

### Task 3: Tab Bar

**Files:**
- Create: `crates/workspace/src/tab.rs`
- Create: `crates/workspace/src/tab_bar.rs`
- Modify: `crates/workspace/src/lib.rs`
- Modify: `alterm/src/main.rs`

- [ ] **Step 1: Define Tab struct**

```rust
// crates/workspace/src/tab.rs
use iced::widget::pane_grid;
use crate::block::Block;

pub struct Tab {
    pub title: String,
    pub panes: pane_grid::State<Block>,
    pub focus: Option<pane_grid::Pane>,
}

impl Tab {
    pub fn new() -> Result<Self, String> {
        let block = Block::new_terminal(24, 80)?;
        let (panes, first_pane) = pane_grid::State::new(block);
        Ok(Tab {
            title: "Terminal".to_string(),
            panes,
            focus: Some(first_pane),
        })
    }
}
```

- [ ] **Step 2: Define TabBar view component**

```rust
// crates/workspace/src/tab_bar.rs
use iced::widget::{button, container, row, text, Space};
use iced::{Element, Length, Theme};

pub fn tab_bar_view<'a, Message: Clone + 'a>(
    tabs: &[TabInfo],
    active: usize,
    on_select: impl Fn(usize) -> Message + 'a,
    on_close: impl Fn(usize) -> Message + 'a,
    on_new: Message,
) -> Element<'a, Message> {
    // Build row of tab buttons + "+" button
    // Active tab highlighted, each has an X close button
    // "+" button at the end creates new tab
}

pub struct TabInfo {
    pub title: String,
}
```

Build a horizontal row of styled buttons. Active tab gets a distinct background color. Each tab shows title + close (X) button. Plus button at the end.

- [ ] **Step 3: Restructure App to manage multiple tabs**

The App now holds:
```rust
struct App {
    tabs: Vec<Tab>,
    active_tab: usize,
}
```

Messages get new variants:
```rust
SelectTab(usize),
CloseTab(usize),
NewTab,
RenameTab,  // F2
```

View becomes:
```rust
fn view(&self) -> Element<Message> {
    let tab_bar = tab_bar_view(...);
    let active_pane_grid = /* render active tab's pane_grid */;
    column![tab_bar, active_pane_grid].into()
}
```

- [ ] **Step 4: Add tab keyboard shortcuts**

- `Ctrl+Shift+T` → new tab
- `Ctrl+Shift+W` → close tab
- `Ctrl+Tab` / `Ctrl+Shift+Tab` → next/previous tab
- `Ctrl+1-9` → jump to tab N
- `F2` → rename tab (basic: just toggle an editable text input)

- [ ] **Step 5: Verify tabs work**

Run: `cargo run --bin alterm`

Expected: Tab bar at top. Ctrl+Shift+T creates new tabs. Each tab has its own independent pane layout. Clicking tabs switches. X button closes. Plus button creates.

- [ ] **Step 6: Commit**

```bash
git add -A && git commit -m "feat: tab bar with new/close/switch tabs"
```

---

### Task 4: Widget Sidebar

**Files:**
- Create: `crates/workspace/src/sidebar.rs`
- Modify: `alterm/src/main.rs`

- [ ] **Step 1: Implement sidebar**

A vertical bar on the right edge of the window with icon buttons:

```rust
// crates/workspace/src/sidebar.rs
use iced::widget::{button, column, container, text, tooltip};
use iced::{Element, Length};

pub fn sidebar_view<'a, Message: Clone + 'a>(
    on_new_terminal: Message,
    on_new_ai: Message,      // placeholder for Phase 3
    on_new_browser: Message,  // placeholder for Phase 4
    on_settings: Message,     // placeholder
) -> Element<'a, Message> {
    let terminal_btn = tooltip(
        button(text("T").size(16)).width(36).height(36).on_press(on_new_terminal),
        "New Terminal",
        tooltip::Position::Left,
    );

    let ai_btn = tooltip(
        button(text("AI").size(12)).width(36).height(36),  // no on_press yet
        "AI Chat (coming soon)",
        tooltip::Position::Left,
    );

    let browser_btn = tooltip(
        button(text("W").size(16)).width(36).height(36),  // no on_press yet
        "Web Browser (coming soon)",
        tooltip::Position::Left,
    );

    let settings_btn = tooltip(
        button(text("⚙").size(16)).width(36).height(36),  // no on_press yet
        "Settings (coming soon)",
        tooltip::Position::Left,
    );

    container(
        column![terminal_btn, ai_btn, browser_btn, settings_btn]
            .spacing(8)
            .padding(8)
    )
    .width(52)
    .height(Length::Fill)
    .into()
}
```

- [ ] **Step 2: Add sidebar to main layout**

```rust
fn view(&self) -> Element<Message> {
    let tab_bar = ...;
    let pane_grid = ...;
    let sidebar = sidebar_view(Message::NewTerminalBlock, ...);

    column![
        tab_bar,
        row![pane_grid, sidebar]
    ].into()
}
```

- [ ] **Step 3: Handle NewTerminalBlock message**

Clicking the Terminal button in the sidebar splits the focused pane (or creates a new pane if there's only one) with a new terminal.

- [ ] **Step 4: Verify and commit**

```bash
git add -A && git commit -m "feat: widget sidebar with terminal button"
```

---

### Task 5: Pane Navigation Keyboard Shortcuts

**Files:**
- Create: `crates/workspace/src/keybindings.rs`
- Modify: `alterm/src/main.rs`

- [ ] **Step 1: Define keybinding registry**

```rust
// crates/workspace/src/keybindings.rs
use iced::keyboard::{Key, Modifiers, key::Named};

pub enum Action {
    // Tabs
    NewTab,
    CloseTab,
    NextTab,
    PrevTab,
    JumpToTab(usize),
    RenameTab,
    // Panes
    SplitRight,
    SplitDown,
    ClosePane,
    MaximizeToggle,
    FocusUp,
    FocusDown,
    FocusLeft,
    FocusRight,
    // General
    CommandPalette,
    OpenSettings,
    // Clipboard
    Copy,
    Paste,
}

/// Check if a key+modifiers combo matches a known shortcut
pub fn match_shortcut(key: &Key, mods: &Modifiers) -> Option<Action> {
    let ctrl_shift = mods.control() && mods.shift();
    let ctrl = mods.control() && !mods.shift();

    match key {
        // Ctrl+Shift shortcuts
        Key::Character(c) if ctrl_shift => match c.as_str() {
            "t" | "T" => Some(Action::NewTab),
            "w" | "W" => Some(Action::CloseTab),
            "d" | "D" => Some(Action::SplitRight),
            "e" | "E" => Some(Action::SplitDown),
            "x" | "X" => Some(Action::ClosePane),
            "z" | "Z" => Some(Action::MaximizeToggle),
            "p" | "P" => Some(Action::CommandPalette),
            "c" | "C" => Some(Action::Copy),
            "v" | "V" => Some(Action::Paste),
            "," => Some(Action::OpenSettings),
            _ => None,
        },
        // Ctrl+Tab / Ctrl+Shift+Tab
        Key::Named(Named::Tab) if ctrl_shift => Some(Action::PrevTab),
        Key::Named(Named::Tab) if ctrl => Some(Action::NextTab),
        // Ctrl+1-9
        Key::Character(c) if ctrl => {
            if let Some(n) = c.chars().next().and_then(|ch| ch.to_digit(10)) {
                if n >= 1 && n <= 9 {
                    return Some(Action::JumpToTab(n as usize - 1));
                }
            }
            None
        },
        // Ctrl+Shift+Arrow for pane navigation
        Key::Named(Named::ArrowUp) if ctrl_shift => Some(Action::FocusUp),
        Key::Named(Named::ArrowDown) if ctrl_shift => Some(Action::FocusDown),
        Key::Named(Named::ArrowLeft) if ctrl_shift => Some(Action::FocusLeft),
        Key::Named(Named::ArrowRight) if ctrl_shift => Some(Action::FocusRight),
        // F2
        Key::Named(Named::F2) => Some(Action::RenameTab),
        _ => None,
    }
}
```

- [ ] **Step 2: Use keybinding registry in main.rs**

Replace the inline shortcut checks with:
```rust
if let Some(action) = keybindings::match_shortcut(&key, &mods) {
    match action {
        Action::NewTab => { /* create new tab */ }
        Action::SplitRight => { /* split focused pane */ }
        Action::FocusRight => {
            if let Some(focused) = self.active_tab().focus {
                if let Some(neighbor) = self.active_tab().panes.adjacent(focused, Direction::Right) {
                    self.active_tab_mut().focus = Some(neighbor);
                }
            }
        }
        // ... handle all actions
        _ => {}
    }
} else {
    // Not a shortcut — send to focused terminal
    if let Some(bytes) = key_to_bytes(&key, &mods) { ... }
}
```

- [ ] **Step 3: Verify and commit**

```bash
git add -A && git commit -m "feat: keyboard shortcuts for tabs, panes, navigation"
```

---

### Task 6: Command Palette

**Files:**
- Create: `crates/workspace/src/command_palette.rs`
- Modify: `alterm/src/main.rs`

- [ ] **Step 1: Implement command palette overlay**

A simple overlay that appears on Ctrl+Shift+P:
- Text input field at the top
- Filtered list of available commands below
- Keyboard navigation (arrow keys, Enter to execute, Escape to close)
- Fuzzy matching on command names

```rust
// crates/workspace/src/command_palette.rs
pub struct CommandPalette {
    pub visible: bool,
    pub query: String,
    pub commands: Vec<Command>,
    pub filtered: Vec<usize>,  // indices into commands
    pub selected: usize,
}

pub struct Command {
    pub name: String,
    pub shortcut: Option<String>,
    pub action: Action,
}

impl CommandPalette {
    pub fn new() -> Self {
        let commands = vec![
            Command { name: "New Tab".into(), shortcut: Some("Ctrl+Shift+T".into()), action: Action::NewTab },
            Command { name: "Close Tab".into(), shortcut: Some("Ctrl+Shift+W".into()), action: Action::CloseTab },
            Command { name: "Split Right".into(), shortcut: Some("Ctrl+Shift+D".into()), action: Action::SplitRight },
            Command { name: "Split Down".into(), shortcut: Some("Ctrl+Shift+E".into()), action: Action::SplitDown },
            Command { name: "Close Pane".into(), shortcut: Some("Ctrl+Shift+X".into()), action: Action::ClosePane },
            Command { name: "Toggle Maximize".into(), shortcut: Some("Ctrl+Shift+Z".into()), action: Action::MaximizeToggle },
            // ... more commands
        ];
        let filtered = (0..commands.len()).collect();
        Self { visible: false, query: String::new(), commands, filtered, selected: 0 }
    }

    pub fn toggle(&mut self) { self.visible = !self.visible; self.query.clear(); self.refilter(); self.selected = 0; }
    pub fn update_query(&mut self, q: String) { self.query = q; self.refilter(); self.selected = 0; }
    pub fn select_next(&mut self) { if self.selected + 1 < self.filtered.len() { self.selected += 1; } }
    pub fn select_prev(&mut self) { if self.selected > 0 { self.selected -= 1; } }
    pub fn execute(&mut self) -> Option<Action> {
        self.filtered.get(self.selected).map(|&i| { self.visible = false; self.commands[i].action.clone() })
    }

    fn refilter(&mut self) {
        let q = self.query.to_lowercase();
        self.filtered = self.commands.iter().enumerate()
            .filter(|(_, cmd)| q.is_empty() || cmd.name.to_lowercase().contains(&q))
            .map(|(i, _)| i)
            .collect();
    }
}
```

- [ ] **Step 2: Render command palette as an overlay in view**

When `command_palette.visible` is true, render a centered overlay on top of the pane grid:
- A text_input for the search query
- A scrollable column of command buttons
- Selected command highlighted

Use `iced::widget::stack` or `container` with absolute positioning to overlay on top of the pane grid.

- [ ] **Step 3: Handle command palette input**

When the palette is visible, intercept keyboard events:
- Arrow Up/Down → navigate
- Enter → execute selected command
- Escape → close palette
- Any character → update search query

- [ ] **Step 4: Verify and commit**

```bash
git add -A && git commit -m "feat: command palette with fuzzy search (Ctrl+Shift+P)"
```

---

### Task 7: Pane Title Bar Controls

**Files:**
- Modify: `alterm/src/main.rs`

- [ ] **Step 1: Add control buttons to pane title bars**

In the `PaneGrid::new()` view function, add controls to each `TitleBar`:

```rust
let title_bar = pane_grid::TitleBar::new(text(block.title()))
    .controls(pane_controls(pane, total_panes, is_maximized))
    .padding(4);
```

The `pane_controls` function returns buttons:
- Split horizontal button (icon: "|")
- Split vertical button (icon: "—")
- Maximize/restore toggle (icon: "□" / "◱")
- Close button (icon: "×") — only if more than one pane

Each button sends the appropriate message with the pane ID.

- [ ] **Step 2: Style the title bar**

- Focused pane: slightly brighter title bar background
- Unfocused pane: dimmer
- Controls appear on hover (or always visible — simpler for beginners)

- [ ] **Step 3: Verify and commit**

```bash
git add -A && git commit -m "feat: pane title bar with split, maximize, close controls"
```

---

### Task 8: Polish — Focus Indicator, Styling, Final Verification

**Files:**
- Modify: `alterm/src/main.rs`
- Modify: various workspace crate files

- [ ] **Step 1: Visual focus indicator**

The focused pane should have a visible border or highlight so the user always knows where keyboard input goes. Use `pane_grid::Content::style()` to set a distinct border color for the focused pane.

- [ ] **Step 2: Tab bar styling**

- Active tab: brighter background, underline indicator
- Inactive tabs: dimmer, hover highlight
- Close button (X) appears on hover
- Plus button styled consistently

- [ ] **Step 3: Sidebar styling**

- Dark background matching the theme
- Button hover effects
- Tooltips visible on hover

- [ ] **Step 4: Final verification**

Run `cargo run --bin alterm` and verify:
1. Window opens with tab bar, one terminal pane, and sidebar
2. Ctrl+Shift+T creates new tabs, each with its own pane layout
3. Ctrl+Shift+D splits the focused pane (new terminal on the right)
4. Ctrl+Shift+E splits down (new terminal below)
5. Drag pane title bars to rearrange
6. Drag split borders to resize
7. Ctrl+Shift+Z maximizes/restores pane
8. Ctrl+Shift+Arrow navigates between panes
9. Ctrl+Shift+P opens command palette
10. Clicking sidebar Terminal button creates a new terminal pane
11. Each terminal is independent (different shell sessions)
12. Keyboard input goes to the focused pane only
13. Tabs can be closed, panes can be closed

- [ ] **Step 5: Commit**

```bash
git add -A && git commit -m "feat: workspace styling and focus indicators, Phase 2 complete"
```

---

## Phase 2 Completion Checklist

- [ ] Tab bar with new tab (+), close (X), click to switch
- [ ] Tab keyboard shortcuts (Ctrl+Shift+T/W, Ctrl+Tab, Ctrl+1-9, F2)
- [ ] Pane grid with split/close/resize/drag-drop via `pane_grid`
- [ ] Split keyboard shortcuts (Ctrl+Shift+D/E)
- [ ] Pane navigation (Ctrl+Shift+Arrow)
- [ ] Block zoom/maximize (Ctrl+Shift+Z)
- [ ] Widget sidebar with Terminal button
- [ ] Command palette (Ctrl+Shift+P)
- [ ] Pane title bar with split/maximize/close controls
- [ ] Focus indicator on active pane
- [ ] Each pane has independent terminal session
- [ ] Keyboard input routes to focused pane only

## Notes for Phase 3

- The Block enum will gain `AIChat` and `Browser` variants
- The sidebar buttons for AI/Browser will get `on_press` handlers
- The settings button will open the settings panel
- The command palette will gain AI-related commands
- Tab titles should update from terminal escape sequences
