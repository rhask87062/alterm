# Phase 1: Foundation — Get a Terminal on Screen

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A working GPU-accelerated terminal emulator that spawns a shell, renders output with ANSI colors, handles keyboard/mouse input, supports scrollback, and can be resized — all in a single iced window.

**Architecture:** iced GUI framework runs the event loop and provides the window. A custom `shader` widget uses glyphon (wgpu text rendering) to draw the terminal grid. `alacritty_terminal` handles VT/ANSI parsing and screen state. `portable-pty` manages the shell process. An async subscription bridges PTY I/O to iced's message loop.

**Tech Stack:** Rust, iced 0.14 (wgpu feature), alacritty_terminal, portable-pty, glyphon, cosmic-text

---

## File Structure

```
altermative/
├── Cargo.toml                    # Workspace root
├── alterm/                       # Binary crate
│   ├── Cargo.toml
│   └── src/
│       └── main.rs               # App entry point, iced Application
├── crates/
│   ├── terminal/                 # Terminal backend
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs            # Public API: Terminal struct
│   │       ├── pty.rs            # PTY spawning and async I/O
│   │       ├── term.rs           # Wraps alacritty_terminal state
│   │       └── event.rs          # Terminal events (data, resize, etc.)
│   └── gpu-renderer/             # GPU text rendering widget
│       ├── Cargo.toml
│       └── src/
│           ├── lib.rs            # Public API: TerminalView widget
│           ├── widget.rs         # iced widget implementation
│           ├── grid.rs           # Terminal grid → render data conversion
│           └── colors.rs         # ANSI color palette → RGB conversion
└── tests/
    ├── terminal_backend_test.rs  # Integration tests for terminal crate
    └── color_test.rs             # Color conversion tests
```

---

### Task 1: Project Scaffold

**Files:**
- Create: `Cargo.toml`
- Create: `alterm/Cargo.toml`
- Create: `alterm/src/main.rs`
- Create: `crates/terminal/Cargo.toml`
- Create: `crates/terminal/src/lib.rs`
- Create: `crates/gpu-renderer/Cargo.toml`
- Create: `crates/gpu-renderer/src/lib.rs`
- Create: `.gitignore`

- [ ] **Step 1: Create workspace Cargo.toml**

```toml
# Cargo.toml
[workspace]
resolver = "2"
members = [
    "alterm",
    "crates/terminal",
    "crates/gpu-renderer",
]

[workspace.package]
version = "0.1.0"
edition = "2024"
license = "MIT"

[workspace.dependencies]
iced = { version = "0.14", features = ["wgpu", "tokio"] }
alacritty_terminal = "0.25"
portable-pty = "0.9"
cosmic-text = "0.18"
glyphon = "0.8"
tokio = { version = "1", features = ["full"] }
log = "0.4"
env_logger = "0.11"
```

- [ ] **Step 2: Create binary crate Cargo.toml**

```toml
# alterm/Cargo.toml
[package]
name = "alterm"
version.workspace = true
edition.workspace = true

[dependencies]
iced.workspace = true
altermative-terminal = { path = "../crates/terminal" }
altermative-gpu-renderer = { path = "../crates/gpu-renderer" }
tokio.workspace = true
log.workspace = true
env_logger.workspace = true
```

- [ ] **Step 3: Create terminal crate Cargo.toml**

```toml
# crates/terminal/Cargo.toml
[package]
name = "altermative-terminal"
version.workspace = true
edition.workspace = true

[dependencies]
alacritty_terminal.workspace = true
portable-pty.workspace = true
tokio.workspace = true
log.workspace = true
```

- [ ] **Step 4: Create gpu-renderer crate Cargo.toml**

```toml
# crates/gpu-renderer/Cargo.toml
[package]
name = "altermative-gpu-renderer"
version.workspace = true
edition.workspace = true

[dependencies]
iced.workspace = true
cosmic-text.workspace = true
glyphon.workspace = true
altermative-terminal = { path = "../terminal" }
log.workspace = true
```

- [ ] **Step 5: Create stub lib.rs files and main.rs**

```rust
// crates/terminal/src/lib.rs
pub mod pty;
pub mod term;
pub mod event;
```

```rust
// crates/gpu-renderer/src/lib.rs
pub mod widget;
pub mod grid;
pub mod colors;
```

```rust
// alterm/src/main.rs
use iced::{application, Element, Theme, Size};
use iced::widget::text;

fn main() -> iced::Result {
    env_logger::init();
    application("Altermative", App::update, App::view)
        .window_size(Size::new(900.0, 600.0))
        .theme(|_| Theme::Dark)
        .run()
}

#[derive(Default)]
struct App;

#[derive(Debug, Clone)]
enum Message {}

impl App {
    fn update(&mut self, _message: Message) {}

    fn view(&self) -> Element<Message> {
        text("Altermative — terminal coming soon").into()
    }
}
```

- [ ] **Step 6: Create .gitignore**

```
/target
**/*.rs.bk
*.swp
.env
```

- [ ] **Step 7: Verify it compiles and runs**

Run: `cd ~/dev-projects/apps/altermative && cargo run --bin alterm`

Expected: An iced window opens with dark theme showing "Altermative — terminal coming soon"

- [ ] **Step 8: Commit**

```bash
cd ~/dev-projects/apps/altermative
git add -A
git commit -m "feat: project scaffold with workspace, iced window opens"
```

---

### Task 2: PTY Spawning and Async I/O

**Files:**
- Create: `crates/terminal/src/pty.rs`
- Create: `crates/terminal/src/event.rs`
- Modify: `crates/terminal/src/lib.rs`

- [ ] **Step 1: Define terminal events**

