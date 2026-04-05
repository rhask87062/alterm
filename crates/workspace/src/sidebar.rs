/// Widget sidebar — a vertical column of icon buttons for creating new blocks.
///
/// Positioned on the right side of the workspace, provides quick access to
/// split the focused pane with different block types.
use iced::widget::{button, column, container, svg, text};
use iced::{Background, Border, Color, Element, Fill, Length, Padding, Theme};

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
pub fn sidebar_view<'a, M: Clone + 'a>(
    map: impl Fn(SidebarAction) -> M + 'a,
) -> Element<'a, M> {
    let btn_size = 36.0;
    let btn_padding = Padding::from([6, 0]);

    let terminal_btn = sidebar_svg_button(
        include_bytes!("../../../assets/icons/sidebar/terminal.svg"),
        Some(map(SidebarAction::NewTerminal)),
        btn_size,
    );
    let ai_btn = sidebar_button("AI", Some(map(SidebarAction::NewAiChat)), btn_size);
    let web_btn = sidebar_svg_button(
        include_bytes!("../../../assets/icons/sidebar/browser.svg"),
        Some(map(SidebarAction::NewBrowser)),
        btn_size,
    );
    let preview_btn = sidebar_svg_button(
        include_bytes!("../../../assets/icons/sidebar/folder.svg"),
        Some(map(SidebarAction::NewPreview)),
        btn_size,
    );
    let settings_btn = sidebar_button("\u{2699}", Some(map(SidebarAction::OpenSettings)), btn_size);
    let info_btn = sidebar_button("?", Some(map(SidebarAction::ShowHotkeyInfo)), btn_size);

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
        .style(|_theme: &Theme| sidebar_container_style())
        .into()
}

/// Build a sidebar button with an SVG icon.
fn sidebar_svg_button<'a, M: Clone + 'a>(
    svg_bytes: &[u8],
    on_press: Option<M>,
    size: f32,
) -> Element<'a, M> {
    let icon = svg(svg::Handle::from_memory(svg_bytes.to_vec()))
        .width(Length::Fixed(20.0))
        .height(Length::Fixed(20.0));

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
            .style(move |_: &Theme, status| sidebar_button_style(status, true));
    } else {
        btn = btn.style(move |_: &Theme, status| sidebar_button_style(status, false));
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
            .style(move |_: &Theme, status| sidebar_button_style(status, true));
    } else {
        btn = btn.style(move |_: &Theme, status| sidebar_button_style(status, false));
    }

    btn.into()
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

fn sidebar_button_style(status: button::Status, enabled: bool) -> button::Style {
    let (bg, text_color) = if !enabled {
        (
            Color::from_rgb(0.09, 0.09, 0.11),
            Color::from_rgb(0.35, 0.35, 0.35),
        )
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
            color: Color::from_rgb(0.20, 0.20, 0.25),
            width: 1.0,
            radius: 4.0.into(),
        },
        ..Default::default()
    }
}
