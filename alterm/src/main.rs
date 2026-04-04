use std::time::Duration;

use iced::event::Status;
use iced::keyboard::key::Named;
use iced::keyboard::{Key, Modifiers};
use iced::widget::{button, container, pane_grid, text};
use iced::window;
use iced::{Element, Event, Fill, Subscription, Task, Theme};

use gpu_renderer::widget::TerminalView;
use workspace::Block;

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
    panes: pane_grid::State<Block>,
    focus: Option<pane_grid::Pane>,
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
}

impl Altermative {
    fn new() -> (Self, Task<Message>) {
        let first_block = Block::new_terminal(24, 80)
            .expect("Failed to spawn initial terminal");

        let (panes, first_pane) = pane_grid::State::new(first_block);

        let app = Altermative {
            panes,
            focus: Some(first_pane),
        };

        (app, Task::none())
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Tick => {
                // Tick all panes.
                for (_pane, block) in self.panes.iter_mut() {
                    block.tick();
                }
            }
            Message::PaneClicked(pane) => {
                self.focus = Some(pane);
            }
            Message::PaneDragged(pane_grid::DragEvent::Dropped { pane, target }) => {
                self.panes.drop(pane, target);
            }
            Message::PaneDragged(_) => {
                // Picked / Canceled — nothing to do.
            }
            Message::PaneResized(pane_grid::ResizeEvent { split, ratio }) => {
                self.panes.resize(split, ratio);
            }
            Message::SplitHorizontal => {
                if let Some(focused) = self.focus {
                    if let Ok(block) = Block::new_terminal(24, 80) {
                        if let Some((new_pane, _split)) =
                            self.panes.split(pane_grid::Axis::Vertical, focused, block)
                        {
                            self.focus = Some(new_pane);
                        }
                    }
                }
            }
            Message::SplitVertical => {
                if let Some(focused) = self.focus {
                    if let Ok(block) = Block::new_terminal(24, 80) {
                        if let Some((new_pane, _split)) =
                            self.panes.split(pane_grid::Axis::Horizontal, focused, block)
                        {
                            self.focus = Some(new_pane);
                        }
                    }
                }
            }
            Message::ClosePane => {
                if let Some(focused) = self.focus {
                    if self.panes.len() > 1 {
                        if let Some((_closed_block, sibling)) = self.panes.close(focused) {
                            self.focus = Some(sibling);
                        }
                    }
                }
            }
            Message::MaximizeToggle => {
                if let Some(focused) = self.focus {
                    if self.panes.maximized().is_some() {
                        self.panes.restore();
                    } else {
                        self.panes.maximize(focused);
                    }
                }
            }
            Message::KeyboardInput(key, modifiers) => {
                // Pane management shortcuts: Ctrl+Shift+...
                if modifiers.control() && modifiers.shift() {
                    if let Key::Character(ref c) = key {
                        let ch = c.as_str().to_ascii_lowercase();
                        match ch.as_str() {
                            "d" => return self.update(Message::SplitHorizontal),
                            "e" => return self.update(Message::SplitVertical),
                            "x" => return self.update(Message::ClosePane),
                            "z" => return self.update(Message::MaximizeToggle),
                            "v" => {
                                return iced::clipboard::read()
                                    .map(Message::ClipboardContent);
                            }
                            "c" => {
                                log::debug!("Ctrl+Shift+C — copy not yet implemented");
                                return Task::none();
                            }
                            _ => {}
                        }
                    }

                    // Ctrl+Shift+Arrow — navigate to adjacent pane.
                    if let Key::Named(ref named) = key {
                        let direction = match named {
                            Named::ArrowLeft => Some(pane_grid::Direction::Left),
                            Named::ArrowRight => Some(pane_grid::Direction::Right),
                            Named::ArrowUp => Some(pane_grid::Direction::Up),
                            Named::ArrowDown => Some(pane_grid::Direction::Down),
                            _ => None,
                        };
                        if let Some(dir) = direction {
                            if let Some(focused) = self.focus {
                                if let Some(adjacent) = self.panes.adjacent(focused, dir) {
                                    self.focus = Some(adjacent);
                                }
                            }
                            return Task::none();
                        }
                    }
                }

                // Reset cursor blink on keypress.
                if let Some(focused) = self.focus {
                    if let Some(block) = self.panes.get_mut(focused) {
                        block.reset_cursor_blink();
                    }
                }

                // Forward to focused terminal.
                if let Some(bytes) = key_to_bytes(&key, &modifiers) {
                    if let Some(focused) = self.focus {
                        if let Some(block) = self.panes.get_mut(focused) {
                            block.write_input(&bytes);
                        }
                    }
                }
            }
            Message::ClipboardContent(content) => {
                if let Some(text) = content {
                    if let Some(focused) = self.focus {
                        if let Some(block) = self.panes.get_mut(focused) {
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
                // TODO: route scroll to the pane under the mouse cursor.
                // For now, scroll the focused pane.
                let lines = delta_y.round() as i32;
                if lines != 0 {
                    if let Some(focused) = self.focus {
                        if let Some(block) = self.panes.get_mut(focused) {
                            block.scroll(lines);
                        }
                    }
                }
            }
        }
        Task::none()
    }

    fn view(&self) -> Element<'_, Message> {
        let focus = self.focus;
        let total_panes = self.panes.len();

        let pane_grid_widget = pane_grid::PaneGrid::new(&self.panes, |pane, block, _is_maximized| {
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
                    .style(move |theme: &Theme| {
                        title_bar_style(theme, is_focused)
                    })
            } else {
                pane_grid::TitleBar::new(title)
                    .padding(4)
                    .style(move |theme: &Theme| {
                        title_bar_style(theme, is_focused)
                    })
            };

            pane_grid::Content::new(content)
                .title_bar(title_bar)
                .style(move |theme: &Theme| {
                    pane_content_style(theme, is_focused)
                })
        })
        .on_click(Message::PaneClicked)
        .on_drag(Message::PaneDragged)
        .on_resize(10, Message::PaneResized)
        .spacing(2)
        .width(Fill)
        .height(Fill);

        container(pane_grid_widget)
            .width(Fill)
            .height(Fill)
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
