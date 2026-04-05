/// Tab bar — a horizontal row of clickable tab buttons.
///
/// Renders one button per tab, with the active tab visually highlighted,
/// a close (X) button on each tab, and a "+" button at the end for creating
/// new tabs.
use iced::widget::{button, container, row, text, Row};
use iced::{Background, Border, Color, Element, Length, Padding, Theme};

/// Returns `true` when the iced theme is light.
fn is_light_theme(theme: &Theme) -> bool {
    matches!(theme, Theme::Light)
}

/// Messages that the tab bar can produce.
/// The consuming application maps these into its own message type.
#[derive(Debug, Clone)]
pub enum TabBarAction {
    Select(usize),
    Close(usize),
    New,
}

/// Render the tab bar as a horizontal row.
///
/// - `titles`: slice of tab titles
/// - `active`: index of the currently active tab
/// - `map`: closure that converts a `TabBarAction` into the app's message type
pub fn tab_bar_view<'a, M: Clone + 'a>(
    titles: &[String],
    active: usize,
    map: impl Fn(TabBarAction) -> M + 'a,
) -> Element<'a, M> {
    let mut tabs: Vec<Element<'a, M>> = Vec::new();

    for (i, title) in titles.iter().enumerate() {
        let is_active = i == active;

        // Tab label text.
        let label = text(title.clone()).size(13);

        // Close button (X) — always visible but smaller.
        let close_btn = button(text("x").size(11))
            .on_press(map(TabBarAction::Close(i)))
            .padding(Padding::from([1, 4]))
            .style(move |theme: &Theme, status| close_button_style(theme, status));

        // Tab content: label + close button in a row.
        let tab_content = row![label, close_btn]
            .spacing(6)
            .align_y(iced::Alignment::Center);

        // The whole tab is a button.
        let tab_btn = button(tab_content)
            .on_press(map(TabBarAction::Select(i)))
            .padding(Padding::from([6, 12]))
            .style(move |theme: &Theme, status| tab_button_style(theme, status, is_active));

        tabs.push(tab_btn.into());
    }

    // "+" button to create a new tab.
    let new_tab_btn = button(text("+").size(14))
        .on_press(map(TabBarAction::New))
        .padding(Padding::from([6, 10]))
        .style(|theme: &Theme, status| new_tab_button_style(theme, status));

    tabs.push(new_tab_btn.into());

    let bar = Row::from_vec(tabs)
        .spacing(2)
        .align_y(iced::Alignment::Center);

    container(bar)
        .width(Length::Fill)
        .padding(Padding::from([4, 4]))
        .style(|theme: &Theme| tab_bar_container_style(theme))
        .into()
}

// ---------------------------------------------------------------------------
// Styles
// ---------------------------------------------------------------------------

fn tab_bar_container_style(theme: &Theme) -> iced::widget::container::Style {
    let light = is_light_theme(theme);
    iced::widget::container::Style {
        background: Some(Background::Color(if light {
            Color::from_rgb(0.93, 0.93, 0.95)
        } else {
            Color::from_rgb(0.06, 0.06, 0.08)
        })),
        border: Border {
            color: if light {
                Color::from_rgb(0.82, 0.82, 0.86)
            } else {
                Color::from_rgb(0.12, 0.12, 0.15)
            },
            width: 0.0,
            radius: 0.0.into(),
        },
        ..Default::default()
    }
}

fn tab_button_style(
    theme: &Theme,
    status: button::Status,
    is_active: bool,
) -> button::Style {
    let light = is_light_theme(theme);

    let bg = match (light, is_active, status) {
        (true, true, _) => Color::from_rgb(0.88, 0.90, 0.95),
        (true, false, button::Status::Hovered) => Color::from_rgb(0.88, 0.88, 0.92),
        (true, false, _) => Color::from_rgb(0.93, 0.93, 0.95),
        (false, true, _) => Color::from_rgb(0.16, 0.18, 0.26),
        (false, false, button::Status::Hovered) => Color::from_rgb(0.13, 0.13, 0.17),
        (false, false, _) => Color::from_rgb(0.08, 0.08, 0.10),
    };

    let text_color = match (light, is_active) {
        (true, true) => Color::from_rgb(0.10, 0.10, 0.15),
        (true, false) => Color::from_rgb(0.45, 0.45, 0.55),
        (false, true) => Color::from_rgb(0.95, 0.96, 1.0),
        (false, false) => Color::from_rgb(0.45, 0.45, 0.48),
    };

    let border_bottom = match (light, is_active) {
        (true, true) => Color::from_rgb(0.20, 0.45, 0.80),
        (false, true) => Color::from_rgb(0.40, 0.60, 0.95),
        _ => Color::TRANSPARENT,
    };

    button::Style {
        background: Some(Background::Color(bg)),
        text_color,
        border: Border {
            color: border_bottom,
            width: if is_active { 1.0 } else { 0.0 },
            radius: 4.0.into(),
        },
        ..Default::default()
    }
}

fn close_button_style(
    theme: &Theme,
    status: button::Status,
) -> button::Style {
    let light = is_light_theme(theme);
    let text_color = match (light, status) {
        (_, button::Status::Hovered) => Color::from_rgb(0.9, 0.3, 0.3),
        (true, _) => Color::from_rgb(0.55, 0.55, 0.60),
        (false, _) => Color::from_rgb(0.4, 0.4, 0.4),
    };
    button::Style {
        background: Some(Background::Color(Color::TRANSPARENT)),
        text_color,
        border: Border {
            color: Color::TRANSPARENT,
            width: 0.0,
            radius: 2.0.into(),
        },
        ..Default::default()
    }
}

fn new_tab_button_style(
    theme: &Theme,
    status: button::Status,
) -> button::Style {
    let light = is_light_theme(theme);
    let bg = match (light, status) {
        (true, button::Status::Hovered) => Color::from_rgb(0.85, 0.85, 0.90),
        (true, _) => Color::from_rgb(0.90, 0.90, 0.92),
        (false, button::Status::Hovered) => Color::from_rgb(0.16, 0.16, 0.20),
        (false, _) => Color::from_rgb(0.10, 0.10, 0.12),
    };
    button::Style {
        background: Some(Background::Color(bg)),
        text_color: if light {
            Color::from_rgb(0.40, 0.40, 0.50)
        } else {
            Color::from_rgb(0.55, 0.55, 0.58)
        },
        border: Border {
            color: Color::TRANSPARENT,
            width: 0.0,
            radius: 4.0.into(),
        },
        ..Default::default()
    }
}