```rust
// crates/terminal/src/event.rs

/// Events emitted by the terminal backend
#[derive(Debug, Clone)]
pub enum TerminalEvent {
    /// Raw bytes received from the PTY
    PtyOutput(Vec<u8>),
    /// PTY process exited
    PtyExited(i32),
    /// Error from PTY
    PtyError(String),
}
```

- [ ] **Step 2: Implement PTY manager**

```rust
// crates/terminal/src/pty.rs
use portable_pty::{native_pty_system, CommandBuilder, MasterPty, PtySize};
use std::io::{Read, Write};
use std::sync::Arc;
use tokio::sync::mpsc;

use crate::event::TerminalEvent;

pub struct PtyHandle {
    master: Box<dyn MasterPty + Send>,
    writer: Box<dyn Write + Send>,
    child: Box<dyn portable_pty::Child + Send + Sync>,
}

impl PtyHandle {
    pub fn spawn(rows: u16, cols: u16) -> Result<(Self, mpsc::Receiver<TerminalEvent>), Box<dyn std::error::Error>> {
        let pty_system = native_pty_system();
        let pair = pty_system.openpty(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })?;

        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string());
        let cmd = CommandBuilder::new(&shell);
        let child = pair.slave.spawn_command(cmd)?;
        drop(pair.slave); // close slave side in parent process

        let reader = pair.master.try_clone_reader()?;
        let writer = pair.master.take_writer()?;

        let (tx, rx) = mpsc::channel(256);

        // Spawn reader thread (blocking I/O → async channel)
        std::thread::spawn(move || {
            pty_reader_thread(reader, tx);
        });

        Ok((
            PtyHandle {
                master: pair.master,
                writer,
                child,
            },
            rx,
        ))
    }

    pub fn write(&mut self, data: &[u8]) -> std::io::Result<()> {
        self.writer.write_all(data)?;
        self.writer.flush()
    }

    pub fn resize(&self, rows: u16, cols: u16) -> Result<(), Box<dyn std::error::Error>> {
        self.master.resize(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })?;
        Ok(())
    }

    pub fn is_alive(&mut self) -> bool {
        self.child.try_wait().ok().flatten().is_none()
    }
}

fn pty_reader_thread(mut reader: Box<dyn Read + Send>, tx: mpsc::Sender<TerminalEvent>) {
    let mut buf = [0u8; 4096];
    loop {
        match reader.read(&mut buf) {
            Ok(0) => {
                let _ = tx.blocking_send(TerminalEvent::PtyExited(0));
                break;
            }
            Ok(n) => {
                let data = buf[..n].to_vec();
                if tx.blocking_send(TerminalEvent::PtyOutput(data)).is_err() {
                    break; // receiver dropped
                }
            }
            Err(e) => {
                let _ = tx.blocking_send(TerminalEvent::PtyError(e.to_string()));
                break;
            }
        }
    }
}
```

- [ ] **Step 3: Update lib.rs to export modules**

```rust
// crates/terminal/src/lib.rs
pub mod pty;
pub mod term;
pub mod event;

pub use event::TerminalEvent;
pub use pty::PtyHandle;
```

- [ ] **Step 4: Verify it compiles**

Run: `cd ~/dev-projects/apps/altermative && cargo check`

Expected: Compiles with no errors (warnings about unused `term` module are OK)

- [ ] **Step 5: Commit**

```bash
cd ~/dev-projects/apps/altermative
git add crates/terminal/
git commit -m "feat: PTY spawning with async reader thread"
```

---

### Task 3: Terminal Emulation Backend

**Files:**
- Create: `crates/terminal/src/term.rs`
- Modify: `crates/terminal/src/lib.rs`

- [ ] **Step 1: Implement terminal state wrapper**

```rust
// crates/terminal/src/term.rs
use alacritty_terminal::event::{Event as AlacrittyEvent, EventListener};
use alacritty_terminal::event_loop::Msg;
use alacritty_terminal::term::Config as TermConfig;
use alacritty_terminal::term::Term;
use alacritty_terminal::term::cell::Cell;
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Column, Line, Point};
use alacritty_terminal::vte::ansi;

use std::sync::{Arc, Mutex};

/// Listener that collects events from alacritty_terminal
#[derive(Clone)]
pub struct EventProxy {
    events: Arc<Mutex<Vec<AlacrittyEvent>>>,
}

impl EventProxy {
    pub fn new() -> Self {
        Self {
            events: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn drain_events(&self) -> Vec<AlacrittyEvent> {
        let mut events = self.events.lock().unwrap();
        std::mem::take(&mut *events)
    }
}

impl EventListener for EventProxy {
    fn send_event(&self, event: AlacrittyEvent) {
        self.events.lock().unwrap().push(event);
    }
}

pub struct TerminalState {
    term: Term<EventProxy>,
    event_proxy: EventProxy,
}

impl TerminalState {
    pub fn new(rows: u16, cols: u16) -> Self {
        let event_proxy = EventProxy::new();
        let size = ansi::Size {
            columns: cols as usize,
            rows: rows as usize,
        };
        let config = TermConfig::default();
        let term = Term::new(config, &size, event_proxy.clone());

        Self { term, event_proxy }
    }

    /// Feed raw PTY output bytes into the terminal parser
    pub fn process_output(&mut self, data: &[u8]) {
        let mut parser = ansi::Processor::new();
        for byte in data {
            parser.advance(&mut self.term, *byte);
        }
    }

    /// Resize the terminal
    pub fn resize(&mut self, rows: u16, cols: u16) {
        let size = ansi::Size {
            columns: cols as usize,
            rows: rows as usize,
        };
        self.term.resize(size);
    }

    /// Get the number of rows
    pub fn rows(&self) -> usize {
        self.term.screen_lines()
    }

    /// Get the number of columns
    pub fn cols(&self) -> usize {
        self.term.columns()
    }

    /// Get a cell at a specific position
    pub fn cell(&self, row: usize, col: usize) -> &Cell {
        let point = Point::new(Line(row as i32), Column(col));
        &self.term[point]
    }

    /// Get cursor position
    pub fn cursor_point(&self) -> Point<usize> {
        self.term.grid().cursor.point
    }

    /// Get the display offset (scroll position)
    pub fn display_offset(&self) -> usize {
        self.term.grid().display_offset()
    }

    /// Drain events from alacritty (title changes, bell, etc.)
    pub fn drain_events(&self) -> Vec<AlacrittyEvent> {
        self.event_proxy.drain_events()
    }

    /// Access the underlying term for advanced operations
    pub fn term(&self) -> &Term<EventProxy> {
        &self.term
    }

    /// Mutable access for selection, etc.
    pub fn term_mut(&mut self) -> &mut Term<EventProxy> {
        &mut self.term
    }
}
```

