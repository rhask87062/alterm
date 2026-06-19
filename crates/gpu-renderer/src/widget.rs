/// Iced canvas widget that renders a terminal grid.
///
/// `TerminalView` wraps a `RenderGrid` and exposes an iced `Element`.
/// `TerminalCanvas` implements `canvas::Program` and does the actual drawing.
use iced::widget::canvas::{self, Frame, Geometry};
use iced::{Color, Element, Fill, Font, Pixels, Point, Rectangle, Size, Theme};
use iced::font::Family;
use iced::mouse;
use iced::widget::Canvas;

use crate::grid::RenderGrid;

// ---------------------------------------------------------------------------
// SelectionState
// ---------------------------------------------------------------------------

/// Tracks mouse-drag selection for a terminal canvas.
#[derive(Default, Clone)]
pub struct SelectionState {
    /// Cell (row, col) where the drag began.
    start: Option<(usize, usize)>,
    /// Cell (row, col) at the cursor's current position.
    current: Option<(usize, usize)>,
    /// True while the left button is held down.
    active: bool,
    /// True once the cursor has moved to a different cell than `start` during
    /// this press. A bare click leaves this `false`, so no highlight is drawn
    /// and no clipboard message is published on release.
    dragged: bool,
    /// The grid's display_offset at the time the selection started.
    /// Used to discard the highlight when the user scrolls away.
    display_offset: usize,
}

// ---------------------------------------------------------------------------
// TerminalView
// ---------------------------------------------------------------------------

/// A widget that renders a terminal grid onto an iced canvas.
pub struct TerminalView {
    /// The render-ready grid produced by `RenderGrid::from_terminal`.
    pub grid: RenderGrid,
    /// Font size in logical pixels (default 14.0).
    pub font_size: f32,
    /// Cell width — monospace approximation: `font_size * 0.6`.
    pub cell_width: f32,
    /// Cell height — `font_size * 1.4`.
    pub cell_height: f32,
    /// The font to use for rendering terminal text.
    pub font: Font,
}

impl TerminalView {
    /// Creates a new `TerminalView` with default font metrics.
    pub fn new(grid: RenderGrid) -> Self {
        let font_size = 14.0_f32;
        TerminalView {
            grid,
            font_size,
            cell_width: font_size * 0.6,
            cell_height: font_size * 1.4,
            font: Font::MONOSPACE,
        }
    }

    /// Set the font size and recalculate cell metrics.
    ///
    /// Cell width = `size * 0.6`, cell height = `size * 1.4`.
    pub fn with_font_size(mut self, size: f32) -> Self {
        self.font_size = size;
        self.cell_width = size * 0.6;
        self.cell_height = size * 1.4;
        self
    }

    /// Set the font family for terminal text rendering.
    ///
    /// Accepts a `&'static str` font name. If the name is empty or
    /// "monospace", falls back to `Font::MONOSPACE`.
    pub fn with_font_family(mut self, family: &'static str) -> Self {
        if family.is_empty() || family.eq_ignore_ascii_case("monospace") {
            self.font = Font::MONOSPACE;
        } else {
            self.font = Font {
                family: Family::Name(family),
                ..Font::MONOSPACE
            };
        }
        self
    }

    /// Returns an iced `Element` backed by a full-size canvas.
    ///
    /// `on_select` is called with the selected text when the user completes
    /// a mouse-drag selection. The returned message is dispatched to the app
    /// and can be used to copy the text to the clipboard.
    ///
    /// `on_context_menu` is called with the absolute cursor position when the
    /// user right-clicks inside the canvas.
    pub fn view<M: 'static>(
        self,
        on_select: impl Fn(String) -> M + 'static,
        on_context_menu: impl Fn(Point) -> M + 'static,
    ) -> Element<'static, M> {
        let program = TerminalCanvas {
            grid: self.grid,
            font_size: self.font_size,
            cell_width: self.cell_width,
            cell_height: self.cell_height,
            font: self.font,
            on_select: Box::new(on_select),
            on_context_menu: Box::new(on_context_menu),
            _msg: std::marker::PhantomData,
        };

        Canvas::new(program)
            .width(Fill)
            .height(Fill)
            .into()
    }
}

