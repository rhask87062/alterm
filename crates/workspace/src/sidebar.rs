/// Widget sidebar — a vertical column of icon buttons for creating new blocks.
///
/// Positioned on the right side of the workspace, provides quick access to
/// split the focused pane with different block types.
use iced::widget::{button, column, container, svg, text};
use iced::{Background, Border, Color, Element, Fill, Length, Padding, Theme};

/// Returns `true` when the iced theme is light.
fn is_light_theme(theme: &Theme) -> bool {
    matches!(theme, Theme::Light)
}

/// Actions the sidebar can produce.
#[derive(Debug, Clone)]
pub enum SidebarAction {
    /// Split the focused pane with a new terminal.
    NewTerminal,
    /// Split the focused pane with a new AI chat.
    NewAiChat,
    /// Split the focused pane with a new browser.
    NewBrowser,
    /// Split the focused pane with a new file preview.
    NewPreview,
    /// Open the settings panel in a pane.
    OpenSettings,
    /// Show the keyboard shortcuts reference pane.
    ShowHotkeyInfo,
}

/// Render the sidebar as a vertical column of square icon buttons.
///
/// `light_mode` controls which SVG icon variant is used (dark icons on
/// light backgrounds and vice-versa).
pub fn sidebar_view<'a, M: Clone + 'a>(
    map: impl Fn(SidebarAction) -> M + 'a,
    light_mode: bool,
) -> Element<'a, M> {
    let btn_size = 36.0;
    let btn_padding = Padding::from([6, 0]);

    let terminal_btn = sidebar_svg_button(
        &theme_svg(include_bytes!("../../../assets/icons/sidebar/terminal.svg"), light_mode),
        Some(map(SidebarAction::NewTerminal)),
        btn_size,
    );
    let ai_btn = sidebar_button("AI", Some(map(SidebarAction::NewAiChat)), btn_size);
    let web_btn = sidebar_svg_button(
        &theme_svg(include_bytes!("../../../assets/icons/sidebar/browser.svg"), light_mode),
        Some(map(SidebarAction::NewBrowser)),
        btn_size,
    );
    let preview_btn = sidebar_svg_button_with_icon_size(
        &theme_svg(include_bytes!("../../../assets/icons/sidebar/folder.svg"), light_mode),
        Some(map(SidebarAction::NewPreview)),
        btn_size,
        24.0,  // slightly larger icon for folder
    );
    let settings_btn = sidebar_svg_button(
        &theme_svg(include_bytes!("../../../assets/icons/sidebar/settings-svgrepo-com.svg"), light_mode),
        Some(map(SidebarAction::OpenSettings)),
        btn_size,
    );
    let info_btn = sidebar_svg_button(
        &theme_svg(include_bytes!("../../../assets/icons/sidebar/info.svg"), light_mode),
        Some(map(SidebarAction::ShowHotkeyInfo)),
        btn_size,
    );

    let top_buttons = column![terminal_btn, ai_btn, web_btn, preview_btn, settings_btn]
        .spacing(4)
        .align_x(iced::Alignment::Center);

    let bottom_buttons = column![info_btn]
        .spacing(4)
        .align_x(iced::Alignment::Center);

    let col = column![
        top_buttons,
        iced::widget::space().height(Fill),
        bottom_buttons,
    ]
    .padding(btn_padding)
    .align_x(iced::Alignment::Center);

    container(col)
        .width(Length::Fixed(44.0))
        .height(Length::Fill)
        .style(|theme: &Theme| sidebar_container_style(theme))
        .into()
}

/// Swap hardcoded SVG icon colors for the current theme.
///
/// In dark mode the original `#CCCCCC` / `#000000` strokes are fine; in light
/// mode we replace them with darker / lighter variants so the icons stay
/// visible against the lighter sidebar background.
fn theme_svg(svg_bytes: &[u8], light_mode: bool) -> Vec<u8> {
    if light_mode {
        let s = String::from_utf8_lossy(svg_bytes);
        s.replace("#CCCCCC", "#333333")
            .replace("#cccccc", "#333333")
            .replace("#000000", "#333333")
            .replace("fill:#CCCCCC", "fill:#333333")
            .into_bytes()
    } else {
        // Dark mode: the info.svg has fill="#000000" which is invisible on
        // dark backgrounds — swap it to light gray.
        let s = String::from_utf8_lossy(svg_bytes);
        s.replace("#000000", "#CCCCCC").into_bytes()
    }
}

