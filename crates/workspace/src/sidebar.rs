/// Widget sidebar — a vertical column of icon buttons for creating new blocks.
///
/// Positioned on the right side of the workspace, provides quick access to
/// split the focused pane with different block types.
use iced::widget::{button, column, container, svg, text, tooltip};
use iced::{Background, Border, Color, Element, Fill, Length, Padding, Theme};
use crate::keybindings::Action;

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
    /// Toggle between light and dark theme.
    ToggleTheme,
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
    let btn_padding = Padding::from([6, 4]);

    let terminal_btn = with_tooltip(
        sidebar_svg_button(
            &theme_svg(include_bytes!("../../../assets/icons/sidebar/terminal.svg"), light_mode),
            Some(map(SidebarAction::NewTerminal)),
            btn_size,
        ),
        tip_text(Action::NewTerminal),
    );
    let ai_btn = with_tooltip(
        sidebar_button("AI", Some(map(SidebarAction::NewAiChat)), btn_size),
        tip_text(Action::ToggleAIChat),
    );
    let web_btn = with_tooltip(
        sidebar_svg_button(
            &theme_svg(include_bytes!("../../../assets/icons/sidebar/browser.svg"), light_mode),
            Some(map(SidebarAction::NewBrowser)),
            btn_size,
        ),
        tip_text(Action::NewBrowser),
    );
    let preview_btn = with_tooltip(
        sidebar_svg_button_with_icon_size(
            &theme_svg(include_bytes!("../../../assets/icons/sidebar/folder.svg"), light_mode),
            Some(map(SidebarAction::NewPreview)),
            btn_size,
            24.0,  // slightly larger icon for folder
        ),
        tip_text(Action::NewPreview),
    );
    let settings_btn = with_tooltip(
        sidebar_svg_button(
            &theme_svg(include_bytes!("../../../assets/icons/sidebar/settings-svgrepo-com.svg"), light_mode),
            Some(map(SidebarAction::OpenSettings)),
            btn_size,
        ),
        tip_text(Action::OpenSettings),
    );
    let info_btn = with_tooltip(
        sidebar_svg_button(
            &theme_svg(include_bytes!("../../../assets/icons/sidebar/info.svg"), light_mode),
            Some(map(SidebarAction::ShowHotkeyInfo)),
            btn_size,
        ),
        tip_text(Action::ShowHotkeyInfo),
    );

    // Show the icon of the mode to switch TO: sun in dark mode, moon in light mode.
    let theme_icon_bytes: &[u8] = if light_mode {
        include_bytes!("../../../assets/icons/sidebar/darkmode.svg")
    } else {
        include_bytes!("../../../assets/icons/sidebar/lightmode.svg")
    };
    let theme_btn = with_tooltip(
        sidebar_svg_button(
            &theme_svg(theme_icon_bytes, light_mode),
            Some(map(SidebarAction::ToggleTheme)),
            btn_size,
        ),
        tip_text(Action::ToggleTheme),
    );

    let top_buttons = column![terminal_btn, ai_btn, web_btn, preview_btn, settings_btn]
        .spacing(4)
        .align_x(iced::Alignment::Center);

    let bottom_buttons = column![theme_btn, info_btn]
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

/// Tooltip text for a sidebar button: "Label  (Ctrl+Shift+X)".
fn tip_text(action: Action) -> String {
    format!("{}  ({})", action.label(), action.shortcut_hint())
}

/// Wrap a built sidebar button in a left-positioned hover tooltip.
fn with_tooltip<'a, M: 'a>(content: Element<'a, M>, hint: String) -> Element<'a, M> {
    tooltip(
        content,
        container(text(hint).size(12))
            .padding(Padding::from([4, 8]))
            .style(tooltip_box_style),
        tooltip::Position::Left,
    )
    .gap(6)
    .into()
}

/// Styled background box for sidebar tooltips (theme-aware).
///
/// The box stays dark in both themes (light text on a dark box reads well over
/// either background); the light branch is only slightly lighter for contrast.
fn tooltip_box_style(theme: &Theme) -> iced::widget::container::Style {
    let light = is_light_theme(theme);
    iced::widget::container::Style {
        background: Some(Background::Color(if light {
            Color::from_rgb(0.165, 0.122, 0.239)
        } else {
            Color::from_rgb(0.114, 0.078, 0.188) // --bg-elev-2
        })),
        text_color: Some(Color::from_rgb(0.925, 0.902, 0.961)),
        border: Border {
            color: if light {
                Color::from_rgb(0.290, 0.208, 0.408)
            } else {
                Color::from_rgb(0.239, 0.173, 0.341) // --line-bright
            },
            width: 1.0,
            radius: 4.0.into(),
        },
        ..Default::default()
    }
}

// ---------------------------------------------------------------------------
// Styles
// ---------------------------------------------------------------------------

fn sidebar_container_style(theme: &Theme) -> iced::widget::container::Style {
    let light = is_light_theme(theme);
    iced::widget::container::Style {
        background: Some(Background::Color(if light {
            Color::from_rgb(0.925, 0.882, 0.969)
        } else {
            Color::from_rgb(0.067, 0.039, 0.110) // violet sidebar
        })),
        border: Border {
            color: if light {
                Color::from_rgb(0.847, 0.792, 0.918)
            } else {
                Color::from_rgb(0.165, 0.122, 0.239) // --line
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
                Color::from_rgb(0.075, 0.047, 0.125),
                Color::from_rgb(0.424, 0.384, 0.522), // --text-faint
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
                Color::from_rgb(0.239, 0.173, 0.341), // --line-bright
                Color::from_rgb(0.925, 0.902, 0.961),
            ),
            button::Status::Pressed => (
                Color::from_rgb(0.290, 0.208, 0.408),
                Color::WHITE,
            ),
            _ => (
                Color::from_rgb(0.114, 0.078, 0.188), // --bg-elev-2
                Color::from_rgb(0.851, 0.820, 0.910),
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