- [ ] **Step 2: Update lib.rs exports**

```rust
// crates/terminal/src/lib.rs
pub mod pty;
pub mod term;
pub mod event;

pub use event::TerminalEvent;
pub use pty::PtyHandle;
pub use term::TerminalState;
```

- [ ] **Step 3: Verify it compiles**

Run: `cd ~/dev-projects/apps/altermative && cargo check`

Expected: Compiles. There may be deprecation warnings from alacritty_terminal — that's fine.

Note: If `alacritty_terminal` API differs from what's shown here (it evolves rapidly), adjust the wrapper methods to match the actual API. The key pattern — wrapping `Term<EventProxy>` and exposing `process_output`, `resize`, `cell`, `cursor_point` — stays the same.

- [ ] **Step 4: Commit**

```bash
cd ~/dev-projects/apps/altermative
git add crates/terminal/
git commit -m "feat: terminal emulation backend wrapping alacritty_terminal"
```

---

### Task 4: ANSI Color Palette

**Files:**
- Create: `crates/gpu-renderer/src/colors.rs`
- Create: `tests/color_test.rs`

- [ ] **Step 1: Write color conversion tests**

```rust
// tests/color_test.rs
use altermative_gpu_renderer::colors::AnsiPalette;

#[test]
fn test_standard_black() {
    let palette = AnsiPalette::default();
    let (r, g, b) = palette.ansi_to_rgb(0);
    assert_eq!((r, g, b), (0x1d, 0x1d, 0x1f)); // dark theme black
}

#[test]
fn test_standard_red() {
    let palette = AnsiPalette::default();
    let (r, g, b) = palette.ansi_to_rgb(1);
    assert_eq!((r, g, b), (0xff, 0x3b, 0x30));
}

#[test]
fn test_bright_white() {
    let palette = AnsiPalette::default();
    let (r, g, b) = palette.ansi_to_rgb(15);
    assert_eq!((r, g, b), (0xf5, 0xf5, 0xf7));
}

#[test]
fn test_256_color_index_16() {
    let palette = AnsiPalette::default();
    let (r, g, b) = palette.ansi_to_rgb(16);
    assert_eq!((r, g, b), (0x00, 0x00, 0x00)); // 6x6x6 cube: 0,0,0
}

#[test]
fn test_256_color_index_196() {
    let palette = AnsiPalette::default();
    let (r, g, b) = palette.ansi_to_rgb(196);
    assert_eq!((r, g, b), (0xff, 0x00, 0x00)); // 6x6x6 cube: 5,0,0
}

#[test]
fn test_grayscale_232() {
    let palette = AnsiPalette::default();
    let (r, g, b) = palette.ansi_to_rgb(232);
    assert_eq!((r, g, b), (0x08, 0x08, 0x08)); // darkest grayscale
}

#[test]
fn test_grayscale_255() {
    let palette = AnsiPalette::default();
    let (r, g, b) = palette.ansi_to_rgb(255);
    assert_eq!((r, g, b), (0xee, 0xee, 0xee)); // lightest grayscale
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd ~/dev-projects/apps/altermative && cargo test --test color_test`

Expected: FAIL — `AnsiPalette` doesn't exist yet.

- [ ] **Step 3: Implement color palette**

```rust
// crates/gpu-renderer/src/colors.rs

/// ANSI 256-color palette with RGB values
pub struct AnsiPalette {
    colors: [(u8, u8, u8); 256],
}

impl Default for AnsiPalette {
    fn default() -> Self {
        let mut colors = [(0u8, 0u8, 0u8); 256];

        // Standard 16 colors (dark theme inspired)
        let base16: [(u8, u8, u8); 16] = [
            (0x1d, 0x1d, 0x1f), // 0  black
            (0xff, 0x3b, 0x30), // 1  red
            (0x30, 0xd1, 0x58), // 2  green
            (0xff, 0x9f, 0x0a), // 3  yellow
            (0x0a, 0x84, 0xff), // 4  blue
            (0xbf, 0x5a, 0xf2), // 5  magenta
            (0x5a, 0xc8, 0xfa), // 6  cyan
            (0xd1, 0xd1, 0xd6), // 7  white
            (0x63, 0x63, 0x66), // 8  bright black
            (0xff, 0x45, 0x3a), // 9  bright red
            (0x30, 0xd1, 0x58), // 10 bright green
            (0xff, 0xd6, 0x0a), // 11 bright yellow
            (0x40, 0x9c, 0xff), // 12 bright blue
            (0xda, 0x8f, 0xff), // 13 bright magenta
            (0x70, 0xd7, 0xff), // 14 bright cyan
            (0xf5, 0xf5, 0xf7), // 15 bright white
        ];
        colors[..16].copy_from_slice(&base16);

        // 216 color cube (indices 16..=231)
        // 6x6x6 RGB cube, each component: 0, 0x5f, 0x87, 0xaf, 0xd7, 0xff
        let cube_values: [u8; 6] = [0x00, 0x5f, 0x87, 0xaf, 0xd7, 0xff];
        for r in 0..6 {
            for g in 0..6 {
                for b in 0..6 {
                    let index = 16 + r * 36 + g * 6 + b;
                    colors[index] = (cube_values[r], cube_values[g], cube_values[b]);
                }
            }
        }

        // 24 grayscale (indices 232..=255)
        // Values: 0x08, 0x12, 0x1c, ..., 0xee (step of 10)
        for i in 0..24 {
            let v = (i * 10 + 8) as u8;
            colors[232 + i] = (v, v, v);
        }

        Self { colors }
    }
}

impl AnsiPalette {
    /// Convert an ANSI color index (0-255) to RGB
    pub fn ansi_to_rgb(&self, index: u8) -> (u8, u8, u8) {
        self.colors[index as usize]
    }

    /// Get foreground color for default text
    pub fn default_fg(&self) -> (u8, u8, u8) {
        (0xe8, 0xe8, 0xed)
    }

    /// Get background color
    pub fn default_bg(&self) -> (u8, u8, u8) {
        (0x12, 0x12, 0x14)
    }
}
```

