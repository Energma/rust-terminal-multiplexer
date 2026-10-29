//! Key-bound [`Action`]s: `dispatch` resolves the client's focus once and
//! routes each action to the handler for its area.

mod app;
mod panes;
mod select;
mod tabs;
mod workspaces;

use anyhow::Result;
use ratatui::layout::Rect;
use rmx_config::action::Action;
use rmx_core::layout::PaneId;

use super::{ClientId, Core};
use crate::state::WorkspaceId;

impl Core {
    pub(super) fn dispatch(&mut self, id: ClientId, action: Action) {
        let Some(c) = self.clients.get(&id) else {
            return;
        };
        let ws = c.workspace;
        let Some(w) = self.workspace(ws) else { return };
        let tab_idx = w.active_tab;
        let pane = w.active().active_pane;
        let area = self.view_rect(ws);
        let result = self.route_action(id, ws, tab_idx, pane, area, action);
        if let Err(err) = result {
            tracing::warn!("action failed: {err:#}");
            self.toast(id, format!("{err:#}"), true);
        }
        self.sync_select(id);
        self.mark_workspace_dirty(ws);
    }

    fn route_action(
        &mut self,
        id: ClientId,
        ws: WorkspaceId,
        tab_idx: usize,
        pane: PaneId,
        area: Rect,
        action: Action,
    ) -> Result<()> {
        match action {
            a @ (Action::NewTab
            | Action::CloseTab
            | Action::RenameTab
            | Action::NextTab
            | Action::PrevTab
            | Action::GoToTab { .. }
            | Action::MoveTab { .. }) => self.tab_action(id, ws, tab_idx, a),
            a @ (Action::SplitPane { .. }
            | Action::ClosePane
            | Action::MoveFocus { .. }
            | Action::ResizePane { .. }
            | Action::TogglePaneZoom
            | Action::SwapPane { .. }
            | Action::ApplyLayout { .. }
            | Action::ToggleSyncInput) => self.pane_action(id, ws, tab_idx, pane, area, a),
            a @ (Action::SelectMode
            | Action::Find
            | Action::FindNext
            | Action::FindPrev
            | Action::Copy
            | Action::CopyAndExit
            | Action::Paste
            | Action::SelectMove { .. }
            | Action::SelectExtend { .. }
            | Action::SelectLineStart
            | Action::SelectLineEnd
            | Action::SelectStart
            | Action::SelectLine
            | Action::SelectWordLeft
            | Action::SelectWordRight
            | Action::ScrollUp { .. }
            | Action::ScrollDown { .. }
            | Action::ScrollPageUp
            | Action::ScrollPageDown
            | Action::ScrollToTop
            | Action::ScrollToBottom) => self.select_action(id, pane, area, a),
            a @ (Action::WorkspaceSwitcher
            | Action::NewWorkspace
            | Action::RenameWorkspace
            | Action::NextWorkspace
            | Action::PrevWorkspace
            | Action::Detach) => self.workspace_action(id, ws, a),
            a @ (Action::CommandPalette
            | Action::OpenMenu
            | Action::ShowHelp
            | Action::Quit
            | Action::ReloadConfig
            | Action::OpenConfig
            | Action::EnterMode { .. }
            | Action::ExitMode
            | Action::Lock
            | Action::Run { .. }
            | Action::SendKeys { .. }) => self.app_action(id, ws, tab_idx, pane, a),
        }
    }
}
