//! The rust-multiplexer server core.
//!
//! One task owns all state (workspaces, panes, clients) and reacts to client
//! messages, pane output, config changes and a frame tick. Everything else
//! talks to it over channels, so there are no locks. Clients connect through
//! [`run`]'s `incoming` channel: in-memory in tests, sockets/pipes in `rmx`.

mod api;
pub mod compositor;
mod core;
mod mouse;
pub mod pane;
mod persist;
mod select;
pub mod state;
mod ui;

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::Result;
use rmx_proto::ServerEnd;
use tokio::sync::mpsc;

use pane::Pane;
use rmx_core::layout::PaneId;
use state::Tab;

#[derive(Debug, Clone)]
pub struct ServerOptions {
    /// Server name (`-L`), used for the session file.
    pub name: String,
    pub config_path: PathBuf,
    /// Restore the saved session at startup.
    pub restore: bool,
    /// Where sessions are saved; `None` disables autosave (tests).
    pub session_file: Option<PathBuf>,
    /// Watch the config file and reload on change.
    pub watch_config: bool,
}

impl ServerOptions {
    pub fn new(name: &str, config_path: PathBuf) -> Self {
        Self {
            name: name.to_owned(),
            session_file: Some(persist::default_path(name)),
            config_path,
            restore: false,
            watch_config: true,
        }
    }
}

/// Runs the server until it is stopped or its last workspace closes.
pub async fn run(incoming: mpsc::Receiver<ServerEnd>, opts: ServerOptions) -> Result<()> {
    core::Core::new(opts)?.run(incoming).await
}

/// A tab's name: the user's, else its active pane's title.
pub(crate) fn tab_title(tab: &Tab, panes: &HashMap<PaneId, Pane>) -> String {
    if let Some(name) = &tab.custom_name {
        return name.clone();
    }
    panes
        .get(&tab.active_pane)
        .map(pane_title)
        .unwrap_or_else(|| "…".into())
}

pub(crate) fn pane_title(pane: &Pane) -> String {
    let title = pane.emulator.title().trim();
    if title.is_empty() {
        pane.program.clone()
    } else {
        title.to_owned()
    }
}

/// `~/projects/x` rather than `/home/me/projects/x`, for compact display.
pub(crate) fn shorten_home(path: &Path) -> String {
    let home =
        std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).map(PathBuf::from);
    match home.and_then(|h| path.strip_prefix(&h).ok().map(Path::to_path_buf)) {
        Some(rest) if rest.as_os_str().is_empty() => "~".into(),
        Some(rest) => format!("~{}{}", std::path::MAIN_SEPARATOR, rest.display()),
        None => path.display().to_string(),
    }
}