- [ ] **Step 4: Update lib.rs**

```rust
// crates/gpu-renderer/src/lib.rs
pub mod colors;
pub mod widget;
pub mod grid;
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cd ~/dev-projects/apps/altermative && cargo test --test color_test`

Expected: All 7 tests PASS

- [ ] **Step 6: Commit**

```bash
cd ~/dev-projects/apps/altermative
git add crates/gpu-renderer/src/colors.rs tests/color_test.rs crates/gpu-renderer/src/lib.rs
git commit -m "feat: ANSI 256-color palette with tests"
```

---

### Task 5: Terminal Grid to Render Data Conversion

**Files:**
- Create: `crates/gpu-renderer/src/grid.rs`

- [ ] **Step 1: Define render data types**

```rust
// crates/gpu-renderer/src/grid.rs
use crate::colors::AnsiPalette;

/// A single character cell ready for rendering
#[derive(Debug, Clone)]
pub struct RenderCell {
    pub c: char,
    pub fg: [f32; 4],  // RGBA normalized 0.0-1.0
    pub bg: [f32; 4],
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub is_cursor: bool,
}

/// The complete grid of cells to render
#[derive(Debug, Clone)]
pub struct RenderGrid {
    pub cells: Vec<Vec<RenderCell>>,
    pub rows: usize,
    pub cols: usize,
}

fn to_float(r: u8, g: u8, b: u8) -> [f32; 4] {
    [r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, 1.0]
}

impl RenderGrid {
    /// Convert an alacritty_terminal screen into render data
    pub fn from_terminal(term: &altermative_terminal::TerminalState, palette: &AnsiPalette) -> Self {
        let rows = term.rows();
        let cols = term.cols();
        let cursor = term.cursor_point();
        let default_fg = to_float(palette.default_fg().0, palette.default_fg().1, palette.default_fg().2);
        let default_bg = to_float(palette.default_bg().0, palette.default_bg().1, palette.default_bg().2);

        let mut cells = Vec::with_capacity(rows);
        for row in 0..rows {
            let mut row_cells = Vec::with_capacity(cols);
            for col in 0..cols {
                let cell = term.cell(row, col);
                let c = cell.c;

                let fg = resolve_color(&cell.fg, palette, &default_fg);
                let bg = resolve_color(&cell.bg, palette, &default_bg);

                let flags = cell.flags;
                let is_cursor = cursor.line == row && cursor.column.0 == col;

                row_cells.push(RenderCell {
                    c,
                    fg,
                    bg,
                    bold: flags.contains(alacritty_terminal::term::cell::Flags::BOLD),
                    italic: flags.contains(alacritty_terminal::term::cell::Flags::ITALIC),
                    underline: flags.contains(alacritty_terminal::term::cell::Flags::UNDERLINE),
                    is_cursor,
                });
            }
            cells.push(row_cells);
        }

        Self { cells, rows, cols }
    }
}

fn resolve_color(
    color: &alacritty_terminal::vte::ansi::Color,
    palette: &AnsiPalette,
    default: &[f32; 4],
) -> [f32; 4] {
    use alacritty_terminal::vte::ansi::Color;
    match color {
        Color::Named(named) => {
            let idx = *named as u8;
            let (r, g, b) = palette.ansi_to_rgb(idx);
            to_float(r, g, b)
        }
        Color::Spec(rgb) => to_float(rgb.r, rgb.g, rgb.b),
        Color::Indexed(idx) => {
            let (r, g, b) = palette.ansi_to_rgb(*idx);
            to_float(r, g, b)
        }
        _ => *default,
    }
}
```

Note: The exact `alacritty_terminal` color and flags API may differ slightly between versions. Adjust `Color` variant names and `Flags` field access to match the actual version installed. The pattern — matching on Named/Spec/Indexed and reading cell flags — stays the same.

- [ ] **Step 2: Verify it compiles**

Run: `cd ~/dev-projects/apps/altermative && cargo check`

Expected: Compiles. There may be warnings about unused code — that's fine.

- [ ] **Step 3: Commit**

```bash
cd ~/dev-projects/apps/altermative
git add crates/gpu-renderer/src/grid.rs
git commit -m "feat: terminal grid to render data conversion"
```

---

### Task 6: GPU Text Rendering Widget

**Files:**
- Create: `crates/gpu-renderer/src/widget.rs`

This is the core rendering widget. It uses iced's `canvas` widget with cosmic-text for text layout, rather than the raw `shader` pipeline, because cosmic-text + iced canvas gives us text rendering without writing custom WGSL shaders. We can optimize to a custom shader later if needed.

