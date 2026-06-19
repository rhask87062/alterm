/// Tab bar — a horizontal row of clickable tab buttons.
///
/// Renders one button per tab, with the active tab visually highlighted,
/// a close (X) button on each tab, and a "+" button at the end for creating
/// new tabs.
use std::rc::Rc;

use iced::widget::{button, container, row, text, text_input, Row};
use iced::{Background, Border, Color, Element, Length, Padding, Theme};
use crate::chrome;

/// Messages that the tab bar can produce.
/// The consuming application maps these into its own message type.
///
/// Note: double-click-to-rename is detected by the application from repeated
/// `Select` clicks (wrapping the tab button in a `mouse_area` doesn't work —
/// the button captures the press before the area sees the double-click).
#[derive(Debug, Clone)]
pub enum TabBarAction {
    Select(usize),
    Close(usize),
    New,
    /// The rename text field changed.
    RenameInput(String),
    /// Commit the in-progress rename (Enter).
    RenameSubmit,
}

/// Render the tab bar as a horizontal row.
///
/// - `titles`: slice of tab titles
/// - `active`: index of the currently active tab
/// - `editing`: index of the tab currently being renamed inline (if any)
/// - `edit_value`: current text in the rename field (used when `editing` is set)
/// - `rename_id`: widget id of the rename field, so the caller can focus it
/// - `map`: closure that converts a `TabBarAction` into the app's message type
pub fn tab_bar_view<'a, M: Clone + 'a>(
    titles: &[String],
    active: usize,
    editing: Option<usize>,
    edit_value: &str,
    rename_id: iced::widget::Id,
    map: impl Fn(TabBarAction) -> M + 'a,
) -> Element<'a, M> {
    // Shared so it can be cloned into the on_input closure while remaining
    // callable for the other tabs.
    let map = Rc::new(map);
    let mut tabs: Vec<Element<'a, M>> = Vec::new();

    for (i, title) in titles.iter().enumerate() {
        let is_active = i == active;

        // While this tab is being renamed, show an inline text field instead.
        if editing == Some(i) {
            let map_input = map.clone();
            let input = text_input("", edit_value)
                .id(rename_id.clone())
                .on_input(move |s| (*map_input)(TabBarAction::RenameInput(s)))
                .on_submit((*map)(TabBarAction::RenameSubmit))
                .size(13)
                .padding(Padding::from([4, 10]))
                .width(Length::Fixed(140.0));
            tabs.push(input.into());
            continue;
        }

        // Tab label text.
        let label = text(title.clone()).size(13);

        // Close button (X) — always visible but smaller.
        let close_btn = button(text("x").size(11))
            .on_press((*map)(TabBarAction::Close(i)))
            .padding(Padding::from([1, 4]))
            .style(move |theme: &Theme, status| close_button_style(theme, status));

        // Tab content: label + close button in a row.
        let tab_content = row![label, close_btn]
            .spacing(6)
            .align_y(iced::Alignment::Center);

        // The whole tab is a button. A single click selects; the application
        // turns a quick second click on the active tab into a rename.
        let tab_btn = button(tab_content)
            .on_press((*map)(TabBarAction::Select(i)))
            .padding(Padding::from([6, 12]))
            .style(move |theme: &Theme, status| tab_button_style(theme, status, is_active));

        tabs.push(tab_btn.into());
    }

    // "+" button to create a new tab.
    let new_tab_btn = button(text("+").size(14))
        .on_press((*map)(TabBarAction::New))
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
    iced::widget::container::Style {
        background: Some(Background::Color(chrome::bg_subtle(theme))),
        border: Border {
            color: chrome::line(theme),
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
    // The active tab is highlighted with the theme's accent tint; inactive
    // tabs sit on the bar and lift slightly on hover.
    let bg = match (is_active, status) {
        (true, _) => chrome::accent_subtle(theme),
        (false, button::Status::Hovered) => chrome::bg_raised(theme),
        (false, _) => chrome::bg_subtle(theme),
    };

    let text_color = match is_active {
        true => chrome::accent_text(theme),
        false => chrome::text_muted(theme),
    };

    let border_bottom = if is_active {
        chrome::accent(theme)
    } else {
        Color::TRANSPARENT
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
    let text_color = match status {
        button::Status::Hovered => chrome::danger(theme),
        _ => chrome::text_muted(theme),
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
    let bg = match status {
        button::Status::Hovered => chrome::bg_raised(theme),
        _ => chrome::bg_subtle(theme),
    };
    button::Style {
        background: Some(Background::Color(bg)),
        text_color: chrome::text_muted(theme),
        border: Border {
            color: Color::TRANSPARENT,
            width: 0.0,
            radius: 4.0.into(),
        },
        ..Default::default()
    }
}
