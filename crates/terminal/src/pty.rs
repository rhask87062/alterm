use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use tokio::sync::mpsc;

use crate::event::TerminalEvent;

/// Read a process's current working directory via `/proc/<pid>/cwd` (Linux only).
pub fn read_proc_cwd(pid: u32) -> Option<PathBuf> {
    #[cfg(target_os = "linux")]
    {
        std::fs::read_link(format!("/proc/{pid}/cwd")).ok()
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = pid;
        None
    }
}

/// Channel buffer size for PTY events.
const EVENT_CHANNEL_SIZE: usize = 1024;

/// A handle to a spawned PTY and its child process.
///
/// The PTY reader runs on a dedicated `std::thread` since PTY I/O is blocking.
/// Events are forwarded through a `tokio::sync::mpsc` channel so that async
/// consumers (e.g. an iced subscription) can await them without blocking.
pub struct PtyHandle {
    /// Write half of the PTY master — wrapped in a Mutex so `write` takes `&mut self`
    /// without needing exclusive access to the whole struct from multiple places.
    master_writer: Arc<Mutex<Box<dyn Write + Send>>>,
    /// The PTY master itself, kept alive to prevent the OS from closing the fd.
    _master: Box<dyn portable_pty::MasterPty + Send>,
    /// The spawned child process.
    child: Box<dyn portable_pty::Child + Send>,
    /// The PTY pair size, tracked locally for resize operations.
    size: Arc<Mutex<PtySize>>,
}

impl PtyHandle {
    /// Open a PTY, spawn the user's `$SHELL` (falling back to `/bin/bash`),
    /// and start a reader thread.
    ///
    /// Returns the handle and the receiving end of the event channel.
    pub fn spawn(rows: u16, cols: u16) -> Result<(Self, mpsc::Receiver<TerminalEvent>), String> {
        Self::spawn_in(rows, cols, None)
    }

    /// Like `spawn`, but optionally start the shell in `cwd` (if given and valid).
    pub fn spawn_in(rows: u16, cols: u16, cwd: Option<&Path>)
        -> Result<(Self, mpsc::Receiver<TerminalEvent>), String>
    {
        let pty_system = native_pty_system();

        let size = PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        };

        let pair = pty_system
            .openpty(size)
            .map_err(|e| format!("openpty failed: {e}"))?;

        let shell = default_shell();
        let mut cmd = CommandBuilder::new(&shell);
        cmd.env("TERM", std::env::var("TERM").unwrap_or_else(|_| "xterm-256color".to_string()));
        cmd.env("SHELL", &shell);
        if let Some(dir) = cwd {
            if dir.is_dir() {
                cmd.cwd(dir);
            }
        }

        let child = pair
            .slave
            .spawn_command(cmd)
            .map_err(|e| format!("spawn_command failed: {e}"))?;

        // The slave end is no longer needed in this process once the child is spawned.
        drop(pair.slave);

        let master = pair.master;

        // Clone the master for the reader thread.
        let mut reader = master
            .try_clone_reader()
            .map_err(|e| format!("try_clone_reader failed: {e}"))?;

        let writer = master
            .take_writer()
            .map_err(|e| format!("take_writer failed: {e}"))?;

        let (tx, rx) = mpsc::channel::<TerminalEvent>(EVENT_CHANNEL_SIZE);

        // Greet every new terminal with the bundled Alterm ASCII logo. It is
        // enqueued before the reader thread starts, so it is rendered ahead of
        // any shell output and the prompt appears just beneath it. `try_send`
        // is safe here: the channel was just created and is empty.
        let _ = tx.try_send(TerminalEvent::PtyOutput(startup_banner()));

