use std::time::Duration;

use iced::event::Status;
use iced::keyboard::key::Named;
use iced::keyboard::{Key, Modifiers};
use iced::widget::{button, column, container, opaque, pane_grid, row, stack, text, text_input, Column};
use iced::window;
use iced::{Background, Border, Color, Element, Event, Fill, Length, Subscription, Task, Theme};

use gpu_renderer::widget::TerminalView;
use workspace::{
    match_shortcut, sidebar_view, tab_bar_view, Action, Block, CommandPalette, SidebarAction, Tab,
    TabBarAction,
};

fn main() -> iced::Result {
    env_logger::init();

    iced::application(Altermative::new, Altermative::update, Altermative::view)
        .title("Altermative")
        .theme(Theme::Dark)
        .window_size((900.0, 600.0))
        .subscription(Altermative::subscription)
        .run()
}

struct Altermative {
    tabs: Vec<Tab>,
    active_tab: usize,
    palette: CommandPalette,
}

#[derive(Debug, Clone)]
enum Message {
    Tick,
    KeyboardInput(Key, Modifiers),
    MouseScroll(f32),
    ClipboardContent(Option<String>),
    PaneClicked(pane_grid::Pane),
    PaneDragged(pane_grid::DragEvent),
    PaneResized(pane_grid::ResizeEvent),
    SplitHorizontal,
    SplitVertical,
    ClosePane,
    MaximizeToggle,
    // Tab management
    NewTab,
    CloseTab(usize),
    SelectTab(usize),
    TabBarAction(TabBarAction),
    // Sidebar
    SidebarAction(SidebarAction),
    SidebarNewTerminal,
    // Command palette
    PaletteQueryChanged(String),
    PaletteSubmit,
}

impl Altermative {
    fn new() -> (Self, Task<Message>) {
        let first_tab = Tab::new().expect("Failed to create initial tab");

        let app = Altermative {
            tabs: vec![first_tab],
            active_tab: 0,
            palette: CommandPalette::new(),
        };

        (app, Task::none())
    }

    /// Get a reference to the active tab.
    fn active_tab(&self) -> &Tab {
        &self.tabs[self.active_tab]
    }

    /// Get a mutable reference to the active tab.
    fn active_tab_mut(&mut self) -> &mut Tab {
        &mut self.tabs[self.active_tab]
    }

    /// Move focus to the adjacent pane in the given direction.
    fn focus_adjacent(&mut self, direction: pane_grid::Direction) {
        let tab = self.active_tab_mut();
        if let Some(focused) = tab.focus {
            if let Some(adjacent) = tab.panes.adjacent(focused, direction) {
                tab.focus = Some(adjacent);
            }
        }
    }

