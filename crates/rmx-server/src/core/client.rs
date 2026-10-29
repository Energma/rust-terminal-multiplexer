//! Per-connection client state, and the helpers that work on a client while
//! other core fields are borrowed.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Instant;

use rmx_config::Config;
use rmx_config::keymap::{KeyEngine, Keymap};
use rmx_core::layout::PaneId;
use rmx_proto::ServerMsg;
use tokio::sync::mpsc;

use crate::ServerOptions;
use crate::compositor::Compositor;
use crate::pane::Pane;
use crate::select::Select;
use crate::state::{Workspace, WorkspaceId};
use crate::ui::{Ctx, Overlay};

pub(super) struct Client {
    pub(super) tx: mpsc::Sender<ServerMsg>,
    /// Where the user ran `rmx`: the session manager offers it for new workspaces.
    pub(super) launch_cwd: Option<PathBuf>,
    pub(super) size: (u16, u16),
    pub(super) workspace: WorkspaceId,
    pub(super) engine: KeyEngine,
    pub(super) keymap: Keymap,
    pub(super) enhanced_keys: bool,
    pub(super) compositor: Compositor,
    pub(super) overlays: Vec<Overlay>,
    pub(super) hints: Option<Vec<(String, String)>>,
    pub(super) toast: Option<(String, Instant, bool)>,
    pub(super) select: Option<Select>,
    pub(super) drag: Option<Drag>,
    pub(super) dirty: bool,
}

pub(super) enum Drag {
    Border {
        pane: PaneId,
        vertical: bool,
        last: u16,
    },
    Select,
    /// The program in the pane asked for mouse events; keep sending them.
    App(PaneId),
}

/// Leaves select mode and returns the pane's view to the live screen.
pub(super) fn end_select(c: &mut Client, panes: &mut HashMap<PaneId, Pane>) {
    if let Some(sel) = c.select.take()
        && let Some(p) = panes.get_mut(&sel.pane)
    {
        p.emulator.set_view_offset(0);
    }
    c.drag = None;
    c.engine.exit_mode();
}

// Takes the core's fields one by one (not `&self`) so callers can hold a
// `&mut` client at the same time; hence the long argument list.
#[allow(clippy::too_many_arguments)]
pub(super) fn make_ctx<'a>(
    config: &'a Config,
    keymap: &'a Keymap,
    workspaces: &'a [Workspace],
    panes: &'a HashMap<PaneId, Pane>,
    attached: &'a HashMap<WorkspaceId, usize>,
    current: WorkspaceId,
    launch_cwd: Option<&'a std::path::Path>,
    opts: &'a ServerOptions,
) -> Ctx<'a> {
    Ctx {
        theme: &config.theme,
        keymap,
        workspaces,
        panes,
        attached,
        current,
        launch_cwd,
        config_path: &opts.config_path,
        commands: &config.commands,
    }
}
