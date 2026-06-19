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
            Color::from_rgb(0.925, 0.882, 0.969)
        } else {
            Color::from_rgb(0.075, 0.047, 0.125) // deep violet bar
        })),
        border: Border {
            color: if light {
                Color::from_rgb(0.847, 0.792, 0.918)
            } else {
                Color::from_rgb(0.165, 0.122, 0.239) // --line
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
        (true, true, _) => Color::from_rgb(0.902, 0.847, 0.969),
        (true, false, button::Status::Hovered) => Color::from_rgb(0.925, 0.882, 0.969),
        (true, false, _) => Color::from_rgb(0.953, 0.925, 0.984),
        (false, true, _) => Color::from_rgb(0.157, 0.000, 0.337), // --purple-deep active
        (false, false, button::Status::Hovered) => Color::from_rgb(0.114, 0.078, 0.188),
        (false, false, _) => Color::from_rgb(0.075, 0.047, 0.125),
    };

    let text_color = match (light, is_active) {
        (true, true) => Color::from_rgb(0.114, 0.078, 0.188),
        (true, false) => Color::from_rgb(0.424, 0.384, 0.522),
        (false, true) => Color::from_rgb(0.980, 0.953, 1.000),
        (false, false) => Color::from_rgb(0.604, 0.561, 0.690), // --text-muted
    };

    let border_bottom = match (light, is_active) {
        (true, true) => Color::from_rgb(0.627, 0.129, 0.839), // --purple-mid
        (false, true) => Color::from_rgb(0.976, 0.467, 1.000), // --accent neon magenta
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
        (_, button::Status::Hovered) => Color::from_rgb(1.000, 0.420, 0.616), // --term-red
        (true, _) => Color::from_rgb(0.424, 0.384, 0.522),
        (false, _) => Color::from_rgb(0.424, 0.384, 0.522),
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
        (true, button::Status::Hovered) => Color::from_rgb(0.902, 0.847, 0.969),
        (true, _) => Color::from_rgb(0.925, 0.882, 0.969),
        (false, button::Status::Hovered) => Color::from_rgb(0.165, 0.122, 0.239),
        (false, _) => Color::from_rgb(0.075, 0.047, 0.125),
    };
    button::Style {
        background: Some(Background::Color(bg)),
        text_color: if light {
            Color::from_rgb(0.424, 0.384, 0.522)
        } else {
            Color::from_rgb(0.604, 0.561, 0.690)
        },
        border: Border {
            color: Color::TRANSPARENT,
            width: 0.0,
            radius: 4.0.into(),
        },
        ..Default::default()
    }
}
