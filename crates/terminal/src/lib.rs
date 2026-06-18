// Terminal emulation backend crate.
// Provides PTY management and alacritty_terminal integration.

pub mod event;
pub mod pty;
pub mod term;

pub use event::TerminalEvent;
pub use pty::{read_proc_cwd, PtyHandle};
pub use term::TerminalState;

/// Re-export alacritty_terminal events so consumers don't need a direct dependency.
pub use alacritty_terminal::event::Event as AlacrittyEvent;