    /// Dispatch a keybinding [`Action`] into the appropriate [`Message`].
    fn dispatch_action(&mut self, action: Action) -> Task<Message> {
        match action {
            Action::NewTab => self.update(Message::NewTab),
            Action::CloseTab => {
                let idx = self.active_tab;
                self.update(Message::CloseTab(idx))
            }
            Action::NextTab => {
                if self.tabs.len() > 1 {
                    let next = (self.active_tab + 1) % self.tabs.len();
                    self.update(Message::SelectTab(next))
                } else {
                    Task::none()
                }
            }
            Action::PrevTab => {
                if self.tabs.len() > 1 {
                    let prev = if self.active_tab == 0 {
                        self.tabs.len() - 1
                    } else {
                        self.active_tab - 1
                    };
                    self.update(Message::SelectTab(prev))
                } else {
                    Task::none()
                }
            }
            Action::JumpToTab(n) => {
                let idx = n - 1;
                if idx < self.tabs.len() {
                    self.update(Message::SelectTab(idx))
                } else {
                    Task::none()
                }
            }
            Action::RenameTab => {
                log::debug!("RenameTab — not yet implemented");
                Task::none()
            }
            Action::SplitRight => self.update(Message::SplitHorizontal),
            Action::SplitDown => self.update(Message::SplitVertical),
            Action::ClosePane => self.update(Message::ClosePane),
            Action::MaximizeToggle => self.update(Message::MaximizeToggle),
            Action::FocusUp => {
                self.focus_adjacent(pane_grid::Direction::Up);
                Task::none()
            }
            Action::FocusDown => {
                self.focus_adjacent(pane_grid::Direction::Down);
                Task::none()
            }
            Action::FocusLeft => {
                self.focus_adjacent(pane_grid::Direction::Left);
                Task::none()
            }
            Action::FocusRight => {
                self.focus_adjacent(pane_grid::Direction::Right);
                Task::none()
            }
            Action::CommandPalette => {
                self.palette.toggle();
                Task::none()
            }
            Action::OpenSettings => {
                log::debug!("OpenSettings — not yet implemented");
                Task::none()
            }
            Action::Copy => {
                log::debug!("Copy — not yet implemented");
                Task::none()
            }
            Action::Paste => {
                iced::clipboard::read().map(Message::ClipboardContent)
            }
        }
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Tick => {
                // Tick all panes in all tabs.
                for tab in &mut self.tabs {
                    for (_pane, block) in tab.panes.iter_mut() {
                        block.tick();
                    }
                }
            }
            Message::PaneClicked(pane) => {
                self.active_tab_mut().focus = Some(pane);
            }
            Message::PaneDragged(pane_grid::DragEvent::Dropped { pane, target }) => {
                self.active_tab_mut().panes.drop(pane, target);
            }
            Message::PaneDragged(_) => {
                // Picked / Canceled — nothing to do.
            }
            Message::PaneResized(pane_grid::ResizeEvent { split, ratio }) => {
                self.active_tab_mut().panes.resize(split, ratio);
            }
            Message::SplitHorizontal => {
                let tab = self.active_tab_mut();
                if let Some(focused) = tab.focus {
                    if let Ok(block) = Block::new_terminal(24, 80) {
                        if let Some((new_pane, _split)) =
                            tab.panes.split(pane_grid::Axis::Vertical, focused, block)
                        {
                            tab.focus = Some(new_pane);
                        }
                    }
                }
            }
            Message::SplitVertical => {
                let tab = self.active_tab_mut();
                if let Some(focused) = tab.focus {
                    if let Ok(block) = Block::new_terminal(24, 80) {
                        if let Some((new_pane, _split)) =
                            tab.panes.split(pane_grid::Axis::Horizontal, focused, block)
                        {
                            tab.focus = Some(new_pane);
                        }
                    }
                }
            }
            Message::ClosePane => {
                let tab = self.active_tab_mut();
                if let Some(focused) = tab.focus {
                    if tab.panes.len() > 1 {
                        if let Some((_closed_block, sibling)) = tab.panes.close(focused) {
                            tab.focus = Some(sibling);
                        }
                    }
                }
            }
            Message::MaximizeToggle => {
                let tab = self.active_tab_mut();
                if let Some(focused) = tab.focus {
                    if tab.panes.maximized().is_some() {
                        tab.panes.restore();
                    } else {
                        tab.panes.maximize(focused);
                    }
                }
            }

            // -- Tab management --
            Message::NewTab => {
                if let Ok(new_tab) = Tab::new() {
                    self.tabs.push(new_tab);
                    self.active_tab = self.tabs.len() - 1;
                }
            }
            Message::CloseTab(index) => {
                if self.tabs.len() > 1 && index < self.tabs.len() {
                    self.tabs.remove(index);
                    // Adjust active_tab index after removal.
                    if self.active_tab >= self.tabs.len() {
                        self.active_tab = self.tabs.len() - 1;
                    } else if self.active_tab > index {
                        self.active_tab -= 1;
                    }
                }
            }
            Message::SelectTab(index) => {
                if index < self.tabs.len() {
                    self.active_tab = index;
                }
            }
            Message::TabBarAction(action) => match action {
                TabBarAction::Select(i) => return self.update(Message::SelectTab(i)),
                TabBarAction::Close(i) => return self.update(Message::CloseTab(i)),
                TabBarAction::New => return self.update(Message::NewTab),
            },
            Message::SidebarAction(action) => match action {
                SidebarAction::NewTerminal => {
                    return self.update(Message::SidebarNewTerminal);
                }
            },
            Message::SidebarNewTerminal => {
                // Split the focused pane horizontally with a new terminal.
                let tab = self.active_tab_mut();
                if let Some(focused) = tab.focus {
                    if let Ok(block) = Block::new_terminal(24, 80) {
                        if let Some((new_pane, _split)) =
                            tab.panes.split(pane_grid::Axis::Vertical, focused, block)
                        {
                            tab.focus = Some(new_pane);
                        }
                    }
                }
            }

            // Command palette messages
            Message::PaletteQueryChanged(query) => {
                self.palette.update_query(query);
            }
            Message::PaletteSubmit => {
                if let Some(action) = self.palette.execute() {
                    return self.dispatch_action(action);
                }
            }

            Message::KeyboardInput(key, modifiers) => {
                // When the palette is open, intercept navigation keys.
                if self.palette.visible {
                    match &key {
                        Key::Named(Named::Escape) => {
                            self.palette.close();
                            return Task::none();
                        }
                        Key::Named(Named::ArrowUp) => {
                            self.palette.select_prev();
                            return Task::none();
                        }
                        Key::Named(Named::ArrowDown) => {
                            self.palette.select_next();
                            return Task::none();
                        }
                        Key::Named(Named::Enter) => {
                            return self.update(Message::PaletteSubmit);
                        }
                        _ => {
                            // Let Ctrl+Shift+P toggle the palette off.
                            if let Some(Action::CommandPalette) = match_shortcut(&key, &modifiers) {
                                self.palette.close();
                                return Task::none();
                            }
                            // All other keys are handled by the text_input widget.
                            return Task::none();
                        }
                    }
                }

                // Route through the keybinding registry.
                if let Some(action) = match_shortcut(&key, &modifiers) {
                    return self.dispatch_action(action);
                }

                // Reset cursor blink on keypress.
                {
                    let tab = self.active_tab_mut();
                    if let Some(focused) = tab.focus {
                        if let Some(block) = tab.panes.get_mut(focused) {
                            block.reset_cursor_blink();
                        }
                    }
                }

                // Forward to focused terminal.
                if let Some(bytes) = key_to_bytes(&key, &modifiers) {
                    let tab = self.active_tab_mut();
                    if let Some(focused) = tab.focus {
                        if let Some(block) = tab.panes.get_mut(focused) {
                            block.write_input(&bytes);
                        }
                    }
                }
            }
            Message::ClipboardContent(content) => {
                if let Some(text) = content {
                    let tab = self.active_tab_mut();
                    if let Some(focused) = tab.focus {
                        if let Some(block) = tab.panes.get_mut(focused) {
                            let mut paste_bytes = Vec::new();
                            paste_bytes.extend_from_slice(b"\x1b[200~");
                            paste_bytes.extend_from_slice(text.as_bytes());
                            paste_bytes.extend_from_slice(b"\x1b[201~");
                            block.write_input(&paste_bytes);
                        }
                    }
                }
            }
            Message::MouseScroll(delta_y) => {
                let lines = delta_y.round() as i32;
                if lines != 0 {
                    let tab = self.active_tab_mut();
                    if let Some(focused) = tab.focus {
                        if let Some(block) = tab.panes.get_mut(focused) {
                            block.scroll(lines);
                        }
                    }
                }
            }
        }
        Task::none()
    }

    fn view(&self) -> Element<'_, Message> {
        let tab = self.active_tab();
        let focus = tab.focus;
        let total_panes = tab.panes.len();

        // Tab bar
        let titles: Vec<String> = self.tabs.iter().map(|t| t.title.clone()).collect();
        let tab_bar = tab_bar_view(&titles, self.active_tab, Message::TabBarAction);

        // Pane grid for the active tab
        let pane_grid_widget =
            pane_grid::PaneGrid::new(&tab.panes, |pane, block, _is_maximized| {
                let is_focused = focus == Some(pane);

                // Build the terminal canvas.
                let grid = block.render_grid();
                let terminal_view = TerminalView::new(grid);
                let content: Element<'_, Message> = terminal_view.view();

                // Title bar.
                let title = text(block.title()).size(12);

                let title_bar = if total_panes > 1 {
                    let close_btn: Element<'_, Message> = button(text("X").size(12))
                        .on_press(Message::ClosePane)
                        .padding(2)
                        .into();

                    pane_grid::TitleBar::new(title)
                        .controls(close_btn)
                        .padding(4)
                        .style(move |theme: &Theme| title_bar_style(theme, is_focused))
                } else {
                    pane_grid::TitleBar::new(title)
                        .padding(4)
                        .style(move |theme: &Theme| title_bar_style(theme, is_focused))
                };

                pane_grid::Content::new(content)
                    .title_bar(title_bar)
                    .style(move |theme: &Theme| pane_content_style(theme, is_focused))
            })
            .on_click(Message::PaneClicked)
            .on_drag(Message::PaneDragged)
            .on_resize(10, Message::PaneResized)
            .spacing(2)
            .width(Fill)
            .height(Fill);

        // Sidebar
        let sidebar = sidebar_view(Message::SidebarAction);

        // Layout: tab bar on top, then [pane_grid | sidebar] below
        let content_row = row![pane_grid_widget, sidebar];
        let layout = column![tab_bar, content_row];

        let base: Element<'_, Message> = container(layout)
            .width(Fill)
            .height(Fill)
            .into();

        // Command palette overlay
        if self.palette.visible {
            let overlay = self.palette_overlay();
            stack![base, opaque(overlay)].into()
        } else {
            base
        }
    }

    /// Build the command palette overlay widget.
    fn palette_overlay(&self) -> Element<'_, Message> {
        // Search input
        let input = text_input("Type a command...", &self.palette.query)
            .on_input(Message::PaletteQueryChanged)
            .on_submit(Message::PaletteSubmit)
            .size(14)
            .padding(8);

        // Command list
        let commands = self.palette.visible_commands();
        let selected = self.palette.selected;

        let mut items: Vec<Element<'_, Message>> = Vec::new();
        for (i, cmd) in commands.iter().enumerate() {
            let is_selected = i == selected;
            let bg_color = if is_selected {
                Color::from_rgb(0.20, 0.30, 0.50)
            } else {
                Color::from_rgb(0.12, 0.12, 0.15)
            };
            let text_color = if is_selected {
                Color::from_rgb(1.0, 1.0, 1.0)
            } else {
                Color::from_rgb(0.75, 0.75, 0.75)
            };

            let label = text(&cmd.label).size(13).color(text_color);
            let shortcut = text(&cmd.shortcut).size(11).color(
                if is_selected {
                    Color::from_rgb(0.7, 0.8, 1.0)
                } else {
                    Color::from_rgb(0.45, 0.45, 0.50)
                },
            );

            let item_row = row![label, iced::widget::space().width(Fill), shortcut]
                .spacing(8)
                .align_y(iced::Alignment::Center);

            let item_container: Element<'_, Message> = container(item_row)
                .width(Fill)
                .padding(6)
                .style(move |_theme: &Theme| iced::widget::container::Style {
                    background: Some(Background::Color(bg_color)),
                    ..Default::default()
                })
                .into();

            items.push(item_container);
        }

        let list = Column::from_vec(items).spacing(1);

        // Wrap the list in a scrollable-like container (limited height).
        let list_container = container(list)
            .max_height(300)
            .width(Fill);

        // The palette box
        let palette_box = column![input, list_container]
            .spacing(2)
            .width(Length::Fixed(450.0));

        let palette_styled = container(palette_box)
            .padding(4)
            .style(|_theme: &Theme| iced::widget::container::Style {
                background: Some(Background::Color(Color::from_rgb(0.10, 0.10, 0.13))),
                border: Border {
                    color: Color::from_rgb(0.30, 0.45, 0.75),
                    width: 1.0,
                    radius: 6.0.into(),
                },
                ..Default::default()
            });

        // Center horizontally, place near top
        container(
            container(palette_styled)
                .center_x(Fill)
                .padding(iced::Padding { top: 60.0, right: 0.0, bottom: 0.0, left: 0.0 }),
        )
        .width(Fill)
        .height(Fill)
        .style(|_theme: &Theme| iced::widget::container::Style {
            background: Some(Background::Color(Color::from_rgba(0.0, 0.0, 0.0, 0.5))),
            ..Default::default()
        })
        .into()
    }

    fn subscription(&self) -> Subscription<Message> {
        let tick = iced::time::every(Duration::from_millis(8)).map(|_| Message::Tick);

        let events =
            iced::event::listen_with(|event, status, _window: window::Id| {
                match &event {
                    Event::Mouse(iced::mouse::Event::WheelScrolled { delta }) => {
                        let y = match delta {
                            iced::mouse::ScrollDelta::Lines { y, .. } => *y,
                            iced::mouse::ScrollDelta::Pixels { y, .. } => *y / 19.6,
                        };
                        if y.abs() > 0.001 {
                            return Some(Message::MouseScroll(y * 3.0));
                        }
                    }
                    _ => {}
                }

                if status == Status::Captured {
                    return None;
                }
                match event {
                    Event::Keyboard(iced::keyboard::Event::KeyPressed {
                        key,
                        modifiers,
                        text: _,
                        ..
                    }) => Some(Message::KeyboardInput(key, modifiers)),
                    _ => None,
                }
            });

        Subscription::batch([tick, events])
    }
}

