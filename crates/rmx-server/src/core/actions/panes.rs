//! Pane actions within the active tab: split, close, focus, resize, zoom,
//! swap, layout presets and synchronised input.

use anyhow::Result;
use ratatui::layout::Rect;
use rmx_config::action::Action;
use rmx_core::layout::{PRESETS, PaneId};

use crate::core::helpers::side;
use crate::core::{ClientId, Core};
use crate::state::WorkspaceId;

impl Core {
    pub(super) fn pane_action(
        &mut self,
        id: ClientId,
        ws: WorkspaceId,
        tab_idx: usize,
        pane: PaneId,
        area: Rect,
        action: Action,
    ) -> Result<()> {
        match action {
            Action::SplitPane {
                direction,
                size,
                command,
            } => {
                self.split(ws, tab_idx, direction, size.unwrap_or(50), command)?;
            }
            Action::ClosePane => self.remove_pane(pane),
            Action::MoveFocus { direction } => {
                let t = self.workspace_mut(ws).unwrap().active_mut();
                t.zoomed = false;
                if let Some(n) = t.layout.neighbor(pane, side(direction), area) {
                    t.active_pane = n;
                }
                self.relayout(ws);
            }
            Action::ResizePane { direction, amount } => {
                let t = self.workspace_mut(ws).unwrap().active_mut();
                if t.layout.resize(pane, side(direction), amount, area) {
                    self.session_changed = true;
                    self.relayout(ws);
                }
            }
            Action::TogglePaneZoom => {
                let t = self.workspace_mut(ws).unwrap().active_mut();
                t.zoomed = !t.zoomed && t.layout.panes().len() > 1;
                self.relayout(ws);
            }
            Action::SwapPane { direction } => {
                let t = self.workspace_mut(ws).unwrap().active_mut();
                t.layout.swap(pane, side(direction), area);
                self.session_changed = true;
                self.relayout(ws);
            }
            Action::ApplyLayout { name } => {
                let t = self.workspace_mut(ws).unwrap().active_mut();
                let name = if name == "next" {
                    let n = PRESETS[t.preset_cursor % PRESETS.len()];
                    t.preset_cursor += 1;
                    n.to_owned()
                } else {
                    name
                };
                t.zoomed = false;
                if !t.layout.apply_preset(&name) {
                    anyhow::bail!("unknown layout \"{name}\" (try: {})", PRESETS.join(", "));
                }
                self.session_changed = true;
                self.relayout(ws);
            }
            Action::ToggleSyncInput => {
                let t = self.workspace_mut(ws).unwrap().active_mut();
                t.sync_input = !t.sync_input;
                let on = t.sync_input;
                self.toast(
                    id,
                    if on {
                        "Typing goes to all panes in this tab"
                    } else {
                        "Typing goes to the focused pane"
                    },
                    false,
                );
            }
            _ => unreachable!("routed by dispatch"),
        }
        Ok(())
    }
}
