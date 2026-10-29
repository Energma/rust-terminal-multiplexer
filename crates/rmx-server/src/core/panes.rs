//! Panes and the tabs that hold them: spawning, new tabs, splits and
//! removal.

use std::path::PathBuf;

use anyhow::Result;
use rmx_config::action::Dir;
use rmx_core::layout::PaneId;

use super::helpers::side;
use super::{Core, DEFAULT_SIZE};
use crate::pane::{Pane, SpawnSpec};
use crate::state::{Tab, WorkspaceId};

impl Core {
    pub(super) fn spawn_pane(&mut self, spec: SpawnSpec, size: (u16, u16)) -> Result<PaneId> {
        let id = self.next_pane;
        let mut spec = SpawnSpec {
            shell: spec.shell.or_else(|| self.config.general.shell.clone()),
            ..spec
        };
        // `rmx ls` etc. inside a pane should talk to *this* server (like $TMUX).
        spec.env.push(("RMX_SERVER".into(), self.opts.name.clone()));
        let pane = Pane::spawn(
            id,
            &spec,
            size.1.max(1),
            size.0.max(1),
            self.config.general.scrollback,
            self.pane_tx.clone(),
        )?;
        self.next_pane += 1;
        self.panes.insert(id, pane);
        self.emit("pane-created", serde_json::json!({ "pane": id }));
        Ok(id)
    }

    pub(super) fn active_pane_of(&self, ws: WorkspaceId) -> Option<PaneId> {
        self.workspace(ws).map(|w| w.active().active_pane)
    }

    pub(super) fn cwd_of_active(&self, ws: WorkspaceId) -> Option<PathBuf> {
        self.active_pane_of(ws)
            .and_then(|p| self.panes.get(&p))
            .map(Pane::cwd)
    }

    pub(super) fn new_tab(&mut self, ws: WorkspaceId, command: Option<String>) -> Result<()> {
        let size = self.sizes.get(&ws).copied().unwrap_or(DEFAULT_SIZE);
        let cwd = self.cwd_of_active(ws);
        let pane = self.spawn_pane(
            SpawnSpec {
                cwd,
                command,
                ..Default::default()
            },
            size,
        )?;
        let w = self.workspace_mut(ws).expect("workspace exists");
        let at = (w.active_tab + 1).min(w.tabs.len());
        w.tabs.insert(at, Tab::new(pane));
        w.last_tab = w.active_tab;
        w.active_tab = at;
        self.session_changed = true;
        self.mark_workspace_dirty(ws);
        Ok(())
    }

    pub(super) fn split(
        &mut self,
        ws: WorkspaceId,
        tab: usize,
        dir: Dir,
        percent: u16,
        command: Option<String>,
    ) -> Result<()> {
        let size = self.sizes.get(&ws).copied().unwrap_or(DEFAULT_SIZE);
        let Some(target) = self
            .workspace(ws)
            .and_then(|w| w.tabs.get(tab))
            .map(|t| t.active_pane)
        else {
            return Ok(());
        };
        let cwd = self.panes.get(&target).map(Pane::cwd);
        let estimate = (size.0 / 2, size.1 / 2);
        let pane = self.spawn_pane(
            SpawnSpec {
                cwd,
                command,
                ..Default::default()
            },
            estimate,
        )?;
        let w = self.workspace_mut(ws).expect("workspace exists");
        let t = &mut w.tabs[tab];
        t.zoomed = false;
        t.layout.split(target, pane, side(dir), percent);
        t.active_pane = pane;
        self.session_changed = true;
        self.relayout(ws);
        Ok(())
    }

    /// Removes a pane from wherever it is, closing its tab and workspace if
    /// it was the last; moves clients off a closed workspace.
    pub(super) fn remove_pane(&mut self, pane: PaneId) {
        self.panes.remove(&pane);
        for client in self.clients.values_mut() {
            if client.select.as_ref().is_some_and(|s| s.pane == pane) {
                client.select = None;
                client.engine.exit_mode();
            }
        }
        let Some(wi) = self
            .workspaces
            .iter()
            .position(|w| w.tab_of(pane).is_some())
        else {
            return;
        };
        let ws_id = self.workspaces[wi].id;
        let ti = self.workspaces[wi].tab_of(pane).unwrap();
        let tab = &mut self.workspaces[wi].tabs[ti];
        if tab.layout.remove(pane) {
            if tab.active_pane == pane {
                tab.active_pane = tab.layout.panes()[0];
            }
            tab.zoomed = false;
        } else {
            self.workspaces[wi].remove_tab(ti);
        }
        self.session_changed = true;
        if self.workspaces[wi].tabs.is_empty() {
            self.close_workspace(ws_id);
        } else {
            self.relayout(ws_id);
        }
    }

    pub(super) fn close_tab(&mut self, ws: WorkspaceId, tab: usize) {
        let panes = self
            .workspace(ws)
            .and_then(|w| w.tabs.get(tab))
            .map(|t| t.layout.panes())
            .unwrap_or_default();
        for p in panes {
            self.remove_pane(p);
        }
    }
}