// ---------------------------------------------------------------------------
// Styles
// ---------------------------------------------------------------------------

fn title_bar_style(
    _theme: &Theme,
    is_focused: bool,
) -> iced::widget::container::Style {
    use iced::{Background, Border, Color};

    let bg = if is_focused {
        Color::from_rgb(0.15, 0.15, 0.20)
    } else {
        Color::from_rgb(0.10, 0.10, 0.12)
    };

    iced::widget::container::Style {
        background: Some(Background::Color(bg)),
        text_color: Some(Color::from_rgb(0.8, 0.8, 0.8)),
        border: Border {
            color: if is_focused {
                Color::from_rgb(0.3, 0.5, 0.8)
            } else {
                Color::TRANSPARENT
            },
            width: if is_focused { 1.0 } else { 0.0 },
            radius: 0.0.into(),
        },
        ..Default::default()
    }
}

fn pane_content_style(
    _theme: &Theme,
    is_focused: bool,
) -> iced::widget::container::Style {
    use iced::{Background, Border, Color};

    iced::widget::container::Style {
        background: Some(Background::Color(Color::from_rgb(0.05, 0.05, 0.05))),
        border: Border {
            color: if is_focused {
                Color::from_rgb(0.3, 0.5, 0.8)
            } else {
                Color::from_rgb(0.15, 0.15, 0.15)
            },
            width: 1.0,
            radius: 0.0.into(),
        },
        ..Default::default()
    }
}

