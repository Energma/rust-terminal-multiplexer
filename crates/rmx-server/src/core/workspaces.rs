//! Workspaces: lookup, naming, creating, closing and switching clients
//! between them.

use std::path::PathBuf;

use anyhow::Result;
use rmx_proto::ServerMsg;

use super::client::end_select;
use super::{ClientId, Core};
use crate::pane::SpawnSpec;
use crate::state::{Tab, Workspace, WorkspaceId};

impl Core {
    pub(super) fn touch(&mut self, ws: WorkspaceId) {
        self.clock += 1;
        let clock = self.clock;
        if let Some(w) = self.workspace_mut(ws) {
            w.last_used = clock;
        }
    }

    pub(super) fn workspace(&self, id: WorkspaceId) -> Option<&Workspace> {
        self.workspaces.iter().find(|w| w.id == id)
    }

    pub(super) fn workspace_mut(&mut self, id: WorkspaceId) -> Option<&mut Workspace> {
        self.workspaces.iter_mut().find(|w| w.id == id)
    }

    fn unique_name(&self, wanted: Option<String>) -> String {
        let taken = |n: &str| self.workspaces.iter().any(|w| w.name == n);
        if let Some(name) = wanted.filter(|n| !n.trim().is_empty()) {
            let name = name.trim().to_owned();
            if !taken(&name) {
                return name;
            }
            return (2..)
                .map(|i| format!("{name}-{i}"))
                .find(|n| !taken(n))
                .unwrap();
        }
        let first = if self.workspaces.is_empty() {
            "main".to_owned()
        } else {
            String::new()
        };
        if !first.is_empty() && !taken(&first) {
            return first;
        }
        (1..).map(|i| i.to_string()).find(|n| !taken(n)).unwrap()
    }

    pub(super) fn new_workspace(
        &mut self,
        name: Option<String>,
        cwd: Option<PathBuf>,
        command: Option<String>,
        size: (u16, u16),
    ) -> Result<WorkspaceId> {
        let cwd_for_name = cwd.clone();
        let pane = self.spawn_pane(
            SpawnSpec {
                cwd,
                command,
                ..Default::default()
            },
            size,
        )?;
        let id = self.next_workspace;
        self.next_workspace += 1;
        // Unnamed workspaces are named after their folder ("api", "rust-mux"),
        // which is what people look for in the session manager.
        let name = self.unique_name(name.or_else(|| folder_name(cwd_for_name.as_deref())));
        self.workspaces.push(Workspace {
            id,
            name: name.clone(),
            tabs: vec![Tab::new(pane)],
            active_tab: 0,
            last_tab: 0,
            last_used: 0,
        });
        self.sizes.insert(id, size);
        self.had_workspaces = true;
        self.session_changed = true;
        self.emit(
            "workspace-created",
            serde_json::json!({ "workspace": id, "name": name }),
        );
        Ok(id)
    }

    pub(super) fn close_workspace(&mut self, ws: WorkspaceId) {
        let Some(i) = self.workspaces.iter().position(|w| w.id == ws) else {
            return;
        };
        let removed = self.workspaces.remove(i);
        for p in removed.pane_ids() {
            self.panes.remove(&p);
        }
        self.sizes.remove(&ws);
        self.session_changed = true;
        self.emit(
            "workspace-closed",
            serde_json::json!({ "workspace": ws, "name": removed.name }),
        );
        // Clients move to the most recently used workspace, or leave.
        let fallback = self
            .workspaces
            .iter()
            .max_by_key(|w| w.last_used)
            .map(|w| w.id);
        let ids: Vec<ClientId> = self
            .clients
            .iter()
            .filter(|(_, c)| c.workspace == ws)
            .map(|(id, _)| *id)
            .collect();
        for id in ids {
            match fallback {
                Some(to) => self.switch_workspace(id, to),
                None => {
                    if let Some(c) = self.clients.remove(&id) {
                        let _ = c.tx.try_send(ServerMsg::Exit {
                            reason: "workspace closed".into(),
                        });
                        self.emit("client-detached", serde_json::json!({ "client": id }));
                        self.recompute_sizes();
                    }
                }
            }
        }
    }

    pub(super) fn switch_workspace(&mut self, client: ClientId, ws: WorkspaceId) {
        if let Some(c) = self.clients.get_mut(&client) {
            end_select(c, &mut self.panes);
            c.workspace = ws;
            c.dirty = true;
        }
        self.touch(ws);
        self.recompute_sizes();
    }
}

fn folder_name(dir: Option<&std::path::Path>) -> Option<String> {
    dir?.file_name().map(|n| n.to_string_lossy().into_owned())
}