// ---------------------------------------------------------------------------
// TerminalCanvas — canvas::Program implementation
// ---------------------------------------------------------------------------

/// Internal canvas program that owns grid data and font metrics.
struct TerminalCanvas<M> {
    grid: RenderGrid,
    font_size: f32,
    cell_width: f32,
    cell_height: f32,
    font: Font,
    on_select: Box<dyn Fn(String) -> M + 'static>,
    on_context_menu: Box<dyn Fn(Point) -> M + 'static>,
    _msg: std::marker::PhantomData<M>,
}

impl<M: 'static> canvas::Program<M> for TerminalCanvas<M> {
    type State = SelectionState;

    fn update(
        &self,
        state: &mut SelectionState,
        event: &canvas::Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<canvas::Action<M>> {
        match event {
            canvas::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                let cell = cursor_to_cell(cursor, bounds, self.cell_width, self.cell_height, self.grid.rows, self.grid.cols);
                if let Some(cell) = cell {
                    state.start = Some(cell);
                    state.current = Some(cell);
                    state.active = true;
                    state.dragged = false;
                    state.display_offset = self.grid.display_offset;
                    Some(canvas::Action::request_redraw().and_capture())
                } else {
                    None
                }
            }
            canvas::Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                if state.active {
                    let new_cell = cursor_to_cell_clamped(cursor, bounds, self.cell_width, self.cell_height, self.grid.rows, self.grid.cols);
                    if new_cell != state.start {
                        state.dragged = true;
                    }
                    state.current = new_cell;
                    Some(canvas::Action::request_redraw())
                } else {
                    None
                }
            }
            canvas::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right)) => {
                if let Some(pos) = cursor.position_in(bounds) {
                    let absolute = Point::new(bounds.x + pos.x, bounds.y + pos.y);
                    Some(
                        canvas::Action::publish((self.on_context_menu)(absolute))
                            .and_capture(),
                    )
                } else {
                    None
                }
            }
            canvas::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                if state.active {
                    state.active = false;
                    if state.dragged {
                        if let (Some(start), Some(end)) = (state.start, state.current) {
                            let text = extract_text(&self.grid, start, end);
                            if !text.is_empty() {
                                return Some(
                                    canvas::Action::publish((self.on_select)(text)).and_capture(),
                                );
                            }
                        }
                        Some(canvas::Action::request_redraw())
                    } else {
                        // Bare click — clear any prior selection so the highlight goes away.
                        state.start = None;
                        state.current = None;
                        Some(canvas::Action::request_redraw())
                    }
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    fn draw(
        &self,
        state: &SelectionState,
        renderer: &iced::Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());

        // Default background — pick light or dark based on the grid's theme flag.
        let (dr, dg, db) = if self.grid.light_mode {
            crate::colors::AnsiPalette::default_bg_light()
        } else {
            crate::colors::AnsiPalette::default_bg()
        };
        let default_bg = Color::from_rgb8(dr, dg, db);

        // Fill entire background first.
        frame.fill_rectangle(
            Point::ORIGIN,
            bounds.size(),
            default_bg,
        );

        let cw = self.cell_width;
        let ch = self.cell_height;

        // Normalize selection range, but only after real drag motion and only
        // if the scroll position hasn't changed since the selection was made —
        // otherwise the highlight would drift over the wrong content as the
        // user scrolls, and a bare click would leave a stray one-cell mark.
        let sel_range = if state.dragged && self.grid.display_offset == state.display_offset {
            normalized_selection(state)
        } else {
            None
        };

        let sel_bg = Color::from_rgb8(51, 120, 220);
        let sel_fg = Color::WHITE;

        for (row_idx, row) in self.grid.cells.iter().enumerate() {
            for (col_idx, cell) in row.iter().enumerate() {
                let x = col_idx as f32 * cw;
                let y = row_idx as f32 * ch;
                let top_left = Point::new(x, y);
                let cell_size = Size::new(cw, ch);

                let selected = sel_range.map_or(false, |(a, b)| cell_in_range(row_idx, col_idx, a, b));

                // Determine fg/bg. Precedence: search highlight > selection >
                // cursor > normal.
                let hl = crate::grid::highlight_colors(cell.highlight, self.grid.light_mode);
                let (fg_color, bg_color) = if let Some((bg, fg)) = hl {
                    (rgba_to_color(&fg), rgba_to_color(&bg))
                } else if selected {
                    (sel_fg, sel_bg)
                } else if cell.is_cursor {
                    let fg = rgba_to_color(&cell.fg);
                    let bg = rgba_to_color(&cell.bg);
                    (bg, fg)
                } else {
                    (rgba_to_color(&cell.fg), rgba_to_color(&cell.bg))
                };

                // Draw cell background if it differs from the default bg
                // (or always for highlighted/cursor/selected cells).
                if hl.is_some() || selected || cell.is_cursor || !colors_approx_equal(bg_color, default_bg) {
                    frame.fill_rectangle(top_left, cell_size, bg_color);
                }

                // Draw the character (skip spaces — they produce no visible glyph).
                if cell.c != ' ' {
                    frame.fill_text(canvas::Text {
                        content: cell.c.to_string(),
                        position: top_left,
                        color: fg_color,
                        size: Pixels(self.font_size),
                        font: self.font,
                        ..canvas::Text::default()
                    });
                }

                // Draw underline as a thin rectangle at the bottom of the cell.
                if cell.underline {
                    let underline_h = (self.font_size * 0.08).max(1.0);
                    let underline_top = Point::new(x, y + ch - underline_h);
                    frame.fill_rectangle(
                        underline_top,
                        Size::new(cw, underline_h),
                        fg_color,
                    );
                }
            }
        }

        // Draw scrollbar if there's scrollback history
        if self.grid.total_history > 0 {
            let scrollbar_width = 6.0_f32;
            let track_x = bounds.width - scrollbar_width - 2.0;
            let track_height = bounds.height;

            // Scrollbar base color: dark scrollbar on light bg, light on dark bg.
            let sb_base = if self.grid.light_mode { 0.0_f32 } else { 1.0_f32 };

            // Track background (subtle)
            frame.fill_rectangle(
                Point::new(track_x, 0.0),
                Size::new(scrollbar_width, track_height),
                Color::from_rgba(sb_base, sb_base, sb_base, 0.05),
            );

            // Thumb: size proportional to visible rows vs total content
            let total_content = self.grid.total_history + self.grid.rows;
            let thumb_ratio = (self.grid.rows as f32) / (total_content as f32);
            let thumb_height = (track_height * thumb_ratio).max(20.0);

            // Position: display_offset=0 means at bottom, max offset means at top
            let max_offset = self.grid.total_history;
            let scroll_fraction = if max_offset > 0 {
                self.grid.display_offset as f32 / max_offset as f32
            } else {
                0.0
            };
            // scroll_fraction=0 → thumb at bottom, =1 → thumb at top
            let thumb_y = (1.0 - scroll_fraction) * (track_height - thumb_height);

            let thumb_color = if self.grid.display_offset > 0 {
                Color::from_rgba(sb_base, sb_base, sb_base, 0.35) // brighter when scrolled
            } else {
                Color::from_rgba(sb_base, sb_base, sb_base, 0.15) // dim when at bottom
            };

            frame.fill_rectangle(
                Point::new(track_x, thumb_y),
                Size::new(scrollbar_width, thumb_height),
                thumb_color,
            );
        }

        vec![frame.into_geometry()]
    }

    fn mouse_interaction(
        &self,
        _state: &SelectionState,
        _bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> mouse::Interaction {
        mouse::Interaction::Text
    }
}

