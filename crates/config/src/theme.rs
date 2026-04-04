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
    /// Dark theme — macOS-inspired dark palette.
    ///
    /// Background and foreground match the colors used in
    /// `crates/gpu-renderer/src/colors.rs`.
    pub fn dark() -> Self {
        Self {
            name: "dark".to_string(),
            bg: (0x12, 0x12, 0x14),
            fg: (0xe8, 0xe8, 0xed),
            cursor: (0xe8, 0xe8, 0xed),
            selection_bg: (0x3a, 0x3a, 0x44),
            ansi_colors: [
                // Normal (0–7)
                (0x1d, 0x1d, 0x1f), // 0 black
                (0xff, 0x3b, 0x30), // 1 red
                (0x30, 0xd1, 0x58), // 2 green
                (0xff, 0x9f, 0x0a), // 3 yellow
                (0x0a, 0x84, 0xff), // 4 blue
                (0xbf, 0x5a, 0xf2), // 5 magenta
                (0x5a, 0xc8, 0xfa), // 6 cyan
                (0xd1, 0xd1, 0xd6), // 7 white
                // Bright (8–15)
                (0x63, 0x63, 0x66), // 8  bright black
                (0xff, 0x45, 0x3a), // 9  bright red
                (0x30, 0xd1, 0x58), // 10 bright green
                (0xff, 0xd6, 0x0a), // 11 bright yellow
                (0x40, 0x9c, 0xff), // 12 bright blue
                (0xda, 0x8f, 0xff), // 13 bright magenta
                (0x70, 0xd7, 0xff), // 14 bright cyan
                (0xf5, 0xf5, 0xf7), // 15 bright white
            ],
        }
    }

    /// Light theme — soft white background with dark text.
    pub fn light() -> Self {
        Self {
            name: "light".to_string(),
            bg: (0xf5, 0xf5, 0xf7),
            fg: (0x1d, 0x1d, 0x1f),
            cursor: (0x1d, 0x1d, 0x1f),
            selection_bg: (0xc7, 0xd7, 0xf0),
            ansi_colors: [
                // Normal (0–7)
                (0x1d, 0x1d, 0x1f), // 0 black
                (0xc0, 0x22, 0x18), // 1 red
                (0x18, 0x7d, 0x36), // 2 green
                (0x8a, 0x56, 0x00), // 3 yellow
                (0x00, 0x56, 0xc8), // 4 blue
                (0x6e, 0x28, 0x9a), // 5 magenta
                (0x00, 0x6e, 0x9e), // 6 cyan
                (0x5e, 0x5e, 0x60), // 7 white
                // Bright (8–15)
                (0x3a, 0x3a, 0x3c), // 8  bright black
                (0xff, 0x3b, 0x30), // 9  bright red
                (0x30, 0xd1, 0x58), // 10 bright green
                (0xff, 0x9f, 0x0a), // 11 bright yellow
                (0x0a, 0x84, 0xff), // 12 bright blue
                (0xbf, 0x5a, 0xf2), // 13 bright magenta
                (0x5a, 0xc8, 0xfa), // 14 bright cyan
                (0xf5, 0xf5, 0xf7), // 15 bright white
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
        assert_eq!(t.bg, (0x12, 0x12, 0x14));
        assert_eq!(t.fg, (0xe8, 0xe8, 0xed));
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