/// Build a sidebar button with an SVG icon.
fn sidebar_svg_button<'a, M: Clone + 'a>(
    svg_bytes: &[u8],
    on_press: Option<M>,
    size: f32,
) -> Element<'a, M> {
    sidebar_svg_button_with_icon_size(svg_bytes, on_press, size, 20.0)
}

/// Build a sidebar button with an SVG icon at a custom icon size.
fn sidebar_svg_button_with_icon_size<'a, M: Clone + 'a>(
    svg_bytes: &[u8],
    on_press: Option<M>,
    size: f32,
    icon_size: f32,
) -> Element<'a, M> {
    let icon = svg(svg::Handle::from_memory(svg_bytes.to_vec()))
        .width(Length::Fixed(icon_size))
        .height(Length::Fixed(icon_size));

    let icon_container = container(icon)
        .width(Length::Fixed(size))
        .height(Length::Fixed(size))
        .center_x(Length::Fixed(size))
        .center_y(Length::Fixed(size));

    let mut btn = button(icon_container)
        .width(Length::Fixed(size))
        .height(Length::Fixed(size))
        .padding(0);

    if let Some(msg) = on_press {
        btn = btn
            .on_press(msg)
            .style(move |theme: &Theme, status| sidebar_button_style(theme, status, true));
    } else {
        btn = btn.style(move |theme: &Theme, status| sidebar_button_style(theme, status, false));
    }

    btn.into()
}

/// Build a single sidebar button with text label (no tooltip).
fn sidebar_button<'a, M: Clone + 'a>(
    label: &'a str,
    on_press: Option<M>,
    size: f32,
) -> Element<'a, M> {
    let label_widget = text(label).size(14).center();

    let mut btn = button(label_widget)
        .width(Length::Fixed(size))
        .height(Length::Fixed(size))
        .padding(0);

    if let Some(msg) = on_press {
        btn = btn
            .on_press(msg)
            .style(move |theme: &Theme, status| sidebar_button_style(theme, status, true));
    } else {
        btn = btn.style(move |theme: &Theme, status| sidebar_button_style(theme, status, false));
    }

    btn.into()
}

// ---------------------------------------------------------------------------
// Styles
// ---------------------------------------------------------------------------

fn sidebar_container_style(theme: &Theme) -> iced::widget::container::Style {
    let light = is_light_theme(theme);
    iced::widget::container::Style {
        background: Some(Background::Color(if light {
            Color::from_rgb(0.92, 0.92, 0.94)
        } else {
            Color::from_rgb(0.07, 0.07, 0.09)
        })),
        border: Border {
            color: if light {
                Color::from_rgb(0.82, 0.82, 0.86)
            } else {
                Color::from_rgb(0.15, 0.15, 0.18)
            },
            width: 1.0,
            radius: 0.0.into(),
        },
        ..Default::default()
    }
}

fn sidebar_button_style(theme: &Theme, status: button::Status, enabled: bool) -> button::Style {
    let light = is_light_theme(theme);

    let (bg, text_color) = if !enabled {
        if light {
            (
                Color::from_rgb(0.90, 0.90, 0.92),
                Color::from_rgb(0.60, 0.60, 0.65),
            )
        } else {
            (
                Color::from_rgb(0.09, 0.09, 0.11),
                Color::from_rgb(0.35, 0.35, 0.35),
            )
        }
    } else if light {
        match status {
            button::Status::Hovered => (
                Color::from_rgb(0.82, 0.82, 0.88),
                Color::from_rgb(0.10, 0.10, 0.15),
            ),
            button::Status::Pressed => (
                Color::from_rgb(0.78, 0.78, 0.84),
                Color::BLACK,
            ),
            _ => (
                Color::from_rgb(0.88, 0.88, 0.92),
                Color::from_rgb(0.20, 0.20, 0.25),
            ),
        }
    } else {
        match status {
            button::Status::Hovered => (
                Color::from_rgb(0.20, 0.20, 0.26),
                Color::from_rgb(0.95, 0.95, 0.95),
            ),
            button::Status::Pressed => (
                Color::from_rgb(0.24, 0.24, 0.30),
                Color::WHITE,
            ),
            _ => (
                Color::from_rgb(0.14, 0.14, 0.18),
                Color::from_rgb(0.85, 0.85, 0.85),
            ),
        }
    };

    button::Style {
        background: Some(Background::Color(bg)),
        text_color,
        border: Border {
            color: if light {
                Color::from_rgb(0.78, 0.78, 0.82)
            } else {
                Color::from_rgb(0.20, 0.20, 0.25)
            },
            width: 1.0,
            radius: 4.0.into(),
        },
        ..Default::default()
    }
}
