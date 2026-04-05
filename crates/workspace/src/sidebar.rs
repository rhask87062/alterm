/// Widget sidebar — a vertical column of icon buttons for creating new blocks.
///
/// Positioned on the right side of the workspace, provides quick access to
/// split the focused pane with different block types.
use iced::widget::{button, column, container, text, tooltip};
use iced::{Background, Border, Color, Element, Length, Padding, Theme};

/// Actions the sidebar can produce.
#[derive(Debug, Clone)]
pub enum SidebarAction {
    /// Split the focused pane with a new terminal.
    NewTerminal,
    /// Split the focused pane with a new AI chat.
    NewAiChat,
    /// Split the focused pane with a new browser.
    NewBrowser,
    /// Open the settings panel in a pane.
    OpenSettings,
}

/// Render the sidebar as a vertical column of square icon buttons.
///
/// - `map`: closure that converts a `SidebarAction` into the app's message type
pub fn sidebar_view<'a, M: Clone + 'a>(
    map: impl Fn(SidebarAction) -> M + 'a,
) -> Element<'a, M> {
    let btn_size = 36.0;
    let btn_padding = Padding::from([6, 0]);

    // Terminal button — active
    let terminal_btn = sidebar_button(
        "T",
        "New terminal (split)",
        Some(map(SidebarAction::NewTerminal)),
        btn_size,
        true,
    );

    // AI button — active
    let ai_btn = sidebar_button(
        "AI",
        "AI chat (split)",
        Some(map(SidebarAction::NewAiChat)),
        btn_size,
        true,
    );

    // Web button — active
    let web_btn = sidebar_button(
        "W",
        "Web browser (split)",
        Some(map(SidebarAction::NewBrowser)),
        btn_size,
        true,
    );

    // Settings button — active
    let settings_btn = sidebar_button(
        "\u{2699}",
        "Settings (Ctrl+Shift+,)",
        Some(map(SidebarAction::OpenSettings)),
        btn_size,
        true,
    );

    let col = column![terminal_btn, ai_btn, web_btn, settings_btn]
        .spacing(4)
        .padding(btn_padding)
        .align_x(iced::Alignment::Center);

    container(col)
        .width(Length::Fixed(44.0))
        .height(Length::Fill)
        .style(|_theme: &Theme| sidebar_container_style())
        .into()
}

/// Build a single sidebar button with a tooltip.
fn sidebar_button<'a, M: Clone + 'a>(
    label: &'a str,
    tip: &'a str,
    on_press: Option<M>,
    size: f32,
    enabled: bool,
) -> Element<'a, M> {
    let label_widget = text(label)
        .size(14)
        .center();

    let mut btn = button(label_widget)
        .width(Length::Fixed(size))
        .height(Length::Fixed(size))
        .padding(0);

    if let Some(msg) = on_press {
        btn = btn
            .on_press(msg)
            .style(move |theme: &Theme, status| sidebar_button_style(theme, status, true));
    } else {
        btn = btn
            .style(move |theme: &Theme, status| sidebar_button_style(theme, status, false));
    }

    let tip_content = container(text(tip).size(12))
        .padding(Padding::from([4, 8]))
        .style(|_theme: &Theme| tooltip_style());

    let _ = enabled; // used via on_press being Some/None

    tooltip(btn, tip_content, tooltip::Position::Left)
        .gap(6)
        .into()
}

// ---------------------------------------------------------------------------
// Styles
// ---------------------------------------------------------------------------

fn sidebar_container_style() -> iced::widget::container::Style {
    iced::widget::container::Style {
        background: Some(Background::Color(Color::from_rgb(0.07, 0.07, 0.09))),
        border: Border {
            color: Color::from_rgb(0.15, 0.15, 0.18),
            width: 1.0,
            radius: 0.0.into(),
        },
        ..Default::default()
    }
}

fn sidebar_button_style(
    _theme: &Theme,
    _status: button::Status,
    enabled: bool,
) -> button::Style {
    let (bg, text_color) = if enabled {
        (
            Color::from_rgb(0.14, 0.14, 0.18),
            Color::from_rgb(0.85, 0.85, 0.85),
        )
    } else {
        (
            Color::from_rgb(0.09, 0.09, 0.11),
            Color::from_rgb(0.35, 0.35, 0.35),
        )
    };

    button::Style {
        background: Some(Background::Color(bg)),
        text_color,
        border: Border {
            color: Color::from_rgb(0.20, 0.20, 0.25),
            width: 1.0,
            radius: 4.0.into(),
        },
        ..Default::default()
    }
}

fn tooltip_style() -> iced::widget::container::Style {
    iced::widget::container::Style {
        background: Some(Background::Color(Color::from_rgb(0.15, 0.15, 0.20))),
        text_color: Some(Color::from_rgb(0.9, 0.9, 0.9)),
        border: Border {
            color: Color::from_rgb(0.25, 0.25, 0.30),
            width: 1.0,
            radius: 4.0.into(),
        },
        ..Default::default()
    }
}
