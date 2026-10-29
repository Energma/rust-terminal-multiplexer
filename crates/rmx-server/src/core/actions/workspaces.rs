//! Workspace actions: the switcher, create, rename, cycle and detach.

use anyhow::Result;
use rmx_config::action::Action;

use crate::core::{ClientId, Core, DEFAULT_SIZE};
use crate::state::WorkspaceId;
use crate::ui::{self, Overlay, PromptThen};

impl Core {
    pub(super) fn workspace_action(
        &mut self,
        id: ClientId,
        ws: WorkspaceId,
        action: Action,
    ) -> Result<()> {
        match action {
            Action::WorkspaceSwitcher => {
                self.push_overlay(id, |ctx| Overlay::Manager(ui::manager::Manager::new(ctx)))
            }
            Action::NewWorkspace => {
                let size = self.sizes.get(&ws).copied().unwrap_or(DEFAULT_SIZE);
                let cwd = self.cwd_of_active(ws);
                let new = self.new_workspace(None, cwd, None, size)?;
                self.switch_workspace(id, new);
            }
            Action::RenameWorkspace => {
                let initial = self
                    .workspace(ws)
                    .map(|w| w.name.clone())
                    .unwrap_or_default();
                self.push_prompt(
                    id,
                    "Rename workspace",
                    initial,
                    PromptThen::RenameWorkspace { workspace: ws },
                );
            }
            Action::NextWorkspace | Action::PrevWorkspace => {
                let i = self.workspaces.iter().position(|w| w.id == ws).unwrap_or(0);
                let n = self.workspaces.len();
                let j = if action == Action::NextWorkspace {
                    (i + 1) % n
                } else {
                    (i + n - 1) % n
                };
                let to = self.workspaces[j].id;
                self.switch_workspace(id, to);
            }
            Action::Detach => self.detach(id),
            _ => unreachable!("routed by dispatch"),
        }
        Ok(())
    }
}
