//! A pane: one child process in a PTY plus its terminal emulator.
//!
//! portable-pty's I/O is blocking, so each pane runs three small threads
//! (reader, writer, waiter) that talk to the core over channels. The core
//! is the only owner of pane state.

use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::mpsc as std_mpsc;
use std::thread;

use anyhow::{Context, Result};
use portable_pty::{ChildKiller, CommandBuilder, MasterPty, PtySize, native_pty_system};
use rmx_core::emulator::{Emulator, Vt100Emulator};
use tokio::sync::mpsc;

pub type PaneId = u32;

#[derive(Debug)]
pub enum PaneEvent {
    Output(PaneId, Vec<u8>),
    /// The PTY reached EOF: the child and everything holding the PTY is gone.
    Closed(PaneId),
}

/// What to run in a new pane.
#[derive(Debug, Clone, Default)]
pub struct SpawnSpec {
    /// Shell override; `None` picks the platform default.
    pub shell: Option<String>,
    /// A command to run instead of an interactive shell; the pane closes
    /// when it exits.
    pub command: Option<String>,
    pub cwd: Option<PathBuf>,
    /// Extra environment for the program.
    pub env: Vec<(String, String)>,
}

pub struct Pane {
    pub id: PaneId,
    pub emulator: Box<dyn Emulator>,
    /// The explicit command this pane runs, if any (kept for session restore).
    pub command: Option<String>,
    /// Short program name shown until the program sets a title.
    pub program: String,
    size: (u16, u16),
    spawn_cwd: PathBuf,
    /// OS process id of the program, when the platform reports one.
    pub pid: Option<u32>,
    // `Option` so Drop can move them out in a deliberate order.
    master: Option<Box<dyn MasterPty + Send>>,
    writer_tx: Option<std_mpsc::Sender<Vec<u8>>>,
    killer: Box<dyn ChildKiller + Send + Sync>,
}

impl Pane {
    /// Spawns a pane sized `rows`×`cols`.
    pub fn spawn(
        id: PaneId,
        spec: &SpawnSpec,
        rows: u16,
        cols: u16,
        scrollback: usize,
        events: mpsc::Sender<PaneEvent>,
    ) -> Result<Self> {
        // portable-pty panics (rather than erroring) when ConPTY is missing on
        // Windows older than 10 1809; turn that into a message users can act on.
        let opened = std::panic::catch_unwind(|| native_pty_system().openpty(pty_size(rows, cols)));
        let pair = match opened {
            Ok(result) => result.context("opening a pseudo-terminal")?,
            Err(_) => anyhow::bail!(
                "this system has no pseudo-console support (Windows 10 1809 or newer is required)"
            ),
        };

        let shell = spec.shell.clone().or_else(default_shell);
        let program = program_name(spec.command.as_deref().or(shell.as_deref()));
        let mut cmd = match (&spec.command, shell) {
            (Some(command), _) => {
                let mut argv = command_argv(command).into_iter();
                let mut cmd = CommandBuilder::new(argv.next().unwrap_or_default());
                cmd.args(argv);
                cmd
            }
            (None, Some(program)) => CommandBuilder::new(program),
            (None, None) => CommandBuilder::new_default_prog(),
        };
        let cwd = spec
            .cwd
            .clone()
            .filter(|d| d.is_dir())
            .or_else(|| std::env::current_dir().ok())
            .or_else(dirs_home)
            .unwrap_or_else(|| PathBuf::from("."));
        cmd.cwd(&cwd);
        cmd.env("TERM", "xterm-256color");
        cmd.env("COLORTERM", "truecolor");
        cmd.env("RMX", "1");
        for (k, v) in &spec.env {
            cmd.env(k, v);
        }
        cmd.env_remove("TMUX");

        let mut child = pair
            .slave
            .spawn_command(cmd)
            .context("starting the shell")?;
        let pid = child.process_id();
        // The child holds its own handle; keeping ours open would stop the
        // reader from ever seeing EOF on Unix.
        drop(pair.slave);

        let killer = child.clone_killer();
        let mut reader = pair.master.try_clone_reader()?;
        let mut writer = pair.master.take_writer()?;

        let out_tx = events.clone();
        thread::Builder::new()
            .name(format!("pane-{id}-reader"))
            .spawn(move || {
                let mut buf = vec![0u8; 64 * 1024];
                loop {
                    match reader.read(&mut buf) {
                        Ok(0) | Err(_) => break,
                        // blocking_send applies back-pressure to the child
                        // when the core falls behind.
                        Ok(n) => {
                            if out_tx
                                .blocking_send(PaneEvent::Output(id, buf[..n].to_vec()))
                                .is_err()
                            {
                                break;
                            }
                        }
                    }
                }
                let _ = out_tx.blocking_send(PaneEvent::Closed(id));
            })?;

        let (writer_tx, writer_rx) = std_mpsc::channel::<Vec<u8>>();
        thread::Builder::new()
            .name(format!("pane-{id}-writer"))
            .spawn(move || {
                for bytes in writer_rx {
                    if writer
                        .write_all(&bytes)
                        .and_then(|()| writer.flush())
                        .is_err()
                    {
                        break;
                    }
                }
            })?;

        // Reaps the child so it doesn't linger as a zombie on Unix. The exit
        // itself is reported via the reader's EOF.
        thread::Builder::new()
            .name(format!("pane-{id}-waiter"))
            .spawn(move || {
                let status = child.wait();
                tracing::debug!(pane = id, ?status, "child exited");
            })?;

        Ok(Self {
            id,
            emulator: Box::new(Vt100Emulator::new(rows, cols, scrollback)),
            command: spec.command.clone(),
            program,
            size: (rows, cols),
            spawn_cwd: cwd,
            pid,
            master: Some(pair.master),
            writer_tx: Some(writer_tx),
            killer,
        })
    }

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

fn pty_size(rows: u16, cols: u16) -> PtySize {
    PtySize {
        rows: rows.max(1),
        cols: cols.max(1),
        pixel_width: 0,
        pixel_height: 0,
    }
}

/// On Windows prefer PowerShell 7, then Windows PowerShell, over portable-pty's
/// `%ComSpec%` (cmd.exe) default. Elsewhere portable-pty's `$SHELL`/passwd
/// lookup is already right.
fn default_shell() -> Option<String> {
    if cfg!(windows) {
        ["pwsh.exe", "powershell.exe"]
            .into_iter()
            .find_map(find_on_path)
    } else {
        None
    }
}

/// "pwsh", "vim", "bash" … from a command line or program path.
fn program_name(command: Option<&str>) -> String {
    let fallback = std::env::var("SHELL").unwrap_or_else(|_| "shell".into());
    let line = command.unwrap_or(&fallback);
    let first = line.split_whitespace().next().unwrap_or("shell");
    let base = first.rsplit(['/', '\\']).next().unwrap_or(first);
    base.strip_suffix(".exe").unwrap_or(base).to_owned()
}

/// Runs `command` through the platform shell so quoting and PATH lookup
/// behave as users expect from typing it.
fn command_argv(command: &str) -> Vec<String> {
    if cfg!(windows) {
        vec!["cmd.exe".into(), "/C".into(), command.into()]
    } else {
        vec!["/bin/sh".into(), "-c".into(), command.into()]
    }
}

fn dirs_home() -> Option<PathBuf> {
    std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).map(PathBuf::from)
}

fn find_on_path(program: &str) -> Option<String> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join(program))
        .find(|candidate| candidate.is_file())
        .map(|p| p.to_string_lossy().into_owned())
}
