// Terminal emulation backend crate.
// Provides PTY management and alacritty_terminal integration.

pub mod event;
pub mod pty;
pub mod term;

pub use event::TerminalEvent;
pub use pty::PtyHandle;
pub use term::TerminalState;
