/// A complete color theme for the terminal and UI.
///
/// Colors are stored as `(r, g, b)` tuples of `u8` values.
/// `ansi_colors` covers the standard 16-color ANSI palette
/// (indices 0–7 normal, 8–15 bright).
#[derive(Debug, Clone)]
pub struct Theme {
    pub name: String,
    pub bg: (u8, u8, u8),
    pub fg: (u8, u8, u8),
    pub cursor: (u8, u8, u8),
    pub selection_bg: (u8, u8, u8),
    /// Standard 16 ANSI colors: [0–7 normal, 8–15 bright].
    pub ansi_colors: [(u8, u8, u8); 16],
}

impl Theme {
    /// Dark theme — "Living Terminal": deep violet-black canvas with
    /// neon-magenta accents, sampled from the app icon / marketing site
    /// (`website/src/styles/global.css`).
    ///
    /// Background and foreground match the colors used in
    /// `crates/gpu-renderer/src/colors.rs`.
    pub fn dark() -> Self {
        Self {
            name: "dark".to_string(),
            bg: (0x0d, 0x08, 0x14),     // --bg deep violet-black
            fg: (0xec, 0xe6, 0xf5),     // --text
            cursor: (0xf9, 0x77, 0xff), // --accent neon magenta
            selection_bg: (0x28, 0x00, 0x56), // --purple-deep
            ansi_colors: [
                // Normal (0–7)
                (0x1d, 0x14, 0x30), // 0 black   — --bg-elev-2
                (0xff, 0x6b, 0x9d), // 1 red     — --term-red
                (0x5e, 0xf2, 0xb0), // 2 green   — --term-green
                (0xff, 0xd5, 0x6b), // 3 yellow  — --term-yellow
                (0x8a, 0x7b, 0xff), // 4 blue    — violet-indigo
                (0xd4, 0x50, 0xfc), // 5 magenta — --orchid
                (0x6f, 0xdf, 0xff), // 6 cyan    — --term-cyan
                (0xec, 0xe6, 0xf5), // 7 white   — --text
                // Bright (8–15)
                (0x6c, 0x62, 0x85), // 8  bright black   — --text-faint
                (0xff, 0x8f, 0xb5), // 9  bright red
                (0x7e, 0xf8, 0xc4), // 10 bright green
                (0xff, 0xe2, 0x8a), // 11 bright yellow
                (0xa9, 0x9b, 0xff), // 12 bright blue
                (0xf9, 0x77, 0xff), // 13 bright magenta — --accent
                (0x9b, 0xea, 0xff), // 14 bright cyan
                (0xfa, 0xf3, 0xff), // 15 bright white
            ],
        }
    }

    /// Light theme — the same "Living Terminal" violet family, but favoring
    /// the lighter tints and white: a soft lavender-white canvas with deep
    /// violet text and contrast-tuned (darker) accents.
    pub fn light() -> Self {
        Self {
            name: "light".to_string(),
            bg: (0xfa, 0xf3, 0xff),     // bright white with a lavender tint
            fg: (0x1d, 0x14, 0x30),     // deep violet text (--bg-elev-2)
            cursor: (0xa0, 0x21, 0xd6), // --purple-mid (readable on white)
            selection_bg: (0xe6, 0xd8, 0xf7), // light lavender
            ansi_colors: [
                // Normal (0–7) — darkened for contrast on the light canvas.
                (0x1d, 0x14, 0x30), // 0 black
                (0xd8, 0x33, 0x6e), // 1 red
                (0x1f, 0x9e, 0x6e), // 2 green
                (0xa9, 0x75, 0x0a), // 3 yellow / amber
                (0x5b, 0x4f, 0xd6), // 4 blue / indigo
                (0x9a, 0x1f, 0xc4), // 5 magenta
                (0x1d, 0x8f, 0xb0), // 6 cyan
                (0x6c, 0x62, 0x85), // 7 white  — --text-faint
                // Bright (8–15) — the lighter, more saturated neon tints.
                (0x9a, 0x8f, 0xb0), // 8  bright black — --text-muted
                (0xff, 0x6b, 0x9d), // 9  bright red    — --term-red
                (0x2b, 0xbd, 0x86), // 10 bright green
                (0xc8, 0x92, 0x00), // 11 bright yellow
                (0x7a, 0x6b, 0xff), // 12 bright blue
                (0xd4, 0x50, 0xfc), // 13 bright magenta — --orchid
                (0x2b, 0xb0, 0xd6), // 14 bright cyan
                (0xec, 0xe6, 0xf5), // 15 bright white   — --text
            ],
        }
    }

    /// Returns all built-in themes in preference order.
    pub fn builtin_themes() -> Vec<Theme> {
        vec![Theme::dark(), Theme::light()]
    }

    /// Look up a built-in theme by name (case-insensitive).
    /// Falls back to `Theme::dark()` for unknown names.
    pub fn by_name(name: &str) -> Theme {
        match name.to_lowercase().as_str() {
            "light" => Theme::light(),
            _ => Theme::dark(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dark_theme_bg_fg() {
        let t = Theme::dark();
        assert_eq!(t.bg, (0x0d, 0x08, 0x14));
        assert_eq!(t.fg, (0xec, 0xe6, 0xf5));
    }

    #[test]
    fn builtin_themes_non_empty() {
        assert!(!Theme::builtin_themes().is_empty());
    }

    #[test]
    fn by_name_fallback() {
        let t = Theme::by_name("unknown");
        assert_eq!(t.name, "dark");
    }

    #[test]
    fn ansi_colors_len() {
        assert_eq!(Theme::dark().ansi_colors.len(), 16);
        assert_eq!(Theme::light().ansi_colors.len(), 16);
    }
}
