use std::time::Duration;

use iced::event::Status;
use iced::keyboard::key::Named;
use iced::keyboard::{Key, Modifiers};
use iced::window;
use iced::{Element, Event, Subscription, Task, Theme};
use tokio::sync::mpsc;

use gpu_renderer::colors::AnsiPalette;
use gpu_renderer::grid::RenderGrid;
use gpu_renderer::widget::TerminalView;
use gpu_renderer::RendererMessage;
use terminal::{AlacrittyEvent, PtyHandle, TerminalEvent, TerminalState};

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
    terminal: TerminalState,
    pty: Option<PtyHandle>,
    pty_rx: Option<mpsc::Receiver<TerminalEvent>>,
    palette: AnsiPalette,
    cursor_visible: bool,
    blink_count: u32,
}

#[derive(Debug, Clone)]
enum Message {
    Tick,
    KeyboardInput(Key, Modifiers),
    WindowResized(iced::Size),
    MouseScroll(f32),
    ClipboardContent(Option<String>),
    Renderer(RendererMessage),
}

impl Altermative {
    fn new() -> (Self, Task<Message>) {
        let terminal = TerminalState::new(24, 80);
        let palette = AnsiPalette::default();

        let (pty, pty_rx) = match PtyHandle::spawn(24, 80) {
            Ok((handle, rx)) => (Some(handle), Some(rx)),
            Err(e) => {
                log::error!("Failed to spawn PTY: {e}");
                (None, None)
            }
        };

        let app = Altermative {
            terminal,
            pty,
            pty_rx,
            palette,
            cursor_visible: true,
            blink_count: 0,
        };

        (app, Task::none())
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Tick => {
                // Drain all available PTY output.
                if let Some(rx) = &mut self.pty_rx {
                    loop {
                        match rx.try_recv() {
                            Ok(TerminalEvent::PtyOutput(data)) => {
                                self.terminal.process_output(&data);
                            }
                            Ok(TerminalEvent::PtyExited(code)) => {
                                log::info!("PTY exited with code {code}");
                                self.pty_rx = None;
                                break;
                            }
                            Ok(TerminalEvent::PtyError(e)) => {
                                log::error!("PTY error: {e}");
                                self.pty_rx = None;
                                break;
                            }
                            Err(mpsc::error::TryRecvError::Empty) => break,
                            Err(mpsc::error::TryRecvError::Disconnected) => {
                                log::info!("PTY channel disconnected");
                                self.pty_rx = None;
                                break;
                            }
                        }
                    }
                }

                // Drain terminal events (title changes, bell, etc.).
                for event in self.terminal.drain_events() {
                    match event {
                        AlacrittyEvent::Title(title) => {
                            log::debug!("Terminal title changed: {title}");
                        }
                        AlacrittyEvent::Bell => {
                            log::debug!("Terminal bell");
                        }
                        _ => {}
                    }
                }

                // Cursor blink: toggle every ~500ms (62 ticks at 8ms each).
                self.blink_count += 1;
                if self.blink_count >= 62 {
                    self.blink_count = 0;
                    self.cursor_visible = !self.cursor_visible;
                }
            }
            Message::KeyboardInput(key, modifiers) => {
                // Reset cursor blink on keypress so cursor stays visible while typing.
                self.cursor_visible = true;
                self.blink_count = 0;

                // Ctrl+Shift+C: copy (TODO: selection text extraction)
                // Ctrl+Shift+V: paste from clipboard
                if modifiers.control() && modifiers.shift() {
                    if let Key::Character(ref c) = key {
                        let ch = c.as_str().to_ascii_lowercase();
                        if ch == "v" {
                            // Read clipboard and send contents to PTY.
                            return iced::clipboard::read().map(Message::ClipboardContent);
                        }
                        if ch == "c" {
                            // TODO: Extract selected text from terminal selection.
                            // The alacritty_terminal Selection API requires significant
                            // wiring (mouse tracking, selection state management).
                            // For now, log a note; copy will be implemented in a
                            // future iteration when mouse selection is added.
                            log::debug!("Ctrl+Shift+C pressed — copy not yet implemented (no selection)");
                            return Task::none();
                        }
                    }
                }
                if let Some(bytes) = key_to_bytes(&key, &modifiers) {
                    if let Some(pty) = &mut self.pty {
                        if let Err(e) = pty.write(&bytes) {
                            log::error!("Failed to write to PTY: {e}");
                        }
                    }
                }
            }
            Message::ClipboardContent(content) => {
                if let Some(text) = content {
                    if let Some(pty) = &mut self.pty {
                        // Use bracketed paste mode: wrap pasted text in
                        // ESC[200~ ... ESC[201~ so the shell can distinguish
                        // pasted text from typed input.
                        let mut paste_bytes = Vec::new();
                        paste_bytes.extend_from_slice(b"\x1b[200~");
                        paste_bytes.extend_from_slice(text.as_bytes());
                        paste_bytes.extend_from_slice(b"\x1b[201~");
                        if let Err(e) = pty.write(&paste_bytes) {
                            log::error!("Failed to paste to PTY: {e}");
                        }
                    }
                }
            }
            Message::MouseScroll(delta_y) => {
                // Positive delta_y = scroll up (toward history), negative = scroll down.
                // The alacritty Scroll::Delta convention: positive = scroll up (toward history).
                let lines = delta_y.round() as i32;
                if lines != 0 {
                    self.terminal.scroll(lines);
                }
            }
            Message::WindowResized(size) => {
                let cell_width: f32 = 14.0 * 0.6;   // 8.4
                let cell_height: f32 = 14.0 * 1.4;   // 19.6
                let new_cols = (size.width / cell_width).floor().max(1.0) as usize;
                let new_rows = (size.height / cell_height).floor().max(1.0) as usize;

                if new_rows != self.terminal.rows() || new_cols != self.terminal.cols() {
                    self.terminal.resize(new_rows, new_cols);
                    if let Some(pty) = &self.pty {
                        if let Err(e) = pty.resize(new_rows as u16, new_cols as u16) {
                            log::error!("PTY resize failed: {e}");
                        }
                    }
                    log::debug!("Resized terminal to {new_rows}x{new_cols}");
                }
            }
            Message::Renderer(_msg) => {
                // RendererMessage is currently empty; future mouse events will go here.
            }
        }
        Task::none()
    }

    fn view(&self) -> Element<'_, Message> {
        let grid = RenderGrid::from_terminal_with_cursor(
            &self.terminal,
            &self.palette,
            self.cursor_visible,
        );
        let terminal_view = TerminalView::new(grid);
        let element: Element<'static, RendererMessage> = terminal_view.view();
        element.map(Message::Renderer)
    }

    fn subscription(&self) -> Subscription<Message> {
        let tick = iced::time::every(Duration::from_millis(8)).map(|_| Message::Tick);

        let events = iced::event::listen_with(|event, status, _window: window::Id| {
            match &event {
                Event::Window(window::Event::Resized(size)) => {
                    return Some(Message::WindowResized(*size));
                }
                Event::Mouse(iced::mouse::Event::WheelScrolled { delta }) => {
                    let y = match delta {
                        iced::mouse::ScrollDelta::Lines { y, .. } => *y,
                        iced::mouse::ScrollDelta::Pixels { y, .. } => *y / 19.6, // convert pixels to lines
                    };
                    if y.abs() > 0.001 {
                        // Positive y from iced = scroll up = toward history.
                        // alacritty Scroll::Delta: positive = scroll up toward history.
                        return Some(Message::MouseScroll(y * 3.0)); // 3 lines per scroll notch
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