// ---------------------------------------------------------------------------
// Selection helpers
// ---------------------------------------------------------------------------

/// Convert cursor position (inside bounds) to a (row, col) cell.
/// Returns None if the cursor is outside the bounds.
fn cursor_to_cell(
    cursor: mouse::Cursor,
    bounds: Rectangle,
    cw: f32,
    ch: f32,
    rows: usize,
    cols: usize,
) -> Option<(usize, usize)> {
    let pos = cursor.position_in(bounds)?;
    Some(pixel_to_cell(pos.x, pos.y, cw, ch, rows, cols))
}

/// Like `cursor_to_cell` but clamps to the terminal edges during a drag
/// so selections can extend to the border when the cursor leaves the widget.
fn cursor_to_cell_clamped(
    cursor: mouse::Cursor,
    bounds: Rectangle,
    cw: f32,
    ch: f32,
    rows: usize,
    cols: usize,
) -> Option<(usize, usize)> {
    let pos = cursor.position()?;
    let rx = (pos.x - bounds.x).clamp(0.0, bounds.width - 1.0);
    let ry = (pos.y - bounds.y).clamp(0.0, bounds.height - 1.0);
    Some(pixel_to_cell(rx, ry, cw, ch, rows, cols))
}

/// Convert a pixel position (relative to canvas top-left) to a (row, col) cell.
fn pixel_to_cell(x: f32, y: f32, cw: f32, ch: f32, rows: usize, cols: usize) -> (usize, usize) {
    let col = ((x / cw) as usize).min(cols.saturating_sub(1));
    let row = ((y / ch) as usize).min(rows.saturating_sub(1));
    (row, col)
}

