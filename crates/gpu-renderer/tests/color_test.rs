use gpu_renderer::colors::AnsiPalette;

#[test]
fn test_standard_black() {
    let palette = AnsiPalette::default();
    assert_eq!(palette.ansi_to_rgb(0), (0x1d, 0x14, 0x30));
}

#[test]
fn test_standard_red() {
    let palette = AnsiPalette::default();
    assert_eq!(palette.ansi_to_rgb(1), (0xff, 0x6b, 0x9d));
}

#[test]
fn test_bright_white() {
    let palette = AnsiPalette::default();
    assert_eq!(palette.ansi_to_rgb(15), (0xfa, 0xf3, 0xff));
}

#[test]
fn test_256_color_index_16() {
    let palette = AnsiPalette::default();
    // Start of 6x6x6 cube: r=0, g=0, b=0
    assert_eq!(palette.ansi_to_rgb(16), (0x00, 0x00, 0x00));
}

#[test]
fn test_256_color_index_196() {
    let palette = AnsiPalette::default();
    // Cube index 196: offset = 196 - 16 = 180
    // r = 180 / 36 = 5, g = (180 % 36) / 6 = 0, b = 180 % 6 = 0
    // component[5] = 0xff, component[0] = 0x00
    assert_eq!(palette.ansi_to_rgb(196), (0xff, 0x00, 0x00));
}

#[test]
fn test_grayscale_232() {
    let palette = AnsiPalette::default();
    // First grayscale: 0x08
    assert_eq!(palette.ansi_to_rgb(232), (0x08, 0x08, 0x08));
}

#[test]
fn test_grayscale_255() {
    let palette = AnsiPalette::default();
    // Last grayscale (index 255): 232 + 23 = 255
    // value = 0x08 + 23 * 10 = 0x08 + 230 = 238 = 0xee
    assert_eq!(palette.ansi_to_rgb(255), (0xee, 0xee, 0xee));
}
