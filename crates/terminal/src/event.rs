/// Events emitted by the PTY reader thread and surfaced to async consumers.
#[derive(Debug, Clone)]
pub enum TerminalEvent {
    /// Raw bytes read from the PTY master.
    PtyOutput(Vec<u8>),
    /// Child process exited with the given exit code.
    PtyExited(i32),
    /// An I/O or system error occurred on the PTY.
    PtyError(String),
}