        // Spawn the blocking reader thread.
        std::thread::spawn(move || {
            let mut buf = vec![0u8; 4096];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) => {
                        // EOF — PTY closed; no exit code available here.
                        let _ = tx.blocking_send(TerminalEvent::PtyExited(0));
                        break;
                    }
                    Ok(n) => {
                        let _ = tx.blocking_send(TerminalEvent::PtyOutput(buf[..n].to_vec()));
                    }
                    Err(e) => {
                        // On Linux, EIO is raised when the slave side closes (normal exit).
                        let msg = e.to_string();
                        // EIO (errno 5) is raised on Linux when the slave PTY
                        // side closes — this is the normal "process exited" signal.
                        let is_eof = e.raw_os_error() == Some(5)
                            || msg.contains("EIO")
                            || msg.contains("Input/output error");
                        if is_eof {
                            let _ = tx.blocking_send(TerminalEvent::PtyExited(0));
                        } else {
                            let _ = tx.blocking_send(TerminalEvent::PtyError(msg));
                        }
                        break;
                    }
                }
            }
        });

        let shared_size = Arc::new(Mutex::new(size));

        let handle = PtyHandle {
            master_writer: Arc::new(Mutex::new(writer)),
            _master: master,
            child,
            size: shared_size,
        };

        Ok((handle, rx))
    }

    /// Write bytes to the PTY master (i.e. send input to the shell).
    pub fn write(&mut self, data: &[u8]) -> Result<(), String> {
        let mut w = self.master_writer.lock().unwrap();
        w.write_all(data).map_err(|e| format!("PTY write failed: {e}"))
    }

    /// Resize the PTY.
    pub fn resize(&self, rows: u16, cols: u16) -> Result<(), String> {
        let new_size = PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        };
        self._master
            .resize(new_size)
            .map_err(|e| format!("PTY resize failed: {e}"))?;
        *self.size.lock().unwrap() = new_size;
        Ok(())
    }

    /// Returns `true` if the child process is still running.
    pub fn is_alive(&mut self) -> bool {
        match self.child.try_wait() {
            Ok(None) => true,        // still running
            Ok(Some(_)) => false,    // exited
            Err(_) => false,         // error querying — treat as dead
        }
    }

    /// PID of the child shell, if available.
    pub fn child_pid(&self) -> Option<u32> {
        self.child.process_id()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(target_os = "linux")]
    #[test]
    fn read_proc_cwd_of_self_matches_current_dir() {
        let pid = std::process::id();
        let cwd = read_proc_cwd(pid).expect("own cwd readable");
        let expected = std::env::current_dir().unwrap().canonicalize().unwrap();
        assert_eq!(cwd.canonicalize().unwrap(), expected);
    }
}

/// The bundled Alterm ASCII logo, shown at the top of every new terminal.
/// Sourced from `assets/ascii_logo.txt` at the repository root.
const ASCII_LOGO: &str = include_str!("../../../assets/ascii_logo.txt");

/// Left-to-right color stops for the logo gradient, all from the Alterm
/// palette: a dark theme purple, through orchid, to a light near-white.
const LOGO_GRADIENT: [(u8, u8, u8); 3] = [
    (0x56, 0x00, 0x8d), // --purple-core (dark)
    (0xd4, 0x50, 0xfc), // --orchid (mid)
    (0xfa, 0xf3, 0xff), // near-white (light)
];

/// Interpolate [`LOGO_GRADIENT`] at position `t` in `0.0..=1.0`.
fn logo_gradient(t: f32) -> (u8, u8, u8) {
    let t = t.clamp(0.0, 1.0);
    let segments = (LOGO_GRADIENT.len() - 1) as f32;
    let scaled = t * segments;
    let i = (scaled.floor() as usize).min(LOGO_GRADIENT.len() - 2);
    let f = scaled - i as f32;
    let (r0, g0, b0) = LOGO_GRADIENT[i];
    let (r1, g1, b1) = LOGO_GRADIENT[i + 1];
    let lerp = |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * f).round() as u8;
    (lerp(r0, r1), lerp(g0, g1), lerp(b0, b1))
}

/// Build the startup banner bytes for a fresh terminal: the ASCII logo painted
/// with a left→right dark-to-light gradient (24-bit ANSI color — one shade per
/// column), CRLF line endings so each line returns to column 0 on a raw PTY,
/// and a trailing blank line before the shell prompt.
fn startup_banner() -> Vec<u8> {
    let logo = ASCII_LOGO.replace("\r\n", "\n");
    let lines: Vec<&str> = logo.lines().collect();
    let max_cols = lines
        .iter()
        .map(|l| l.chars().count())
        .max()
        .unwrap_or(1)
        .max(1);

    let mut out = String::new();
    for line in &lines {
        let mut last: Option<(u8, u8, u8)> = None;
        for (col, ch) in line.chars().enumerate() {
            let t = if max_cols <= 1 {
                0.0
            } else {
                col as f32 / (max_cols - 1) as f32
            };
            let rgb = logo_gradient(t);
            if last != Some(rgb) {
                let (r, g, b) = rgb;
                out.push_str(&format!("\x1b[38;2;{r};{g};{b}m"));
                last = Some(rgb);
            }
            out.push(ch);
        }
        // Reset the color and return to column 0 before the next line.
        out.push_str("\x1b[0m\r\n");
    }
    out.push_str("\r\n");
    out.into_bytes()
}

fn default_shell() -> String {
    std::env::var("SHELL")
        .ok()
        .filter(|shell| !shell.trim().is_empty())
        .unwrap_or_else(|| {
            #[cfg(target_os = "macos")]
            {
                "/bin/zsh".to_string()
            }

            #[cfg(not(target_os = "macos"))]
            {
                "/bin/bash".to_string()
            }
        })
}