/// Normalize selection state into a (start, end) pair with start <= end,
/// or None if there is no selection.
fn normalized_selection(state: &SelectionState) -> Option<((usize, usize), (usize, usize))> {
    let start = state.start?;
    let current = state.current?;
    if start <= current { Some((start, current)) } else { Some((current, start)) }
}

/// Returns true if (row, col) falls within the inclusive selection range [a, b].
fn cell_in_range(row: usize, col: usize, a: (usize, usize), b: (usize, usize)) -> bool {
    let (ar, ac) = a;
    let (br, bc) = b;
    (row > ar || (row == ar && col >= ac)) && (row < br || (row == br && col <= bc))
}

/// Extract the selected text from the grid between two cell positions.
fn extract_text(grid: &RenderGrid, a: (usize, usize), b: (usize, usize)) -> String {
    let (start, end) = if a <= b { (a, b) } else { (b, a) };
    let (start_row, start_col) = start;
    let (end_row, end_col) = end;

    let mut lines: Vec<String> = Vec::new();
    let last_row = end_row.min(grid.rows.saturating_sub(1));

    for row in start_row..=last_row {
        if row >= grid.cells.len() {
            break;
        }
        let row_cells = &grid.cells[row];
        let from_col = if row == start_row { start_col } else { 0 };
        let to_col = if row == end_row { end_col } else { row_cells.len().saturating_sub(1) };
        let from_col = from_col.min(row_cells.len());
        let to_col = (to_col + 1).min(row_cells.len());

        let line: String = row_cells[from_col..to_col].iter().map(|c| c.c).collect();
        lines.push(line.trim_end().to_string());
    }

    lines.join("\n").trim().to_string()
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Convert a `[f32; 4]` RGBA array (from `RenderCell`) to an iced `Color`.
#[inline]
fn rgba_to_color(rgba: &[f32; 4]) -> Color {
    Color { r: rgba[0], g: rgba[1], b: rgba[2], a: rgba[3] }
}

/// Approximate equality check for colors (avoids background redraws on
/// every cell when the color matches the default).
#[inline]
fn colors_approx_equal(a: Color, b: Color) -> bool {
    (a.r - b.r).abs() < 0.004
        && (a.g - b.g).abs() < 0.004
        && (a.b - b.b).abs() < 0.004
        && (a.a - b.a).abs() < 0.004
}