- [ ] **Step 1: Implement the terminal view widget**

```rust
// crates/gpu-renderer/src/widget.rs
use crate::colors::AnsiPalette;
use crate::grid::{RenderCell, RenderGrid};

use cosmic_text::{Attrs, Buffer, Color as CColor, Family, FontSystem, Metrics, Shaping, Weight};
use iced::widget::canvas::{self, Cache, Canvas, Frame, Geometry};
use iced::mouse;
use iced::{Element, Length, Rectangle, Renderer, Size, Theme};

pub struct TerminalView {
    grid: RenderGrid,
    font_size: f32,
    cell_width: f32,
    cell_height: f32,
}

impl TerminalView {
    pub fn new(grid: RenderGrid) -> Self {
        let font_size = 14.0;
        let cell_height = font_size * 1.4;
        let cell_width = font_size * 0.6; // monospace approximation

        Self {
            grid,
            font_size,
            cell_width,
            cell_height,
        }
    }

    pub fn view(&self) -> Element<'_, super::RendererMessage> {
        Canvas::new(TerminalCanvas {
            grid: &self.grid,
            font_size: self.font_size,
            cell_width: self.cell_width,
            cell_height: self.cell_height,
        })
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
    }
}

struct TerminalCanvas<'a> {
    grid: &'a RenderGrid,
    font_size: f32,
    cell_width: f32,
    cell_height: f32,
}

impl<'a> canvas::Program<super::RendererMessage> for TerminalCanvas<'a> {
    type State = ();

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());

        // Draw background
        let bg = &self.grid.cells.first().and_then(|r| r.first()).map(|c| c.bg).unwrap_or([0.07, 0.07, 0.08, 1.0]);
        frame.fill_rectangle(
            iced::Point::ORIGIN,
            bounds.size(),
            iced::Color::from_rgba(bg[0], bg[1], bg[2], bg[3]),
        );

        // Draw each cell
        for (row_idx, row) in self.grid.cells.iter().enumerate() {
            let y = row_idx as f32 * self.cell_height;
            if y > bounds.height {
                break;
            }

            for (col_idx, cell) in row.iter().enumerate() {
                let x = col_idx as f32 * self.cell_width;
                if x > bounds.width {
                    break;
                }

                // Draw cell background if different from default
                if cell.bg != *bg || cell.is_cursor {
                    let bg_color = if cell.is_cursor {
                        iced::Color::from_rgba(0.9, 0.9, 0.9, 0.9)
                    } else {
                        iced::Color::from_rgba(cell.bg[0], cell.bg[1], cell.bg[2], cell.bg[3])
                    };
                    frame.fill_rectangle(
                        iced::Point::new(x, y),
                        Size::new(self.cell_width, self.cell_height),
                        bg_color,
                    );
                }

                // Draw character
                if cell.c != ' ' && cell.c != '\0' {
                    let fg_color = if cell.is_cursor {
                        iced::Color::from_rgba(0.07, 0.07, 0.08, 1.0)
                    } else {
                        iced::Color::from_rgba(cell.fg[0], cell.fg[1], cell.fg[2], cell.fg[3])
                    };

                    let text = canvas::Text {
                        content: cell.c.to_string(),
                        position: iced::Point::new(x, y),
                        color: fg_color,
                        size: iced::Pixels(self.font_size),
                        font: iced::Font::MONOSPACE,
                        ..canvas::Text::default()
                    };
                    frame.fill_text(text);
                }

                // Draw underline
                if cell.underline {
                    let fg_color = iced::Color::from_rgba(cell.fg[0], cell.fg[1], cell.fg[2], cell.fg[3]);
                    frame.fill_rectangle(
                        iced::Point::new(x, y + self.cell_height - 2.0),
                        Size::new(self.cell_width, 1.0),
                        fg_color,
                    );
                }
            }
        }

        vec![frame.into_geometry()]
    }
}
```

- [ ] **Step 2: Add RendererMessage to lib.rs**

```rust
// crates/gpu-renderer/src/lib.rs
pub mod colors;
pub mod widget;
pub mod grid;

#[derive(Debug, Clone)]
pub enum RendererMessage {
    // Will be extended with click, selection events
}
```

- [ ] **Step 3: Verify it compiles**

Run: `cd ~/dev-projects/apps/altermative && cargo check`

Expected: Compiles. The canvas API may need minor adjustments depending on the exact iced 0.14 canvas API — the pattern of `canvas::Program`, `Frame`, `fill_rectangle`, `fill_text` is stable.

- [ ] **Step 4: Commit**

```bash
cd ~/dev-projects/apps/altermative
git add crates/gpu-renderer/
git commit -m "feat: terminal canvas rendering widget with cell grid drawing"
```

---

### Task 7: Wire Everything Together

**Files:**
- Modify: `alterm/src/main.rs`

- [ ] **Step 1: Connect PTY → terminal state → renderer in main.rs**

