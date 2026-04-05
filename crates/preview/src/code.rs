use syntect::easy::HighlightLines;
use syntect::highlighting::{FontStyle, ThemeSet};
use syntect::parsing::SyntaxSet;

pub struct HighlightedLine {
    pub line_number: usize,
    pub spans: Vec<HighlightedSpan>,
}

pub struct HighlightedSpan {
    pub text: String,
    pub fg: (u8, u8, u8),
    pub bg: (u8, u8, u8),
    pub bold: bool,
    pub italic: bool,
}

pub fn highlight_file(source: &str, language: &str) -> Vec<HighlightedLine> {
    let ss = SyntaxSet::load_defaults_newlines();
    let ts = ThemeSet::load_defaults();
    let theme = &ts.themes["base16-ocean.dark"];

    let syntax = ss
        .find_syntax_by_name(language)
        .or_else(|| ss.find_syntax_by_extension(&language.to_lowercase()))
        .unwrap_or_else(|| ss.find_syntax_plain_text());

    let mut h = HighlightLines::new(syntax, theme);
    let mut lines = Vec::new();

    for (i, line) in source.lines().enumerate() {
        let ranges = h.highlight_line(line, &ss).unwrap_or_default();
        let spans: Vec<HighlightedSpan> = ranges
            .iter()
            .map(|(style, text)| HighlightedSpan {
                text: text.to_string(),
                fg: (
                    style.foreground.r,
                    style.foreground.g,
                    style.foreground.b,
                ),
                bg: (
                    style.background.r,
                    style.background.g,
                    style.background.b,
                ),
                bold: style.font_style.contains(FontStyle::BOLD),
                italic: style.font_style.contains(FontStyle::ITALIC),
            })
            .collect();

        lines.push(HighlightedLine {
            line_number: i + 1,
            spans,
        });
    }

    lines
}
