/// Terminal grid to render-ready data conversion.
///
/// Converts the alacritty_terminal screen state into a flat, GPU-friendly
/// representation that the renderer can consume directly.
use alacritty_terminal::index::{Column, Line, Point};
use alacritty_terminal::term::cell::{Cell, Flags};
use alacritty_terminal::vte::ansi::{Color, NamedColor};

use terminal::term::TerminalState;

use crate::colors::AnsiPalette;

// ---------------------------------------------------------------------------
// RenderCell
// ---------------------------------------------------------------------------

/// A single cell ready for rendering.
#[derive(Debug, Clone)]
pub struct RenderCell {
    /// The Unicode character to display.
    pub c: char,
    /// Foreground color as normalized RGBA.
    pub fg: [f32; 4],
    /// Background color as normalized RGBA.
    pub bg: [f32; 4],
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    /// True when this cell is under the terminal cursor.
    pub is_cursor: bool,
}

// ---------------------------------------------------------------------------
// RenderGrid
// ---------------------------------------------------------------------------

/// The complete visible terminal grid converted to render-ready cells.
pub struct RenderGrid {
    /// Row-major 2-D grid: `cells[row][col]`.
    pub cells: Vec<Vec<RenderCell>>,
    pub rows: usize,
    pub cols: usize,
}

impl RenderGrid {
    /// Build a `RenderGrid` from the current visible state of `term`.
    ///
    /// Color names are resolved through `palette`.  The cursor position is
    /// obtained from `term.cursor_point()`.
    pub fn from_terminal(term: &TerminalState, palette: &AnsiPalette) -> Self {
        let rows = term.rows();
        let cols = term.cols();

        let cursor: Point = term.cursor_point();

        let mut cells: Vec<Vec<RenderCell>> = Vec::with_capacity(rows);

        for row in 0..rows {
            let mut row_cells: Vec<RenderCell> = Vec::with_capacity(cols);

            for col in 0..cols {
                let render_cell = if let Some(cell) = term.cell(row, col) {
                    cell_to_render(cell, row, col, cursor, palette)
                } else {
                    // Out-of-bounds — fill with a blank default cell.
                    blank_cell(palette)
                };
                row_cells.push(render_cell);
            }

            cells.push(row_cells);
        }

        RenderGrid { cells, rows, cols }
    }
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Convert one `Cell` from alacritty_terminal into a `RenderCell`.
fn cell_to_render(
    cell: &Cell,
    row: usize,
    col: usize,
    cursor: Point,
    palette: &AnsiPalette,
) -> RenderCell {
    let bold = cell.flags.contains(Flags::BOLD);
    let italic = cell.flags.contains(Flags::ITALIC);
    let underline = cell.flags.intersects(Flags::ALL_UNDERLINES);

    // Handle INVERSE (swap fg/bg).
    let (raw_fg, raw_bg) = if cell.flags.contains(Flags::INVERSE) {
        (cell.bg, cell.fg)
    } else {
        (cell.fg, cell.bg)
    };

    let fg = resolve_color(raw_fg, palette, true);
    let bg = resolve_color(raw_bg, palette, false);

    let is_cursor = cursor.line == Line(row as i32) && cursor.column == Column(col);

    RenderCell { c: cell.c, fg, bg, bold, italic, underline, is_cursor }
}

/// A blank cell using palette defaults.
fn blank_cell(_palette: &AnsiPalette) -> RenderCell {
    let (r, g, b) = AnsiPalette::default_fg();
    let fg = to_float(r, g, b);
    let (r, g, b) = AnsiPalette::default_bg();
    let bg = to_float(r, g, b);
    RenderCell { c: ' ', fg, bg, bold: false, italic: false, underline: false, is_cursor: false }
}

/// Resolve an alacritty_terminal `Color` to normalized RGBA floats.
///
/// `is_fg` controls which default is used when a `NamedColor` falls outside
/// the 0–255 indexed range (i.e. the special Foreground/Background/Cursor
/// sentinels).
pub fn resolve_color(color: Color, palette: &AnsiPalette, is_fg: bool) -> [f32; 4] {
    match color {
        // A direct 24-bit RGB value — just normalize it.
        Color::Spec(rgb) => to_float(rgb.r, rgb.g, rgb.b),

        // An index into the 256-color table.
        Color::Indexed(idx) => {
            let (r, g, b) = palette.ansi_to_rgb(idx);
            to_float(r, g, b)
        },

        // A named ANSI color.  The first 16 map directly into the palette
        // table; the higher sentinels (Foreground, Background, Cursor, dim
        // variants, …) fall back to the appropriate theme default.
        Color::Named(named) => {
            let idx = named as usize;
            if idx < 16 {
                // Standard 16 named colors live at palette indices 0–15.
                let (r, g, b) = palette.ansi_to_rgb(idx as u8);
                to_float(r, g, b)
            } else {
                // NamedColor::Foreground (256), Background (257), Cursor (258),
                // dim variants, bright fg, etc.  Use theme defaults.
                match named {
                    NamedColor::Background => {
                        let (r, g, b) = AnsiPalette::default_bg();
                        to_float(r, g, b)
                    },
                    _ => {
                        // Everything else (Foreground, Cursor, dim/bright
                        // variants) falls back to either the fg or bg default
                        // depending on context.
                        if is_fg {
                            let (r, g, b) = AnsiPalette::default_fg();
                            to_float(r, g, b)
                        } else {
                            let (r, g, b) = AnsiPalette::default_bg();
                            to_float(r, g, b)
                        }
                    },
                }
            }
        },
    }
}

/// Convert 8-bit RGB components to a normalized `[f32; 4]` RGBA array
/// with full opacity.
#[inline]
pub fn to_float(r: u8, g: u8, b: u8) -> [f32; 4] {
    [r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, 1.0]
}
