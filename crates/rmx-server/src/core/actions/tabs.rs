//! Tab actions: create, close, rename, switch and reorder.

use anyhow::Result;
use rmx_config::action::Action;

use crate::core::{ClientId, Core};
use crate::state::WorkspaceId;
use crate::tab_title;
use crate::ui::PromptThen;

impl Core {
    pub(super) fn tab_action(
        &mut self,
        id: ClientId,
        ws: WorkspaceId,
        tab_idx: usize,
        action: Action,
    ) -> Result<()> {
        match action {
            Action::NewTab => self.new_tab(ws, None)?,
            Action::CloseTab => self.close_tab(ws, tab_idx),
            Action::RenameTab => {
                let initial = self
                    .workspace(ws)
                    .map(|w| tab_title(w.active(), &self.panes))
                    .unwrap_or_default();
                self.push_prompt(
                    id,
                    "Rename tab",
                    initial,
                    PromptThen::RenameTab {
                        workspace: ws,
                        tab: self
                            .workspace(ws)
                            .map(|w| w.active().id)
                            .unwrap_or_default(),
                    },
                );
            }
            Action::NextTab | Action::PrevTab => {
                let w = self.workspace_mut(ws).unwrap();
                let n = w.tabs.len();
                let next = if action == Action::NextTab {
                    (w.active_tab + 1) % n
                } else {
                    (w.active_tab + n - 1) % n
                };
                w.select_tab(next);
            }
            Action::GoToTab { index } => {
                let w = self.workspace_mut(ws).unwrap();
                w.select_tab(index.saturating_sub(1).min(w.tabs.len() - 1));
            }
            Action::MoveTab { delta } => {
                let w = self.workspace_mut(ws).unwrap();
                let to = (w.active_tab as i64 + i64::from(delta)).clamp(0, w.tabs.len() as i64 - 1)
                    as usize;
                w.tabs.swap(w.active_tab, to);
                w.active_tab = to;
                self.session_changed = true;
            }
            _ => unreachable!("routed by dispatch"),
        }
        Ok(())
    }
}
