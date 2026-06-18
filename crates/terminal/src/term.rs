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
    /// Row 0 is the top of the visible viewport. When the terminal is scrolled
    /// (display_offset > 0), this returns cells from the scrollback history.
    pub fn cell(&self, row: usize, col: usize) -> Option<&Cell> {
        if row >= self.rows() || col >= self.cols() {
            return None;
        }
        let display_offset = self.term.grid().display_offset();
        // Line(0) = top of the active screen. Negative lines = scrollback.
        // When scrolled up by `display_offset`, the viewport top is at
        // Line(-(display_offset as i32)).
        let line = Line(row as i32 - display_offset as i32);
        let point = Point::new(line, Column(col));
        Some(&self.term.grid()[point])
    }

    /// Current cursor position (viewport-relative).
    ///
    /// Returns `None` if the terminal is scrolled (cursor is off-viewport).
    pub fn cursor_point(&self) -> Option<Point> {
        if self.term.grid().display_offset() == 0 {
            Some(self.term.grid().cursor.point)
        } else {
            None // cursor is below the viewport when scrolled up
        }
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

    /// Encode up to `max_lines` of scrollback (history + visible) as ANSI text
    /// for session persistence. Preserves fg/bg colors and bold/italic/underline.
    pub fn scrollback_ansi(&self, max_lines: usize) -> String {
        use alacritty_terminal::term::cell::Flags;
        use alacritty_terminal::vte::ansi::Color;

        /// Return the SGR numeric fragment for a color (e.g. "31", "38;5;200",
        /// "38;2;255;0;0", or "" for terminal default).
        fn sgr_color(color: Color, is_fg: bool) -> String {
            match color {
                Color::Named(named) => {
                    let idx = named as usize;
                    if idx < 8 {
                        // Normal colors: fg 30-37, bg 40-47
                        let base = if is_fg { 30 } else { 40 };
                        format!("{}", base + idx)
                    } else if idx < 16 {
                        // Bright colors: fg 90-97, bg 100-107
                        let base = if is_fg { 90 } else { 100 };
                        format!("{}", base + (idx - 8))
                    } else {
                        // Sentinel values (Foreground=256, Background=257, Cursor=258,
                        // Dim*, BrightForeground, DimForeground) — use terminal default
                        if is_fg { "39".to_string() } else { "49".to_string() }
                    }
                }
                Color::Indexed(i) => {
                    if is_fg {
                        format!("38;5;{}", i)
                    } else {
                        format!("48;5;{}", i)
                    }
                }
                Color::Spec(rgb) => {
                    if is_fg {
                        format!("38;2;{};{};{}", rgb.r, rgb.g, rgb.b)
                    } else {
                        format!("48;2;{};{};{}", rgb.r, rgb.g, rgb.b)
                    }
                }
            }
        }

        /// Assemble a full `\x1b[<codes>m` SGR sequence from style components.
        /// Returns an empty string when all style elements are default.
        fn sgr_seq(fg: &str, bg: &str, bold: bool, italic: bool, underline: bool) -> String {
            let mut codes: Vec<String> = Vec::new();
            if bold      { codes.push("1".to_string()); }
            if italic    { codes.push("3".to_string()); }
            if underline { codes.push("4".to_string()); }
            // Only emit fg/bg when not default
            if fg != "39" { codes.push(fg.to_string()); }
            if bg != "49" { codes.push(bg.to_string()); }
            if codes.is_empty() {
                String::new()
            } else {
                format!("\x1b[{}m", codes.join(";"))
            }
        }

        let grid = self.term.grid();
        let total = grid.total_lines();
        let screen = grid.screen_lines();
        // Topmost line index is -(history); bottom is screen-1.
        let history = total.saturating_sub(screen);
        let first = -(history as i32);
        let last = screen as i32 - 1;
        let start = (last - (max_lines as i32) + 1).max(first);

        let cols = grid.columns();
        let mut out = String::new();
        for line in start..=last {
            let mut last_fg = "39".to_string();
            let mut last_bg = "49".to_string();
            let mut last_bold = false;
            let mut last_italic = false;
            let mut last_underline = false;
            let mut line_buf = String::new();
            let mut last_nonblank = 0usize;

            for col in 0..cols {
                let cell = &grid[Point::new(Line(line), Column(col))];
                let fg = sgr_color(cell.fg, true);
                let bg = sgr_color(cell.bg, false);
                let bold = cell.flags.contains(Flags::BOLD);
                let italic = cell.flags.contains(Flags::ITALIC);
                let underline = cell.flags.intersects(Flags::ALL_UNDERLINES);

                if fg != last_fg || bg != last_bg || bold != last_bold
                    || italic != last_italic || underline != last_underline
                {
                    line_buf.push_str("\x1b[0m");
                    let seq = sgr_seq(&fg, &bg, bold, italic, underline);
                    line_buf.push_str(&seq);
                    last_fg = fg;
                    last_bg = bg;
                    last_bold = bold;
                    last_italic = italic;
                    last_underline = underline;
                }
                line_buf.push(cell.c);
                if cell.c != ' ' {
                    last_nonblank = line_buf.len();
                }
            }
            // Trim trailing blank cells; end with reset + CRLF
            line_buf.truncate(last_nonblank);
            line_buf.push_str("\x1b[0m");
            out.push_str(&line_buf);
            out.push_str("\r\n");
        }
        out
    }
}

#[cfg(test)]
mod scrollback_tests {
    use super::*;

    #[test]
    fn scrollback_ansi_captures_text_and_caps_lines() {
        let mut t = TerminalState::new(24, 80);
        // Write red "hello", reset, newline, then "world".
        t.process_output(b"\x1b[31mhello\x1b[0m\r\nworld\r\n");
        let out = t.scrollback_ansi(1000);
        assert!(out.contains("hello"));
        assert!(out.contains("world"));
        // Contains an SGR color escape for the red text.
        assert!(out.contains("\x1b[") && out.contains("31"));
        // Line cap is respected.
        let capped = t.scrollback_ansi(1);
        assert!(capped.lines().count() <= 1 + 1); // allow trailing newline
    }
}
