//! A running pane's life: input, output, resize, cwd, and orderly shutdown.

use std::path::PathBuf;
use std::thread;

use super::Pane;
use super::platform::pty_size;

impl Pane {
    pub fn write(&self, bytes: Vec<u8>) {
        if let Some(tx) = &self.writer_tx {
            let _ = tx.send(bytes);
        }
    }

    /// Feeds PTY output to the emulator and sends back any query replies.
    pub fn process_output(&mut self, bytes: &[u8]) {
        self.emulator.process(bytes);
        let replies = self.emulator.take_replies();
        if !replies.is_empty() {
            self.write(replies);
        }
    }

    /// Best guess at the pane's current directory: what the shell reported
    /// (OSC 7), else the process's cwd where the OS exposes it, else where it
    /// started. Used so splits and restored sessions open in the right place.
    pub fn cwd(&self) -> PathBuf {
        if let Some(dir) = self.emulator.cwd() {
            return PathBuf::from(dir);
        }
        #[cfg(target_os = "linux")]
        if let Some(dir) = self
            .pid
            .and_then(|pid| std::fs::read_link(format!("/proc/{pid}/cwd")).ok())
        {
            return dir;
        }
        self.spawn_cwd.clone()
    }

    pub fn resize(&mut self, rows: u16, cols: u16) {
        // Every resize makes the program redraw (and ConPTY repaint), so
        // skip the ones that change nothing.
        if self.size == (rows, cols) {
            return;
        }
        self.size = (rows, cols);
        self.emulator.resize(rows, cols);
        if let Some(master) = &self.master
            && let Err(err) = master.resize(pty_size(rows, cols))
        {
            tracing::warn!(pane = self.id, %err, "resizing pty failed");
        }
    }
}

impl Drop for Pane {
    fn drop(&mut self) {
        let _ = self.killer.kill();
        // Ending the writer thread drops the PTY writer before the master.
        self.writer_tx.take();
        // On Windows, dropping the master calls ClosePseudoConsole, which can
        // block until the output pipe is drained. The reader thread keeps
        // draining into the core, so drop off-thread to never stall the core.
        if let Some(master) = self.master.take() {
            let _ = thread::Builder::new()
                .name(format!("pane-{}-close", self.id))
                .spawn(move || drop(master));
        }
    }
}
