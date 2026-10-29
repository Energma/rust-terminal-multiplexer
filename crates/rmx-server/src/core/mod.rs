//! The core state machine. See the crate docs for the concurrency model.
//!
//! [`Core`] is defined here; its behaviour is split by concern across the
//! submodules, each adding its own `impl Core` block.

mod actions;
mod client;
mod commands;
mod config_reload;
mod connections;
mod control;
mod event_loop;
mod handshake;
mod helpers;
mod input;
mod mouse;
mod panes;
mod persistence;
mod selection;
mod sizing;
#[cfg(test)]
mod tests;
mod tick;
mod workspaces;

use std::collections::HashMap;
use std::time::Instant;

use rmx_config::Config;
use rmx_core::layout::PaneId;
use rmx_proto::ServerMsg;
use tokio::sync::mpsc;

use crate::ServerOptions;
use crate::pane::{Pane, PaneEvent};
use crate::state::{Workspace, WorkspaceId};
use client::Client;
use event_loop::Event;

const DEFAULT_SIZE: (u16, u16) = (80, 24);

pub type ClientId = u32;

pub struct Core {
    opts: ServerOptions,
    config: Config,
    workspaces: Vec<Workspace>,
    panes: HashMap<PaneId, Pane>,
    clients: HashMap<ClientId, Client>,
    /// Pane-area size each workspace is laid out for.
    sizes: HashMap<WorkspaceId, (u16, u16)>,
    /// Interactive clients per workspace, for the session manager.
    attached: HashMap<WorkspaceId, usize>,
    subscribers: Vec<mpsc::Sender<ServerMsg>>,
    next_pane: PaneId,
    next_workspace: WorkspaceId,
    next_client: ClientId,
    clock: u64,
    paste_buffer: String,
    pane_tx: mpsc::Sender<PaneEvent>,
    pane_rx: Option<mpsc::Receiver<PaneEvent>>,
    events_tx: mpsc::Sender<Event>,
    events_rx: Option<mpsc::Receiver<Event>>,
    session_changed: bool,
    last_save: Instant,
    reload_at: Option<Instant>,
    had_workspaces: bool,
    stop: bool,
    /// Connections that haven't said Hello: control clients, or interactive
    /// ones still handshaking. Interactive ones move to `clients`.
    pending: HashMap<ClientId, mpsc::Sender<ServerMsg>>,
    /// Set after a partial restore: don't overwrite the session file until
    /// the user saves explicitly.
    protect_session_file: bool,
    _watcher: Option<notify::RecommendedWatcher>,
}
