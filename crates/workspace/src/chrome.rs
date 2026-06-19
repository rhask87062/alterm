//! Theme-aware chrome colors.
//!
//! All UI chrome (sidebar, tab bar, pane title bars, panels) derives its
//! colors from the active iced theme's *extended palette* rather than from
//! hardcoded values. This means every theme — the built-in ones (Solarized,
//! Gruvbox, Catppuccin, …) and our own "Alterm Dark"/"Alterm Light" — gets
//! palette-appropriate chrome automatically, and switching themes actually
//! repaints the chrome.
//!
//! iced generates background shades by `deviate`-ing the base background toward
//! more contrast (lighter on dark themes, darker on light themes), so `weak`
//! through `strong` form a consistent low→high elevation ramp regardless of
//! whether the theme is light or dark. `primary` is the theme's accent color.

use iced::{Color, Theme};

/// Base background — the canvas behind content (terminals, panels).
pub fn bg_base(theme: &Theme) -> Color {
    theme.extended_palette().background.base.color
}

/// Subtly raised surface — bars and panels that sit just above the canvas
/// (tab bar, sidebar, inactive title bars).
pub fn bg_subtle(theme: &Theme) -> Color {
    theme.extended_palette().background.weak.color
}

/// A more raised surface — hover states and lifted elements.
pub fn bg_raised(theme: &Theme) -> Color {
    theme.extended_palette().background.strong.color
}

/// Pane title bars — clearly more elevated than the tab bar (which uses
/// [`bg_subtle`]) so individual panes' title bars contrast against the app
/// title bar. Starts from the most-raised palette step and pushes further
/// past it — lightening on dark themes, darkening on light ones — for a
/// punchy, unmistakable shade on every theme.
pub fn bg_pane_title(theme: &Theme) -> Color {
    let base = bg_pressed(theme);
    // Lighten on dark themes, darken on light themes, toward more contrast.
    let target = if is_dark(bg_base(theme)) { WHITE } else { BLACK };
    mix(base, target, 0.16)
}

const WHITE: Color = Color { r: 1.0, g: 1.0, b: 1.0, a: 1.0 };
const BLACK: Color = Color { r: 0.0, g: 0.0, b: 0.0, a: 1.0 };

/// Perceived-luminance check — true when a color reads as dark.
fn is_dark(c: Color) -> bool {
    0.2126 * c.r + 0.7152 * c.g + 0.0722 * c.b < 0.5
}

/// Linearly interpolate between two colors (`f` in 0.0..=1.0).
fn mix(a: Color, b: Color, f: f32) -> Color {
    Color {
        r: a.r + (b.r - a.r) * f,
        g: a.g + (b.g - a.g) * f,
        b: a.b + (b.b - a.b) * f,
        a: a.a + (b.a - a.a) * f,
    }
}

/// Pressed / most-raised surface.
pub fn bg_pressed(theme: &Theme) -> Color {
    theme.extended_palette().background.stronger.color
}

/// Border / divider lines.
pub fn line(theme: &Theme) -> Color {
    theme.extended_palette().background.strong.color
}

/// Primary readable text on the base background.
pub fn text(theme: &Theme) -> Color {
    theme.extended_palette().background.base.text
}

/// Muted/secondary text — the readable text color at reduced opacity, which
/// stays legible on any theme's chrome.
pub fn text_muted(theme: &Theme) -> Color {
    Color { a: 0.55, ..text(theme) }
}

/// The theme's accent color (e.g. neon magenta for Alterm, blue for Solarized).
/// Used for focus borders and active-tab underlines.
pub fn accent(theme: &Theme) -> Color {
    theme.extended_palette().primary.base.color
}

/// A muted tint of the accent — used to highlight the active pane's title bar
/// and the active tab without overwhelming the chrome.
pub fn accent_subtle(theme: &Theme) -> Color {
    theme.extended_palette().primary.weak.color
}

/// Text guaranteed readable on top of [`accent_subtle`].
pub fn accent_text(theme: &Theme) -> Color {
    theme.extended_palette().primary.weak.text
}

/// The theme's danger color — used for the tab close button on hover.
pub fn danger(theme: &Theme) -> Color {
    theme.extended_palette().danger.base.color
}
