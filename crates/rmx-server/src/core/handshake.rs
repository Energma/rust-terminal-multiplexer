//! The Hello handshake that turns a connection into an interactive client.

use std::path::PathBuf;
use std::time::Instant;

use ratatui::layout::Rect;
use rmx_config::keymap::KeyEngine;
use rmx_proto::{Attach, Hello, PROTOCOL_VERSION, ServerMsg};

use super::client::{Client, make_ctx};
use super::{ClientId, Core};
use crate::compositor::{self, Compositor};
use crate::ui::{self, Overlay};

impl Core {
    pub(super) fn hello(&mut self, id: ClientId, mut hello: Hello) {
        let Some(tx) = self.pending_conns().remove(&id) else {
            return;
        };
        (hello.cols, hello.rows) = clamp_size(hello.cols, hello.rows);
        if hello.version != PROTOCOL_VERSION {
            let reason = format!(
                "rmx server is protocol v{PROTOCOL_VERSION} but this client is v{}; run `rmx stop` and try again",
                hello.version
            );
            let _ = tx.try_send(ServerMsg::Exit { reason });
            return;
        }
        let pane_size = pane_size_for(hello.cols, hello.rows);
        let workspace = match hello.attach {
            Attach::Last => match self.workspaces.iter().max_by_key(|w| w.last_used) {
                Some(w) => Ok(w.id),
                None => {
                    self.new_workspace(None, hello.cwd.clone().map(PathBuf::from), None, pane_size)
                }
            },
            Attach::Named(name) => self
                .workspaces
                .iter()
                .find(|w| w.name == name)
                .map(|w| w.id)
                .ok_or_else(|| anyhow::anyhow!("no workspace named \"{name}\" (see `rmx list`)")),
            Attach::New { name, cwd } => {
                self.new_workspace(name, cwd.map(PathBuf::from), None, pane_size)
            }
        };
        let workspace = match workspace {
            Ok(w) => w,
            Err(err) => {
                let _ = tx.try_send(ServerMsg::Exit {
                    reason: format!("{err:#}"),
                });
                return;
            }
        };
        let direct = self.config.direct_enabled(hello.enhanced_keys);
        let keymap = self.config.keymap(direct);
        let _ = tx.try_send(ServerMsg::Welcome {
            mouse: self.config.general.mouse,
        });
        let leader = keymap.leader().display_short();
        let mut client = Client {
            tx,
            launch_cwd: hello.cwd.map(PathBuf::from).filter(|d| d.is_dir()),
            size: (hello.cols, hello.rows),
            workspace,
            engine: KeyEngine::new(),
            keymap,
            enhanced_keys: hello.enhanced_keys,
            compositor: Compositor::new(hello.cols, hello.rows),
            overlays: Vec::new(),
            hints: None,
            toast: Some((
                format!("{leader} leader · F10 menu · F1 help"),
                Instant::now(),
                false,
            )),
            select: None,
            drag: None,
            dirty: true,
        };
        if hello.open_manager {
            let ctx = make_ctx(
                &self.config,
                &client.keymap,
                &self.workspaces,
                &self.panes,
                &self.attached,
                workspace,
                client.launch_cwd.as_deref(),
                &self.opts,
            );
            client
                .overlays
                .push(Overlay::Manager(ui::manager::Manager::new(&ctx)));
        }
        self.clients.insert(id, client);
        self.touch(workspace);
        tracing::info!(client = id, workspace, "client attached");
        self.emit(
            "client-attached",
            serde_json::json!({ "client": id, "workspace": workspace }),
        );
        self.recompute_sizes();
    }
}

/// Smallest screen we lay out for. Smaller terminals show the top-left of
/// it; every layout and overlay can assume at least this much room.
const MIN_SIZE: (u16, u16) = (40, 10);

pub(super) fn clamp_size(cols: u16, rows: u16) -> (u16, u16) {
    (cols.max(MIN_SIZE.0), rows.max(MIN_SIZE.1))
}

fn pane_size_for(cols: u16, rows: u16) -> (u16, u16) {
    let a = compositor::pane_area(Rect::new(0, 0, cols, rows));
    (a.width.max(1), a.height.max(1))
}
