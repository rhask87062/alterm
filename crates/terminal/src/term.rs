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

/// A search hit in grid-line coordinates. `*_line` is alacritty grid space:
/// 0 = top of the active screen, negative = scrollback history.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchMatch {
    pub start_line: i32,
    pub start_col: usize,
    pub end_line: i32,
    pub end_col: usize,
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

    /// Scroll so `target_line` (grid-line coordinate; negative = history) is
    /// visible. No-op when it is already on screen; otherwise centers it,
    /// clamped to the available history.
    pub fn scroll_to_line(&mut self, target_line: i32) {
        let rows = self.rows() as i32;
        let offset = self.display_offset() as i32;
        let top = -offset;
        let bottom = rows - 1 - offset;
        if target_line >= top && target_line <= bottom {
            return; // already visible
        }
        let history = self.term.grid().history_size() as i32;
        let desired = (rows / 2 - target_line).clamp(0, history);
        let delta = desired - offset;
        if delta != 0 {
            self.scroll(delta);
        }
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

    /// Find every match of `pattern` across scrollback history + the active
    /// screen. Returns matches top-to-bottom. `Err` if the pattern fails to
    /// compile (invalid regex).
    pub fn search_all(&self, pattern: &str) -> Result<Vec<SearchMatch>, String> {
        use alacritty_terminal::index::Direction;
        use alacritty_terminal::term::search::{RegexIter, RegexSearch};

        // alacritty's RegexSearch::new applies a "smartcase" heuristic: if the
        // pattern has no uppercase letters it forces global case-insensitive
        // matching. Override this with an inline `(?-i)` flag when the pattern
        // is not already marked case-insensitive via `(?i)`. The inline flag
        // takes precedence over the global DFA-level setting.
        let effective_pattern;
        let pattern = if pattern.starts_with("(?i)") {
            pattern
        } else {
            effective_pattern = format!("(?-i){pattern}");
            &effective_pattern
        };

        let mut regex = RegexSearch::new(pattern).map_err(|e| e.to_string())?;

        let rows = self.rows() as i32;
        let cols = self.cols();
        if rows == 0 || cols == 0 {
            return Ok(Vec::new());
        }
        let history = self.term.grid().history_size() as i32;
        let start = Point::new(Line(-history), Column(0));
        let end = Point::new(Line(rows - 1), Column(cols - 1));

        let matches = RegexIter::new(start, end, Direction::Right, &self.term, &mut regex)
            .map(|m| {
                let s = *m.start();
                let e = *m.end();
                SearchMatch {
                    start_line: s.line.0,
                    start_col: s.column.0,
                    end_line: e.line.0,
                    end_col: e.column.0,
                }
            })
            .collect();
        Ok(matches)
    }
}

// ---------------------------------------------------------------------------
// Search pattern helpers
// ---------------------------------------------------------------------------

/// Backslash-escape regex metacharacters so a query matches literally.
fn escape_regex(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if matches!(
            c,
            '\\' | '.' | '^' | '$' | '*' | '+' | '?' | '(' | ')' | '[' | ']' | '{' | '}' | '|'
                | '-' | '#' | '&' | '~'
        ) {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// Build the final regex string for the search engine.
///
/// - `regex_mode == false`: the query is escaped so it matches literally.
/// - `case_sensitive == false`: a `(?i)` prefix is added for case-insensitive
///   matching.
pub fn build_search_pattern(query: &str, regex_mode: bool, case_sensitive: bool) -> String {
    let body = if regex_mode {
        query.to_string()
    } else {
        escape_regex(query)
    };
    if case_sensitive {
        body
    } else {
        format!("(?i){body}")
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_pattern_escapes_literal_metachars() {
        assert_eq!(build_search_pattern("a.b", false, true), "a\\.b");
        assert_eq!(build_search_pattern("[E0432]", false, true), "\\[E0432\\]");
    }

    #[test]
    fn build_pattern_adds_case_insensitive_prefix() {
        assert_eq!(build_search_pattern("error", false, false), "(?i)error");
        assert_eq!(build_search_pattern("error", false, true), "error");
    }

    #[test]
    fn build_pattern_regex_mode_passthrough() {
        assert_eq!(build_search_pattern("\\d+", true, true), "\\d+");
        assert_eq!(build_search_pattern("ab", true, false), "(?i)ab");
    }

    #[test]
    fn build_pattern_escapes_class_and_verbose_metachars() {
        assert_eq!(build_search_pattern("a-z", false, true), "a\\-z");
        assert_eq!(build_search_pattern("f#b", false, true), "f\\#b");
    }

    #[test]
    fn search_all_finds_literal_substring() {
        let mut t = TerminalState::new(24, 80);
        t.process_output(b"error here\n");
        let pat = build_search_pattern("error", false, true);
        let m = t.search_all(&pat).unwrap();
        assert_eq!(m.len(), 1);
        assert_eq!(m[0].start_line, 0);
        assert_eq!(m[0].start_col, 0);
        assert_eq!(m[0].end_col, 4); // "error" spans cols 0..=4
    }

    #[test]
    fn search_all_respects_case() {
        let mut t = TerminalState::new(24, 80);
        t.process_output(b"ERROR\n");
        assert_eq!(t.search_all(&build_search_pattern("error", false, false)).unwrap().len(), 1);
        assert_eq!(t.search_all(&build_search_pattern("error", false, true)).unwrap().len(), 0);
    }

    #[test]
    fn search_all_invalid_regex_is_err() {
        let t = TerminalState::new(24, 80);
        assert!(t.search_all("(").is_err());
    }

    #[test]
    fn search_all_finds_match_in_scrollback() {
        let mut t = TerminalState::new(24, 80);
        t.process_output(b"needle\n");
        for _ in 0..40 {
            t.process_output(b"x\n");
        }
        let m = t.search_all(&build_search_pattern("needle", false, true)).unwrap();
        assert_eq!(m.len(), 1);
        assert!(m[0].start_line < 0, "match should be in scrollback history");
    }

    #[test]
    fn scroll_to_line_keeps_visible_line_put() {
        let mut t = TerminalState::new(24, 80);
        t.process_output(b"hello\n");
        t.scroll_to_line(0); // line 0 already on screen
        assert_eq!(t.display_offset(), 0);
    }

    #[test]
    fn scroll_to_line_brings_history_line_into_view() {
        let mut t = TerminalState::new(24, 80);
        for _ in 0..100 {
            t.process_output(b"line\n");
        }
        t.scroll_to_line(-50);
        let off = t.display_offset() as i32;
        let rows = 24;
        // -50 must be within the visible window [-off, rows-1-off].
        assert!(-off <= -50 && -50 <= rows - 1 - off, "target line not visible");
        assert!(off > 0);
    }
}