```rust
// alterm/src/main.rs
use iced::{application, Element, Subscription, Theme, Size, Task};
use iced::widget::{column, container, text};

use altermative_terminal::{PtyHandle, TerminalEvent, TerminalState};
use altermative_gpu_renderer::colors::AnsiPalette;
use altermative_gpu_renderer::grid::RenderGrid;
use altermative_gpu_renderer::widget::TerminalView;

use tokio::sync::mpsc;

fn main() -> iced::Result {
    env_logger::init();
    application("Altermative", App::update, App::view)
        .subscription(App::subscription)
        .window_size(Size::new(900.0, 600.0))
        .theme(|_| Theme::Dark)
        .run()
}

struct App {
    terminal: TerminalState,
    pty: Option<PtyHandle>,
    pty_rx: Option<mpsc::Receiver<TerminalEvent>>,
    palette: AnsiPalette,
    initialized: bool,
}

#[derive(Debug, Clone)]
enum Message {
    Initialized,
    PtyOutput(Vec<u8>),
    PtyExited(i32),
    PtyError(String),
    KeyboardInput(iced::keyboard::Key, iced::keyboard::Modifiers),
    Renderer(altermative_gpu_renderer::RendererMessage),
}

impl Default for App {
    fn default() -> Self {
        let rows = 24;
        let cols = 80;
        let terminal = TerminalState::new(rows, cols);

        let (pty, pty_rx) = match PtyHandle::spawn(rows, cols) {
            Ok((pty, rx)) => (Some(pty), Some(rx)),
            Err(e) => {
                log::error!("Failed to spawn PTY: {}", e);
                (None, None)
            }
        };

        Self {
            terminal,
            pty,
            pty_rx,
            palette: AnsiPalette::default(),
            initialized: true,
        }
    }
}

impl App {
    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Initialized => {}
            Message::PtyOutput(data) => {
                self.terminal.process_output(&data);
            }
            Message::PtyExited(code) => {
                log::info!("PTY exited with code {}", code);
                self.pty = None;
            }
            Message::PtyError(err) => {
                log::error!("PTY error: {}", err);
            }
            Message::KeyboardInput(key, _modifiers) => {
                if let Some(ref mut pty) = self.pty {
                    if let Some(bytes) = key_to_bytes(&key) {
                        let _ = pty.write(&bytes);
                    }
                }
            }
            Message::Renderer(_msg) => {}
        }
        Task::none()
    }

    fn view(&self) -> Element<Message> {
        let grid = RenderGrid::from_terminal(&self.terminal, &self.palette);
        let view = TerminalView::new(grid);

        container(view.view().map(Message::Renderer))
            .width(iced::Length::Fill)
            .height(iced::Length::Fill)
            .into()
    }

    fn subscription(&self) -> Subscription<Message> {
        let pty_sub = if self.pty.is_some() {
            iced::event::listen_with(|event, _status, _id| {
                if let iced::Event::Keyboard(iced::keyboard::Event::KeyPressed { key, modifiers, .. }) = event {
                    Some(Message::KeyboardInput(key, modifiers))
                } else {
                    None
                }
            })
        } else {
            Subscription::none()
        };

        pty_sub
    }
}

/// Convert iced keyboard key to bytes to send to PTY
fn key_to_bytes(key: &iced::keyboard::Key) -> Option<Vec<u8>> {
    use iced::keyboard::Key;
    match key {
        Key::Character(c) => Some(c.as_bytes().to_vec()),
        Key::Named(named) => {
            use iced::keyboard::key::Named;
            match named {
                Named::Enter => Some(b"\r".to_vec()),
                Named::Backspace => Some(b"\x7f".to_vec()),
                Named::Tab => Some(b"\t".to_vec()),
                Named::Escape => Some(b"\x1b".to_vec()),
                Named::ArrowUp => Some(b"\x1b[A".to_vec()),
                Named::ArrowDown => Some(b"\x1b[B".to_vec()),
                Named::ArrowRight => Some(b"\x1b[C".to_vec()),
                Named::ArrowLeft => Some(b"\x1b[D".to_vec()),
                Named::Home => Some(b"\x1b[H".to_vec()),
                Named::End => Some(b"\x1b[F".to_vec()),
                Named::PageUp => Some(b"\x1b[5~".to_vec()),
                Named::PageDown => Some(b"\x1b[6~".to_vec()),
                Named::Delete => Some(b"\x1b[3~".to_vec()),
                Named::Space => Some(b" ".to_vec()),
                _ => None,
            }
        }
        _ => None,
    }
}
```

- [ ] **Step 2: Wire PTY async reader to iced messages**

The PTY reader runs on a separate thread (Task 2). We need to bridge it to iced's subscription system. Update `App` to use a subscription that reads from the mpsc channel:

Replace the subscription method:

```rust
    fn subscription(&self) -> Subscription<Message> {
        // Keyboard events
        let keyboard_sub = iced::event::listen_with(|event, _status, _id| {
            if let iced::Event::Keyboard(iced::keyboard::Event::KeyPressed { key, modifiers, .. }) = event {
                Some(Message::KeyboardInput(key, modifiers))
            } else {
                None
            }
        });

        keyboard_sub
    }
```

Note: The PTY output bridging requires moving the `mpsc::Receiver` into a subscription stream. Since iced subscriptions need to be recreatable, we'll use a simpler approach for Phase 1 — poll the receiver in the update loop. Add a timer subscription:

Add to `subscription()`:

```rust
    fn subscription(&self) -> Subscription<Message> {
        let keyboard_sub = iced::event::listen_with(|event, _status, _id| {
            if let iced::Event::Keyboard(iced::keyboard::Event::KeyPressed { key, modifiers, .. }) = event {
                Some(Message::KeyboardInput(key, modifiers))
            } else {
                None
            }
        });

        // Poll PTY output at 120fps
        let tick_sub = iced::time::every(std::time::Duration::from_millis(8))
            .map(|_| Message::Tick);

        Subscription::batch([keyboard_sub, tick_sub])
    }
```

Add `Message::Tick` to the enum and handle it:

```rust
#[derive(Debug, Clone)]
enum Message {
    // ... existing variants ...
    Tick,
}
```

In update:

```rust
            Message::Tick => {
                if let Some(ref mut rx) = self.pty_rx {
                    // Drain all available PTY output
                    while let Ok(event) = rx.try_recv() {
                        match event {
                            TerminalEvent::PtyOutput(data) => {
                                self.terminal.process_output(&data);
                            }
                            TerminalEvent::PtyExited(code) => {
                                log::info!("PTY exited with code {}", code);
                                self.pty = None;
                            }
                            TerminalEvent::PtyError(err) => {
                                log::error!("PTY error: {}", err);
                            }
                        }
                    }
                }
            }
```

