/// Block — the core abstraction for content living inside a pane.
///
/// A `Block` wraps a unit of terminal state or AI chat session so that the
/// workspace layer can manage multiple panes uniformly without knowing the
/// details of each content type.
use tokio::sync::mpsc;

use gpu_renderer::colors::AnsiPalette;
use gpu_renderer::grid::RenderGrid;
use terminal::{PtyHandle, TerminalEvent, TerminalState};

use browser::BrowserState;
use preview::PreviewState;

use crate::ai_chat::AIChatState;
use crate::settings_panel::SettingsState;

/// How many ticks before the cursor blink state toggles.
const BLINK_TICKS: u32 = 30;

/// Font size used for terminal rendering (logical pixels).
pub const FONT_SIZE: f32 = 14.0;
/// Cell width = font_size * 0.6 (monospace approximation).
pub const CELL_WIDTH: f32 = FONT_SIZE * 0.6;
/// Cell height = font_size * 1.4.
pub const CELL_HEIGHT: f32 = FONT_SIZE * 1.4;

/// A pane's content unit.
pub enum Block {
    Terminal {
        state: TerminalState,
        pty: PtyHandle,
        events: mpsc::Receiver<TerminalEvent>,
        palette: AnsiPalette,
        cursor_visible: bool,
        blink_count: u32,
        /// Whether the terminal content has changed since the last render.
        dirty: bool,
        /// Cached render grid to avoid rebuilding when nothing changed.
        cached_grid: Option<RenderGrid>,
    },
    AIChat {
        state: AIChatState,
    },
    Settings {
        state: SettingsState,
    },
    Browser {
        state: BrowserState,
    },
    Preview {
        state: PreviewState,
    },
    HotkeyInfo,
}

impl Block {
    /// Calculate terminal grid dimensions (rows, cols) from pixel dimensions.
    pub fn size_from_pixels(pixel_width: f32, pixel_height: f32) -> (u16, u16) {
        let cols = (pixel_width / CELL_WIDTH).floor().max(10.0) as u16;
        let rows = (pixel_height / CELL_HEIGHT).floor().max(2.0) as u16;
        (rows, cols)
    }

    /// Create a new terminal block, spawning a PTY with the given dimensions.
    pub fn new_terminal(rows: u16, cols: u16) -> Result<Self, String> {
        let (pty, events) = PtyHandle::spawn(rows, cols)?;
        let state = TerminalState::new(rows as usize, cols as usize);
        let palette = AnsiPalette::default();
        Ok(Block::Terminal {
            state,
            pty,
            events,
            palette,
            cursor_visible: true,
            blink_count: 0,
            dirty: true,
            cached_grid: None,
        })
    }

    /// Create a terminal whose shell starts in `cwd` (if given and valid).
    pub fn new_terminal_in(rows: u16, cols: u16, cwd: Option<&std::path::Path>) -> Result<Self, String> {
        let (pty, events) = PtyHandle::spawn_in(rows, cols, cwd)?;
        let state = TerminalState::new(rows as usize, cols as usize);
        let palette = AnsiPalette::default();
        Ok(Block::Terminal {
            state,
            pty,
            events,
            palette,
            cursor_visible: true,
            blink_count: 0,
            dirty: true,
            cached_grid: None,
        })
    }

    /// The terminal's current working directory, if determinable.
    pub fn working_dir(&self) -> Option<std::path::PathBuf> {
        match self {
            Block::Terminal { pty, .. } => pty.child_pid().and_then(terminal::read_proc_cwd),
            _ => None,
        }
    }

    /// Create a new AI chat block for the given provider and model.
    pub fn new_ai_chat(provider: String, model: String) -> Self {
        Block::AIChat {
            state: AIChatState::new(provider, model),
        }
    }

    /// Create a new settings panel block with a working copy of the given config.
    pub fn new_settings(config: alterm_config::AppConfig) -> Self {
        Block::Settings {
            state: SettingsState::new(config),
        }
    }

    /// Create a new browser block navigated to `url`.
    pub fn new_browser(url: &str) -> Self {
        Block::Browser {
            state: BrowserState::new(url),
        }
    }

    /// Create a new file preview block at the given path.
    pub fn new_preview(path: &str) -> Self {
        Block::Preview {
            state: PreviewState::open(path),
        }
    }

    /// Create a new hotkey info reference pane (no state needed).
    pub fn new_hotkey_info() -> Self {
        Block::HotkeyInfo
    }

