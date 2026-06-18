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