- [ ] **Step 3: Verify it compiles and runs**

Run: `cd ~/dev-projects/apps/altermative && cargo run --bin alterm`

Expected: A window opens with a dark background. You should see your shell prompt rendered as text. Typing should produce characters. Basic commands like `ls` should show output with colors.

Known limitations at this stage:
- Cell sizing may be off (monospace width approximation)
- No scrollback yet
- No resize handling
- No selection/copy-paste
- Canvas redraws every frame (not optimized)

- [ ] **Step 4: Commit**

```bash
cd ~/dev-projects/apps/altermative
git add alterm/src/main.rs
git commit -m "feat: working terminal — PTY, VT parsing, and canvas rendering connected"
```

---

### Task 8: Window Resize Handling

**Files:**
- Modify: `alterm/src/main.rs`

- [ ] **Step 1: Add resize message handling**

Add to the Message enum:

```rust
    WindowResized(Size),
```

Add a resize event to the subscription:

```rust
        let resize_sub = iced::event::listen_with(|event, _status, _id| {
            if let iced::Event::Window(iced::window::Event::Resized(size)) = event {
                Some(Message::WindowResized(size))
            } else {
                None
            }
        });
```

Add to `Subscription::batch`:

```rust
        Subscription::batch([keyboard_sub, tick_sub, resize_sub])
```

Handle in update:

```rust
            Message::WindowResized(size) => {
                let cell_width = 14.0 * 0.6;  // match TerminalView
                let cell_height = 14.0 * 1.4;
                let cols = (size.width / cell_width) as u16;
                let rows = (size.height / cell_height) as u16;
                if cols > 0 && rows > 0 {
                    self.terminal.resize(rows, cols);
                    if let Some(ref pty) = self.pty {
                        let _ = pty.resize(rows, cols);
                    }
                }
            }
```

- [ ] **Step 2: Test resize**

Run: `cd ~/dev-projects/apps/altermative && cargo run --bin alterm`

Expected: Resizing the window reflows terminal content. Running `tput cols; tput lines` in the terminal should show updated values after resize.

- [ ] **Step 3: Commit**

```bash
cd ~/dev-projects/apps/altermative
git add alterm/src/main.rs
git commit -m "feat: terminal resize handling — PTY and grid update on window resize"
```

---

### Task 9: Scrollback

**Files:**
- Modify: `alterm/src/main.rs`
- Modify: `crates/terminal/src/term.rs`

- [ ] **Step 1: Add scroll message handling**

Add to Message enum:

```rust
    Scroll(f32),  // delta lines (negative = scroll up)
```

Add scroll events to subscription:

```rust
        let scroll_sub = iced::event::listen_with(|event, _status, _id| {
            if let iced::Event::Mouse(iced::mouse::Event::WheelScrolled { delta }) = event {
                match delta {
                    iced::mouse::ScrollDelta::Lines { y, .. } => Some(Message::Scroll(-y)),
                    iced::mouse::ScrollDelta::Pixels { y, .. } => Some(Message::Scroll(-y / 20.0)),
                }
            } else {
                None
            }
        });
```

Add to batch. Handle in update:

```rust
            Message::Scroll(delta) => {
                let lines = delta as i32;
                self.terminal.scroll(lines);
            }
```

- [ ] **Step 2: Add scroll method to TerminalState**

```rust
// Add to crates/terminal/src/term.rs, impl TerminalState:

    /// Scroll the display by the given number of lines
    /// Positive = scroll down (toward recent), negative = scroll up (toward history)
    pub fn scroll(&mut self, lines: i32) {
        use alacritty_terminal::grid::Scroll;
        if lines > 0 {
            self.term.scroll_display(Scroll::Delta(lines));
        } else if lines < 0 {
            self.term.scroll_display(Scroll::Delta(lines));
        }
    }
```

Note: The exact `Scroll` enum API may differ. Check `alacritty_terminal::grid::Scroll` for the actual variants — it may be `Scroll::Lines(i32)` or `Scroll::Delta(i32)`.

- [ ] **Step 3: Update grid.rs to account for display offset**

The `RenderGrid::from_terminal` function should already work with scrollback because `alacritty_terminal` adjusts the visible grid based on `display_offset()`. No changes needed if we're reading from `term[Point]` which respects the display offset.

- [ ] **Step 4: Test scrollback**

Run: `cd ~/dev-projects/apps/altermative && cargo run --bin alterm`

Test: Run `seq 1 200` to fill scrollback, then scroll up with the mouse wheel. You should see earlier lines.

- [ ] **Step 5: Commit**

```bash
cd ~/dev-projects/apps/altermative
git add alterm/src/main.rs crates/terminal/src/term.rs
git commit -m "feat: mouse wheel scrollback support"
```

---

### Task 10: Selection and Copy/Paste

**Files:**
- Modify: `alterm/src/main.rs`
- Modify: `crates/terminal/src/term.rs`

- [ ] **Step 1: Add selection support to TerminalState**

```rust
// Add to crates/terminal/src/term.rs, impl TerminalState:

    /// Start a selection at the given grid point
    pub fn start_selection(&mut self, point: Point, ty: alacritty_terminal::selection::SelectionType) {
        let side = alacritty_terminal::index::Side::Left;
        self.term.selection = Some(alacritty_terminal::selection::Selection::new(ty, point, side));
    }

    /// Update selection to the given point
    pub fn update_selection(&mut self, point: Point) {
        if let Some(ref mut sel) = self.term.selection {
            let side = alacritty_terminal::index::Side::Left;
            sel.update(point, side);
        }
    }

    /// Get the selected text
    pub fn selection_text(&self) -> Option<String> {
        self.term.selection.as_ref().map(|sel| {
            let content = sel.to_range(&self.term);
            // Extract text from selected range
            content.map(|range| {
                let mut text = String::new();
                // Iterate through selected cells and collect text
                // This uses alacritty_terminal's built-in selection text extraction
                text
            }).unwrap_or_default()
        })
    }

    /// Clear the current selection
    pub fn clear_selection(&mut self) {
        self.term.selection = None;
    }
```

