//! UI commands ([`Cmd`]) produced by overlays: menus, dialogs and the
//! session manager.

use anyhow::Result;
use rmx_config::action::Dir;

use super::{ClientId, Core, DEFAULT_SIZE};
use crate::state::WorkspaceId;
use crate::ui::{self, Cmd, Overlay};

impl Core {
    pub(super) fn run_cmd(&mut self, id: ClientId, cmd: Cmd) {
        let result: Result<()> = (|| {
            match cmd {
                Cmd::RenameTab {
                    workspace,
                    tab,
                    name,
                } => {
                    if let Some(t) = self
                        .workspace_mut(workspace)
                        .and_then(|w| w.tab_index(tab).map(|i| &mut w.tabs[i]))
                    {
                        t.custom_name = Some(name);
                        self.session_changed = true;
                    }
                }
                Cmd::RenameWorkspace { workspace, name } => {
                    if self
                        .workspaces
                        .iter()
                        .any(|w| w.name == name && w.id != workspace)
                    {
                        anyhow::bail!("a workspace named \"{name}\" already exists");
                    }
                    if let Some(w) = self.workspace_mut(workspace) {
                        w.name = name.clone();
                        self.session_changed = true;
                        self.emit(
                            "workspace-renamed",
                            serde_json::json!({ "workspace": workspace, "name": name }),
                        );
                    }
                }
                Cmd::NewWorkspace { name, cwd } => {
                    let ws = self.clients.get(&id).map(|c| c.workspace).unwrap_or(0);
                    let size = self.sizes.get(&ws).copied().unwrap_or(DEFAULT_SIZE);
                    let cwd = cwd.or_else(|| self.cwd_of_active(ws));
                    let new = self.new_workspace(name, cwd, None, size)?;
                    self.switch_workspace(id, new);
                }
                Cmd::Find(query) => {
                    if let Some(pane) = self.client_pane(id) {
                        self.with_select(id, pane, |sel, emu, _| {
                            sel.find(&query, false, emu);
                        });
                    }
                }
                Cmd::Open {
                    workspace,
                    tab,
                    pane,
                } => {
                    if let Some(w) = self.workspace_mut(workspace) {
                        if let Some(i) = tab.and_then(|t| w.tab_index(t)) {
                            w.select_tab(i);
                        }
                        if let Some(p) = pane {
                            let t = w.active_mut();
                            if t.layout.contains(p) {
                                t.active_pane = p;
                            }
                        }
                    }
                    self.switch_workspace(id, workspace);
                }
                Cmd::NewTab { workspace } => self.new_tab(workspace, None)?,
                Cmd::Split {
                    workspace,
                    tab,
                    right,
                } => {
                    let Some(tab) = self.workspace(workspace).and_then(|w| w.tab_index(tab)) else {
                        return Ok(());
                    };
                    self.split(
                        workspace,
                        tab,
                        if right { Dir::Right } else { Dir::Down },
                        50,
                        None,
                    )?;
                }
                Cmd::CloseWorkspace { workspace } => self.close_workspace(workspace),
                Cmd::CloseTab { workspace, tab } => {
                    if let Some(i) = self.workspace(workspace).and_then(|w| w.tab_index(tab)) {
                        self.close_tab(workspace, i);
                    }
                }
                Cmd::Confirm { message, then } => {
                    self.push_overlay(id, move |_| {
                        Overlay::Confirm(ui::dialog::Confirm::new(message, *then))
                    });
                }
                Cmd::Prompt {
                    title,
                    initial,
                    then,
                } => self.push_prompt(id, &title, initial, then),
                Cmd::QuitServer => self.stop = true,
            }
            Ok(())
        })();
        if let Err(err) = result {
            self.toast(id, format!("{err:#}"), true);
        }
        let ids: Vec<WorkspaceId> = self.workspaces.iter().map(|w| w.id).collect();
        for ws in ids {
            self.mark_workspace_dirty(ws);
        }
        if let Some(c) = self.clients.get_mut(&id) {
            c.dirty = true;
        }
    }
}
