/// ANSI 256-color palette for the terminal renderer.
///
/// Provides the full 256-entry table used to map ANSI color indices to
/// RGB values:
///   - Indices 0–15:   16 standard colors (dark theme)
///   - Indices 16–231: 6×6×6 RGB cube
///   - Indices 232–255: 24-step grayscale ramp
pub struct AnsiPalette {
    table: [(u8, u8, u8); 256],
}

/// Component values for the 6×6×6 RGB cube.
const CUBE_STEPS: [u8; 6] = [0x00, 0x5f, 0x87, 0xaf, 0xd7, 0xff];

impl Default for AnsiPalette {
    fn default() -> Self {
        let mut table = [(0u8, 0u8, 0u8); 256];

        // Standard 16 colors — "Living Terminal" dark theme, sampled from the
        // app icon / marketing site. Keep in sync with `Theme::dark()` in
        // `crates/config/src/theme.rs`.
        table[0]  = (0x1d, 0x14, 0x30); // black   — --bg-elev-2
        table[1]  = (0xff, 0x6b, 0x9d); // red     — --term-red
        table[2]  = (0x5e, 0xf2, 0xb0); // green   — --term-green
        table[3]  = (0xff, 0xd5, 0x6b); // yellow  — --term-yellow
        table[4]  = (0x8a, 0x7b, 0xff); // blue    — violet-indigo
        table[5]  = (0xd4, 0x50, 0xfc); // magenta — --orchid
        table[6]  = (0x6f, 0xdf, 0xff); // cyan    — --term-cyan
        table[7]  = (0xec, 0xe6, 0xf5); // white   — --text
        table[8]  = (0x6c, 0x62, 0x85); // bright black   — --text-faint
        table[9]  = (0xff, 0x8f, 0xb5); // bright red
        table[10] = (0x7e, 0xf8, 0xc4); // bright green
        table[11] = (0xff, 0xe2, 0x8a); // bright yellow
        table[12] = (0xa9, 0x9b, 0xff); // bright blue
        table[13] = (0xf9, 0x77, 0xff); // bright magenta — --accent
        table[14] = (0x9b, 0xea, 0xff); // bright cyan
        table[15] = (0xfa, 0xf3, 0xff); // bright white

        // 6×6×6 RGB cube — indices 16–231.
        for i in 0u8..216 {
            let r = CUBE_STEPS[(i / 36) as usize];
            let g = CUBE_STEPS[((i % 36) / 6) as usize];
            let b = CUBE_STEPS[(i % 6) as usize];
            table[16 + i as usize] = (r, g, b);
        }

        // 24-step grayscale ramp — indices 232–255.
        // Values: 0x08, 0x12, 0x1c, …, 0xee (step of 10).
        for i in 0u8..24 {
            let v = 0x08u8 + i * 10;
            table[232 + i as usize] = (v, v, v);
        }

        AnsiPalette { table }
    }
}

impl AnsiPalette {
    /// Map an ANSI color index (0–255) to an RGB tuple.
    pub fn ansi_to_rgb(&self, index: u8) -> (u8, u8, u8) {
        self.table[index as usize]
    }

    /// Default foreground color (dark theme) — --text.
    pub fn default_fg() -> (u8, u8, u8) {
        (0xec, 0xe6, 0xf5)
    }

    /// Default background color (dark theme) — deep violet-black --bg.
    pub fn default_bg() -> (u8, u8, u8) {
        (0x0d, 0x08, 0x14)
    }

    /// Default foreground color for light theme — deep violet text.
    pub fn default_fg_light() -> (u8, u8, u8) {
        (0x1d, 0x14, 0x30)
    }

    /// Default background color for light theme — lavender-white.
    pub fn default_bg_light() -> (u8, u8, u8) {
        (0xfa, 0xf3, 0xff)
    }
}
