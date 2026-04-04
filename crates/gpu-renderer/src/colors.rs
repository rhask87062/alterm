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

        // Standard 16 colors — dark macOS-inspired theme.
        table[0]  = (0x1d, 0x1d, 0x1f); // black
        table[1]  = (0xff, 0x3b, 0x30); // red
        table[2]  = (0x30, 0xd1, 0x58); // green
        table[3]  = (0xff, 0x9f, 0x0a); // yellow
        table[4]  = (0x0a, 0x84, 0xff); // blue
        table[5]  = (0xbf, 0x5a, 0xf2); // magenta
        table[6]  = (0x5a, 0xc8, 0xfa); // cyan
        table[7]  = (0xd1, 0xd1, 0xd6); // white
        table[8]  = (0x63, 0x63, 0x66); // bright black
        table[9]  = (0xff, 0x45, 0x3a); // bright red
        table[10] = (0x30, 0xd1, 0x58); // bright green
        table[11] = (0xff, 0xd6, 0x0a); // bright yellow
        table[12] = (0x40, 0x9c, 0xff); // bright blue
        table[13] = (0xda, 0x8f, 0xff); // bright magenta
        table[14] = (0x70, 0xd7, 0xff); // bright cyan
        table[15] = (0xf5, 0xf5, 0xf7); // bright white

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

    /// Default foreground color.
    pub fn default_fg() -> (u8, u8, u8) {
        (0xe8, 0xe8, 0xed)
    }

    /// Default background color.
    pub fn default_bg() -> (u8, u8, u8) {
        (0x12, 0x12, 0x14)
    }
}