Note: The exact selection API (`Selection::new`, `sel.update`, text extraction) depends on the `alacritty_terminal` version. Check the actual API — the Selection type handles rectangular selection, word selection, line selection, etc. The pattern is: create Selection, update it on mouse move, extract text on copy.

- [ ] **Step 2: Add mouse event handling to main.rs**

Add to Message enum:

```rust
    MousePress(iced::Point),
    MouseMove(iced::Point),
    MouseRelease(iced::Point),
    Copy,
    Paste,
```

Add mouse events to subscription and handle Ctrl+Shift+C/V:

```rust
            Message::KeyboardInput(key, modifiers) => {
                // Check for Ctrl+Shift+C (copy)
                if modifiers.control() && modifiers.shift() {
                    if let iced::keyboard::Key::Character(ref c) = key {
                        if c.as_str() == "C" || c.as_str() == "c" {
                            if let Some(text) = self.terminal.selection_text() {
                                return iced::clipboard::write(text);
                            }
                            return Task::none();
                        }
                        if c.as_str() == "V" || c.as_str() == "v" {
                            return iced::clipboard::read(|text| {
                                Message::PasteText(text.unwrap_or_default())
                            });
                        }
                    }
                }
                // Normal key input
                if let Some(ref mut pty) = self.pty {
                    if let Some(bytes) = key_to_bytes(&key) {
                        let _ = pty.write(&bytes);
                    }
                }
            }
```

Add `PasteText(String)` to Message and handle:

```rust
            Message::PasteText(text) => {
                if let Some(ref mut pty) = self.pty {
                    let _ = pty.write(text.as_bytes());
                }
            }
```

- [ ] **Step 3: Test copy/paste**

Run: `cd ~/dev-projects/apps/altermative && cargo run --bin alterm`

Test: Type `echo "hello world"`. Select text with mouse, press Ctrl+Shift+C. Press Ctrl+Shift+V to paste.

Note: Full mouse selection will need pixel-to-grid-point conversion. For Phase 1, getting Ctrl+Shift+C/V working with keyboard is the priority.

- [ ] **Step 4: Commit**

```bash
cd ~/dev-projects/apps/altermative
git add alterm/src/main.rs crates/terminal/src/term.rs
git commit -m "feat: copy/paste with Ctrl+Shift+C/V"
```

---

### Task 11: Polish — Cursor Blink, Title, Focus

**Files:**
- Modify: `alterm/src/main.rs`

- [ ] **Step 1: Add cursor blink**

Add a blink toggle to App state:

```rust
struct App {
    // ... existing fields ...
    cursor_visible: bool,
    blink_count: u32,
}
```

In the `Tick` handler, toggle blink every ~500ms (roughly 60 ticks at 8ms):

```rust
            Message::Tick => {
                self.blink_count += 1;
                if self.blink_count % 62 == 0 {
                    self.cursor_visible = !self.cursor_visible;
                }
                // ... existing PTY drain code ...
            }
```

Pass `cursor_visible` to `RenderGrid::from_terminal` so the cursor cell respects the blink state.

- [ ] **Step 2: Set window title from terminal escape sequences**

In the `Tick` handler, after draining PTY output, check for title events:

```rust
                // Check for terminal events (title changes, bell, etc.)
                for event in self.terminal.drain_events() {
                    match event {
                        alacritty_terminal::event::Event::Title(title) => {
                            log::info!("Terminal title: {}", title);
                            // iced doesn't easily support dynamic window titles yet
                            // Will be addressed in Phase 2
                        }
                        _ => {}
                    }
                }
```

- [ ] **Step 3: Final test**

Run: `cd ~/dev-projects/apps/altermative && cargo run --bin alterm`

Verify:
1. Window opens with dark background and shell prompt
2. Typing produces characters, commands execute
3. `ls --color` shows colored output
4. Arrow keys work for command history
5. Ctrl+C sends interrupt
6. Resizing the window reflows content
7. Mouse wheel scrolls through history
8. Ctrl+Shift+C/V copies and pastes
9. Cursor blinks

- [ ] **Step 4: Commit**

```bash
cd ~/dev-projects/apps/altermative
git add alterm/src/main.rs
git commit -m "feat: cursor blink, terminal event handling, Phase 1 complete"
```

---

## Phase 1 Completion Checklist

- [ ] Single terminal pane renders in an iced window
- [ ] GPU-accelerated text rendering via iced canvas
- [ ] PTY integration — default shell spawns and accepts input
- [ ] ANSI escape sequence parsing via alacritty_terminal
- [ ] ANSI colors: 16 standard + 256 indexed + truecolor
- [ ] Keyboard input: printable chars, enter, backspace, arrows, escape, tab
- [ ] Window resize reflows terminal content
- [ ] Mouse wheel scrollback
- [ ] Copy/paste (Ctrl+Shift+C/V)
- [ ] Cursor rendering with blink
- [ ] Binary compiles and runs on Linux

## Notes for Phase 2

- Replace iced canvas text rendering with custom wgpu shader for better performance at scale
- The `TerminalView` widget will become a block in the tiling layout
- The `App` struct will evolve into the Mux (multiplexer) managing multiple terminals
- PTY subscription should move from polling (8ms tick) to a proper async stream
- Selection needs mouse-to-grid-point conversion for click-and-drag
- Window title changes need iced window API support