// ---------------------------------------------------------------------------
// Key mapping
// ---------------------------------------------------------------------------

/// Convert an iced keyboard key press into the bytes that should be sent to the PTY.
fn key_to_bytes(key: &Key, modifiers: &Modifiers) -> Option<Vec<u8>> {
    match key {
        Key::Character(c) => {
            let s = c.as_str();
            // Handle Ctrl+<letter> sequences.
            if modifiers.control() {
                if let Some(ch) = s.chars().next() {
                    let lower = ch.to_ascii_lowercase();
                    if lower >= 'a' && lower <= 'z' {
                        // Ctrl+A = 0x01, ..., Ctrl+Z = 0x1A
                        let ctrl_byte = (lower as u8) - b'a' + 1;
                        return Some(vec![ctrl_byte]);
                    }
                }
            }
            Some(s.as_bytes().to_vec())
        }
        Key::Named(named) => named_key_to_bytes(named, modifiers),
        Key::Unidentified => None,
    }
}

/// Convert a named key to the corresponding byte sequence for the PTY.
fn named_key_to_bytes(named: &Named, _modifiers: &Modifiers) -> Option<Vec<u8>> {
    match named {
        Named::Enter => Some(b"\r".to_vec()),
        Named::Backspace => Some(vec![0x7f]),
        Named::Tab => Some(b"\t".to_vec()),
        Named::Escape => Some(vec![0x1b]),
        Named::Space => Some(b" ".to_vec()),

        // Arrow keys -- standard ANSI escape sequences.
        Named::ArrowUp => Some(b"\x1b[A".to_vec()),
        Named::ArrowDown => Some(b"\x1b[B".to_vec()),
        Named::ArrowRight => Some(b"\x1b[C".to_vec()),
        Named::ArrowLeft => Some(b"\x1b[D".to_vec()),

        // Navigation keys.
        Named::Home => Some(b"\x1b[H".to_vec()),
        Named::End => Some(b"\x1b[F".to_vec()),
        Named::PageUp => Some(b"\x1b[5~".to_vec()),
        Named::PageDown => Some(b"\x1b[6~".to_vec()),
        Named::Delete => Some(b"\x1b[3~".to_vec()),
        Named::Insert => Some(b"\x1b[2~".to_vec()),

        // Modifier keys themselves should not produce output.
        Named::Shift | Named::Control | Named::Alt | Named::Super | Named::Meta => None,

        _ => None,
    }
}
