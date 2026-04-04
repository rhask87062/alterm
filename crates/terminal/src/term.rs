/// Terminal emulation backend wrapping `alacritty_terminal::Term`.
///
/// `TerminalState` owns an `alacritty_terminal::Term` instance together with a
/// VTE byte processor and an `EventProxy` that collects events emitted by the
/// terminal (title changes, bell, PTY write requests, …).
use std::sync::{Arc, Mutex};

use alacritty_terminal::Term;
use alacritty_terminal::event::{Event, EventListener};
use alacritty_terminal::grid::{Dimensions, Scroll};
use alacritty_terminal::index::{Column, Line, Point};
use alacritty_terminal::term::Config;
use alacritty_terminal::term::cell::Cell;
use alacritty_terminal::vte::ansi;

// ---------------------------------------------------------------------------
// Dimensions helper
// ---------------------------------------------------------------------------

/// A simple size value that implements [`Dimensions`] so we can pass it to
/// `Term::new` and `Term::resize`.
#[derive(Clone, Copy, Debug)]
pub struct TermSize {
    pub rows: usize,
    pub cols: usize,
}

impl TermSize {
    pub fn new(rows: usize, cols: usize) -> Self {
        Self { rows, cols }
    }
}

/// Default scrollback history in lines.
const SCROLLBACK_LINES: usize = 10_000;

impl Dimensions for TermSize {
    fn total_lines(&self) -> usize {
        self.rows + SCROLLBACK_LINES
    }

    fn screen_lines(&self) -> usize {
        self.rows
    }

    fn columns(&self) -> usize {
        self.cols
    }
}

// ---------------------------------------------------------------------------
// EventProxy
// ---------------------------------------------------------------------------

/// Collects events fired by the terminal and makes them available through
/// [`TerminalState::drain_events`].
#[derive(Clone, Default, Debug)]
pub struct EventProxy {
    inner: Arc<Mutex<Vec<Event>>>,
}

impl EventProxy {
    pub fn new() -> Self {
        Self::default()
    }

    /// Drain all queued events and return them.
    pub fn drain(&self) -> Vec<Event> {
        let mut guard = self.inner.lock().expect("EventProxy mutex poisoned");
        std::mem::take(&mut *guard)
    }
}

impl EventListener for EventProxy {
    fn send_event(&self, event: Event) {
        if let Ok(mut guard) = self.inner.lock() {
            guard.push(event);
        }
    }
}

// ---------------------------------------------------------------------------
// TerminalState
// ---------------------------------------------------------------------------

/// High-level wrapper around `alacritty_terminal::Term`.
///
/// Owns the VTE byte [`ansi::Processor`] and the [`EventProxy`] so callers
/// do not need to manage these separately.
pub struct TerminalState {
    term: Term<EventProxy>,
    processor: ansi::Processor,
    event_proxy: EventProxy,
}

impl TerminalState {
    /// Create a new terminal with the given grid dimensions.
    pub fn new(rows: usize, cols: usize) -> Self {
        let mut config = Config::default();
        config.scrolling_history = SCROLLBACK_LINES;
        let size = TermSize::new(rows, cols);
        let event_proxy = EventProxy::new();
        let term = Term::new(config, &size, event_proxy.clone());
        let processor = ansi::Processor::new();
        Self { term, processor, event_proxy }
    }

    /// Feed raw PTY output through the VTE parser and update the grid.
    pub fn process_output(&mut self, data: &[u8]) {
        self.processor.advance(&mut self.term, data);
    }

    /// Resize the terminal grid.
    pub fn resize(&mut self, rows: usize, cols: usize) {
        let size = TermSize::new(rows, cols);
        self.term.resize(size);
    }

    /// Number of visible rows.
    pub fn rows(&self) -> usize {
        self.term.screen_lines()
    }

    /// Number of columns.
    pub fn cols(&self) -> usize {
        self.term.columns()
    }

    /// Access a cell at the given viewport-relative row and column.
    ///
    /// Row 0 is the top of the visible viewport.  Returns `None` if the
    /// coordinates are out of bounds.
    pub fn cell(&self, row: usize, col: usize) -> Option<&Cell> {
        if row >= self.rows() || col >= self.cols() {
            return None;
        }
        // `Term::grid` exposes a `Grid<Cell>` indexed by `Point<Line, Column>`.
        // Lines in the grid are stored relative to the viewport: line 0 is the
        // topmost visible line.
        let point = Point::new(Line(row as i32), Column(col));
        Some(&self.term.grid()[point])
    }

    /// Current cursor position (viewport-relative).
    pub fn cursor_point(&self) -> Point {
        self.term.grid().cursor.point
    }

    /// Scroll the viewport by `lines` lines.
    ///
    /// Positive values scroll **up** (toward history), negative values scroll
    /// **down** (toward recent output).
    pub fn scroll(&mut self, lines: i32) {
        self.term.scroll_display(Scroll::Delta(lines));
    }

    /// Current display (scroll) offset — 0 means the bottom (latest) output
    /// is visible.
    pub fn display_offset(&self) -> usize {
        self.term.grid().display_offset()
    }

    /// Drain all queued [`Event`]s from the event proxy.
    pub fn drain_events(&self) -> Vec<Event> {
        self.event_proxy.drain()
    }

    /// Immutable access to the underlying [`Term`].
    pub fn term(&self) -> &Term<EventProxy> {
        &self.term
    }

    /// Mutable access to the underlying [`Term`].
    pub fn term_mut(&mut self) -> &mut Term<EventProxy> {
        &mut self.term
    }
}
