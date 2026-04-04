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
use terminal::{PtyHandle, TerminalEvent, TerminalState};

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
}

#[derive(Debug, Clone)]
enum Message {
    Tick,
    KeyboardInput(Key, Modifiers),
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
        };

        (app, Task::none())
    }

    fn update(&mut self, message: Message) {
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
            }
            Message::KeyboardInput(key, modifiers) => {
                if let Some(bytes) = key_to_bytes(&key, &modifiers) {
                    if let Some(pty) = &mut self.pty {
                        if let Err(e) = pty.write(&bytes) {
                            log::error!("Failed to write to PTY: {e}");
                        }
                    }
                }
            }
            Message::Renderer(_msg) => {
                // RendererMessage is currently empty; future mouse events will go here.
            }
        }
    }

    fn view(&self) -> Element<'_, Message> {
        let grid = RenderGrid::from_terminal(&self.terminal, &self.palette);
        let terminal_view = TerminalView::new(grid);
        let element: Element<'static, RendererMessage> = terminal_view.view();
        element.map(Message::Renderer)
    }

    fn subscription(&self) -> Subscription<Message> {
        let tick = iced::time::every(Duration::from_millis(8)).map(|_| Message::Tick);

        let keyboard = iced::event::listen_with(|event, status, _window: window::Id| {
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

        Subscription::batch([tick, keyboard])
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