    /// Reconstruct a block from its persisted state. `config` is needed to
    /// rebuild a `Settings` pane.
    pub fn from_state(bs: &crate::session::BlockState, config: &alterm_config::AppConfig) -> Block {
        use crate::session::BlockState;
        match bs {
            BlockState::Terminal { cwd, rows, cols, .. } => {
                let dir = cwd.as_deref();
                Block::new_terminal_in((*rows).max(1), (*cols).max(1), dir)
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

    /// Whether this block is a hotkey info pane.
    pub fn is_hotkey_info(&self) -> bool {
        matches!(self, Block::HotkeyInfo)
    }

    /// Drive the block forward one tick:
    /// - Terminal: drain pending PTY output and advance cursor blink.
    /// - AIChat: no-op (streaming is handled via messages).
    pub fn tick(&mut self) {
        match self {
            Block::Terminal {
                state,
                events,
                cursor_visible,
                blink_count,
                dirty,
                ..
            } => {
                // Drain all pending PTY events without blocking.
                let mut received_output = false;
                loop {
                    match events.try_recv() {
                        Ok(TerminalEvent::PtyOutput(data)) => {
                            state.process_output(&data);
                            received_output = true;
                        }
                        Ok(TerminalEvent::PtyExited(_)) | Ok(TerminalEvent::PtyError(_)) => {
                            break;
                        }
                        Err(_) => break, // empty or disconnected
                    }
                }

                if received_output {
                    *dirty = true;
                }

                // Cursor blink.
                *blink_count += 1;
                if *blink_count >= BLINK_TICKS {
                    *blink_count = 0;
                    *cursor_visible = !*cursor_visible;
                    *dirty = true;
                }
            }
            Block::AIChat { .. } => {
                // Streaming is driven by external messages, not ticks.
            }
            Block::Settings { .. } => {
                // Settings is a pure UI panel — nothing to tick.
            }
            Block::Browser { .. } => {
                // Browser state is driven by user navigation messages.
            }
            Block::Preview { .. } => {
                // Preview state is driven by navigation messages.
            }
            Block::HotkeyInfo => {
                // Static reference pane — nothing to tick.
            }
        }

        // Rebuild the cached grid only when something changed.
        self.refresh_cache();
    }

    /// Send raw input bytes to the PTY (no-op for non-terminal blocks).
    pub fn write_input(&mut self, data: &[u8]) {
        match self {
            Block::Terminal { pty, .. } => {
                if let Err(e) = pty.write(data) {
                    log::warn!("Block::write_input failed: {e}");
                }
            }
            Block::AIChat { .. } => {}
            Block::Settings { .. } => {}
            Block::Browser { .. } => {}
            Block::Preview { .. } => {}
            Block::HotkeyInfo => {}
        }
    }

    /// Resize both the terminal grid and the PTY (no-op for non-terminal blocks).
    pub fn resize(&mut self, rows: u16, cols: u16) {
        match self {
            Block::Terminal { state, pty, dirty, cached_grid, .. } => {
                state.resize(rows as usize, cols as usize);
                if let Err(e) = pty.resize(rows, cols) {
                    log::warn!("Block::resize PTY failed: {e}");
                }
                *dirty = true;
                *cached_grid = None;
            }
            Block::AIChat { .. } => {}
            Block::Settings { .. } => {}
            Block::Browser { .. } => {}
            Block::Preview { .. } => {}
            Block::HotkeyInfo => {}
        }
        self.refresh_cache();
    }

    /// Return the current terminal dimensions (rows, cols).
    /// For non-terminal blocks, returns (0, 0).
    pub fn dimensions(&self) -> (u16, u16) {
        match self {
            Block::Terminal { state, .. } => {
                (state.rows() as u16, state.cols() as u16)
            }
            Block::AIChat { .. } => (0, 0),
            Block::Settings { .. } => (0, 0),
            Block::Browser { .. } => (0, 0),
            Block::Preview { .. } => (0, 0),
            Block::HotkeyInfo => (0, 0),
        }
    }

    /// Human-readable title for the pane tab / title bar.
    pub fn title(&self) -> String {
        match self {
            Block::Terminal { .. } => "Terminal".to_string(),
            Block::AIChat { state } => {
                format!("AI Chat ({})", state.provider_name)
            }
            Block::Settings { state } => {
                if state.dirty { "Settings *".to_string() } else { "Settings".to_string() }
            }
            Block::Browser { state } => {
                format!("Browser — {}", state.display_title())
            }
            Block::Preview { state } => {
                let name = state.path.file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| state.path.display().to_string());
                format!("Preview — {name}")
            }
            Block::HotkeyInfo => "Keyboard Shortcuts".to_string(),
        }
    }

    /// Whether this block is a terminal.
    pub fn is_terminal(&self) -> bool {
        matches!(self, Block::Terminal { .. })
    }

    /// Whether this block is an AI chat.
    pub fn is_ai_chat(&self) -> bool {
        matches!(self, Block::AIChat { .. })
    }

    /// Whether this block is a settings panel.
    pub fn is_settings(&self) -> bool {
        matches!(self, Block::Settings { .. })
    }

    /// Whether this block is a browser.
    pub fn is_browser(&self) -> bool {
        matches!(self, Block::Browser { .. })
    }

    /// Whether this block is a file preview.
    pub fn is_preview(&self) -> bool {
        matches!(self, Block::Preview { .. })
    }

    /// Build a render-ready grid from the block's current terminal state.
    ///
    /// Returns a cached grid when nothing has changed since the last render.
    /// For non-terminal blocks, returns a minimal empty grid.
    ///
    /// `light_mode` selects light-theme default fg/bg colors.
    pub fn render_grid(&self, light_mode: bool) -> RenderGrid {
        match self {
            Block::Terminal { cached_grid, state, palette, cursor_visible, .. } => {
                // Return cached grid if available, otherwise build on demand
                // (handles the case where view() is called before the first tick).
                // If the cached grid has a different light_mode we rebuild.
                if let Some(grid) = cached_grid {
                    if grid.light_mode == light_mode {
                        return grid.clone();
                    }
                }
                RenderGrid::from_terminal_with_cursor(state, palette, *cursor_visible, light_mode)
            }
            Block::AIChat { .. } | Block::Settings { .. } | Block::Browser { .. } | Block::Preview { .. } | Block::HotkeyInfo => {
                // Non-terminal blocks don't use the terminal canvas.
                RenderGrid {
                    cells: Vec::new(),
                    rows: 0,
                    cols: 0,
                    display_offset: 0,
                    total_history: 0,
                    light_mode: false,
                }
            }
        }
    }

    /// Rebuild the cached render grid if the terminal is dirty.
    ///
    /// Call this at the end of `tick()` so the immutable `render_grid()`
    /// used by `view()` always has a ready snapshot.
    fn refresh_cache(&mut self) {
        match self {
            Block::Terminal { state, palette, cursor_visible, dirty, cached_grid, .. } => {
                if *dirty || cached_grid.is_none() {
                    // Use the existing cached light_mode, or default to dark.
                    let lm = cached_grid.as_ref().map_or(false, |g| g.light_mode);
                    *cached_grid = Some(
                        RenderGrid::from_terminal_with_cursor(state, palette, *cursor_visible, lm),
                    );
                    *dirty = false;
                }
            }
            Block::AIChat { .. } | Block::Settings { .. } | Block::Browser { .. } | Block::Preview { .. } | Block::HotkeyInfo => {}
        }
    }

    /// Reset cursor blink so the cursor is immediately visible (e.g. on keypress).
    pub fn reset_cursor_blink(&mut self) {
        match self {
            Block::Terminal { cursor_visible, blink_count, .. } => {
                *cursor_visible = true;
                *blink_count = 0;
            }
            Block::AIChat { .. } | Block::Settings { .. } | Block::Browser { .. } | Block::Preview { .. } | Block::HotkeyInfo => {}
        }
    }

    /// Scroll the terminal viewport by the given number of lines.
    ///
    /// Positive = scroll up (toward history), negative = scroll down.
    /// No-op for non-terminal blocks.
    pub fn scroll(&mut self, lines: i32) {
        match self {
            Block::Terminal { state, dirty, .. } => {
                state.scroll(lines);
                *dirty = true;
            }
            Block::AIChat { .. } | Block::Settings { .. } | Block::Browser { .. } | Block::Preview { .. } | Block::HotkeyInfo => {}
        }
        self.refresh_cache();
    }

    /// Read the last N lines of visible text from the terminal.
    ///
    /// Returns `None` for non-terminal blocks. The returned string contains
    /// one line per element, with trailing whitespace trimmed.
    pub fn recent_output(&self, lines: usize) -> Option<String> {
        match self {
            Block::Terminal { state, .. } => {
                let rows = state.rows();
                let cols = state.cols();
                let start = if rows > lines { rows - lines } else { 0 };

                let mut output = Vec::new();
                for row in start..rows {
                    let mut line = String::with_capacity(cols);
                    for col in 0..cols {
                        if let Some(cell) = state.cell(row, col) {
                            line.push(cell.c);
                        }
                    }
                    output.push(line.trim_end().to_string());
                }

                // Trim trailing empty lines.
                while output.last().map_or(false, |l| l.is_empty()) {
                    output.pop();
                }

                if output.is_empty() {
                    None
                } else {
                    Some(output.join("\n"))
                }
            }
            Block::AIChat { .. } | Block::Settings { .. } | Block::Browser { .. } | Block::Preview { .. } | Block::HotkeyInfo => None,
        }
    }

    /// Snapshot this block's restorable state for session persistence.
    pub fn to_block_state(&self) -> crate::session::BlockState {
        use crate::session::BlockState;
        match self {
            Block::Terminal { state, .. } => BlockState::Terminal {
                cwd: self.working_dir(),
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
}
