/// Block — the core abstraction for content living inside a pane.
///
/// A `Block` wraps a unit of terminal state (and, in the future, AI chat,
/// browser views, etc.) so that the workspace layer can manage multiple panes
/// uniformly without knowing the details of each content type.
use tokio::sync::mpsc;

use gpu_renderer::colors::AnsiPalette;
use gpu_renderer::grid::RenderGrid;
use terminal::{PtyHandle, TerminalEvent, TerminalState};

/// How many ticks before the cursor blink state toggles.
const BLINK_TICKS: u32 = 30;

/// A pane's content unit.
pub enum Block {
    Terminal {
        state: TerminalState,
        pty: PtyHandle,
        events: mpsc::Receiver<TerminalEvent>,
        palette: AnsiPalette,
        cursor_visible: bool,
        blink_count: u32,
    },
}

impl Block {
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
        })
    }

    /// Drive the block forward one tick:
    /// - drain all pending PTY output and feed it to the terminal,
    /// - advance the cursor blink counter.
    pub fn tick(&mut self) {
        match self {
            Block::Terminal {
                state,
                events,
                cursor_visible,
                blink_count,
                ..
            } => {
                // Drain all pending PTY events without blocking.
                loop {
                    match events.try_recv() {
                        Ok(TerminalEvent::PtyOutput(data)) => {
                            state.process_output(&data);
                        }
                        Ok(TerminalEvent::PtyExited(_)) | Ok(TerminalEvent::PtyError(_)) => {
                            break;
                        }
                        Err(_) => break, // empty or disconnected
                    }
                }

                // Cursor blink.
                *blink_count += 1;
                if *blink_count >= BLINK_TICKS {
                    *blink_count = 0;
                    *cursor_visible = !*cursor_visible;
                }
            }
        }
    }

    /// Send raw input bytes to the PTY.
    pub fn write_input(&mut self, data: &[u8]) {
        match self {
            Block::Terminal { pty, .. } => {
                if let Err(e) = pty.write(data) {
                    log::warn!("Block::write_input failed: {e}");
                }
            }
        }
    }

    /// Resize both the terminal grid and the PTY.
    pub fn resize(&mut self, rows: u16, cols: u16) {
        match self {
            Block::Terminal { state, pty, .. } => {
                state.resize(rows as usize, cols as usize);
                if let Err(e) = pty.resize(rows, cols) {
                    log::warn!("Block::resize PTY failed: {e}");
                }
            }
        }
    }

    /// Return the current terminal dimensions (rows, cols).
    pub fn dimensions(&self) -> (u16, u16) {
        match self {
            Block::Terminal { state, .. } => {
                (state.rows() as u16, state.cols() as u16)
            }
        }
    }

    /// Human-readable title for the pane tab / title bar.
    pub fn title(&self) -> String {
        match self {
            Block::Terminal { .. } => "Terminal".to_string(),
        }
    }

    /// Build a render-ready grid from the block's current terminal state.
    pub fn render_grid(&self) -> RenderGrid {
        match self {
            Block::Terminal { state, palette, cursor_visible, .. } => {
                RenderGrid::from_terminal_with_cursor(state, palette, *cursor_visible)
            }
        }
    }

    /// Reset cursor blink so the cursor is immediately visible (e.g. on keypress).
    pub fn reset_cursor_blink(&mut self) {
        match self {
            Block::Terminal { cursor_visible, blink_count, .. } => {
                *cursor_visible = true;
                *blink_count = 0;
            }
        }
    }

    /// Scroll the terminal viewport by the given number of lines.
    ///
    /// Positive = scroll up (toward history), negative = scroll down.
    pub fn scroll(&mut self, lines: i32) {
        match self {
            Block::Terminal { state, .. } => {
                state.scroll(lines);
            }
        }
    }
}
