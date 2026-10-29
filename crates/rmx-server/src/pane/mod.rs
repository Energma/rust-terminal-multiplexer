//! A pane: one child process in a PTY plus its terminal emulator.
//!
//! portable-pty's I/O is blocking, so each pane runs three small threads
//! (reader, writer, waiter) that talk to the core over channels. The core
//! is the only owner of pane state.

mod platform;
mod running;
mod spawn;

use std::path::PathBuf;
use std::sync::mpsc as std_mpsc;

use portable_pty::{ChildKiller, MasterPty};
use rmx_core::emulator::Emulator;

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
