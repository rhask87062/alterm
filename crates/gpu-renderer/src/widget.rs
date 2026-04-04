/// Iced canvas widget that renders a terminal grid.
///
/// `TerminalView` wraps a `RenderGrid` and exposes an iced `Element`.
/// `TerminalCanvas` implements `canvas::Program` and does the actual drawing.
use iced::widget::canvas::{self, Frame, Geometry};
use iced::{Color, Element, Fill, Font, Pixels, Point, Rectangle, Size, Theme};
use iced::mouse;
use iced::widget::Canvas;

use crate::grid::RenderGrid;

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
        }
    }

    /// Returns an iced `Element` backed by a full-size canvas.
    ///
    /// Consumes `self` so that the grid data is moved into the canvas program
    /// and lives as long as the returned element.
    pub fn view<M: 'static>(self) -> Element<'static, M> {
        let program = TerminalCanvas {
            grid: self.grid,
            font_size: self.font_size,
            cell_width: self.cell_width,
            cell_height: self.cell_height,
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
    _msg: std::marker::PhantomData<M>,
}

impl<M: 'static> canvas::Program<M> for TerminalCanvas<M> {
    type State = ();

    fn draw(
        &self,
        _state: &(),
        renderer: &iced::Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());

        // Default background from the palette.
        let (dr, dg, db) = crate::colors::AnsiPalette::default_bg();
        let default_bg = Color::from_rgb8(dr, dg, db);

        // Fill entire background first.
        frame.fill_rectangle(
            Point::ORIGIN,
            bounds.size(),
            default_bg,
        );

        let cw = self.cell_width;
        let ch = self.cell_height;

        for (row_idx, row) in self.grid.cells.iter().enumerate() {
            for (col_idx, cell) in row.iter().enumerate() {
                let x = col_idx as f32 * cw;
                let y = row_idx as f32 * ch;
                let top_left = Point::new(x, y);
                let cell_size = Size::new(cw, ch);

                // Determine fg/bg, inverting for cursor cells.
                let (fg_color, bg_color) = if cell.is_cursor {
                    // Cursor: invert — use cell fg as bg, default bg as fg.
                    let fg = rgba_to_color(&cell.fg);
                    let bg = rgba_to_color(&cell.bg);
                    // Swap fg and bg for the cursor block.
                    (bg, fg)
                } else {
                    (rgba_to_color(&cell.fg), rgba_to_color(&cell.bg))
                };

                // Draw cell background if it differs from the default bg
                // (or always for cursor cells).
                if cell.is_cursor || !colors_approx_equal(bg_color, default_bg) {
                    frame.fill_rectangle(top_left, cell_size, bg_color);
                }

                // Draw the character (skip spaces — they produce no visible glyph).
                if cell.c != ' ' {
                    frame.fill_text(canvas::Text {
                        content: cell.c.to_string(),
                        position: top_left,
                        color: fg_color,
                        size: Pixels(self.font_size),
                        font: Font::MONOSPACE,
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

            // Track background (subtle)
            frame.fill_rectangle(
                Point::new(track_x, 0.0),
                Size::new(scrollbar_width, track_height),
                Color::from_rgba(1.0, 1.0, 1.0, 0.05),
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
                Color::from_rgba(1.0, 1.0, 1.0, 0.35) // brighter when scrolled
            } else {
                Color::from_rgba(1.0, 1.0, 1.0, 0.15) // dim when at bottom
            };

            frame.fill_rectangle(
                Point::new(track_x, thumb_y),
                Size::new(scrollbar_width, thumb_height),
                thumb_color,
            );
        }

        vec![frame.into_geometry()]
    }
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
