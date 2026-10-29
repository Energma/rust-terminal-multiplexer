//! Sizing and redraw bookkeeping: which size each workspace is laid out for
//! and which clients need a new frame.

use std::collections::HashMap;

use ratatui::layout::Rect;
use rmx_core::layout::PaneId;

use super::{Core, DEFAULT_SIZE};
use crate::compositor::{self, tab_rects};
use crate::state::WorkspaceId;

impl Core {
    /// Each workspace is laid out for the smallest client viewing it, so
    /// every client sees the whole thing (tmux's `window-size smallest`).
    pub(super) fn recompute_sizes(&mut self) {
        self.attached = self.clients_by_ws();
        let mut sizes: HashMap<WorkspaceId, (u16, u16)> = HashMap::new();
        for c in self.clients.values() {
            let area = compositor::pane_area(Rect::new(0, 0, c.size.0, c.size.1));
            let s = sizes
                .entry(c.workspace)
                .or_insert((area.width, area.height));
            *s = (s.0.min(area.width), s.1.min(area.height));
        }
        for (ws, size) in sizes {
            if self.sizes.get(&ws) != Some(&size) {
                self.sizes.insert(ws, size);
            }
        }
        let ids: Vec<WorkspaceId> = self.workspaces.iter().map(|w| w.id).collect();
        for ws in ids {
            self.relayout(ws);
        }
    }

    /// Resizes every pane of a workspace to its layout and marks viewers dirty.
    pub(super) fn relayout(&mut self, ws: WorkspaceId) {
        let (w, h) = self.sizes.get(&ws).copied().unwrap_or(DEFAULT_SIZE);
        let area = Rect::new(0, 0, w.max(1), h.max(1));
        if let Some(workspace) = self.workspaces.iter().find(|x| x.id == ws) {
            for tab in &workspace.tabs {
                for (id, rect) in tab_rects(tab, area).0 {
                    if let Some(p) = self.panes.get_mut(&id) {
                        p.resize(rect.height, rect.width);
                    }
                }
            }
        }
        self.mark_workspace_dirty(ws);
    }

    pub(super) fn mark_workspace_dirty(&mut self, ws: WorkspaceId) {
        for c in self.clients.values_mut().filter(|c| c.workspace == ws) {
            c.dirty = true;
        }
    }

    pub(super) fn mark_pane_dirty(&mut self, pane: PaneId) {
        // Viewers of the pane's workspace, plus anyone with an overlay open
        // (the session manager previews any workspace).
        let ws = self
            .workspaces
            .iter()
            .find(|w| w.tab_of(pane).is_some())
            .map(|w| w.id);
        for c in self.clients.values_mut() {
            if Some(c.workspace) == ws || !c.overlays.is_empty() {
                c.dirty = true;
            }
        }
    }

    pub(super) fn view_rect(&self, ws: WorkspaceId) -> Rect {
        let (w, h) = self.sizes.get(&ws).copied().unwrap_or(DEFAULT_SIZE);
        Rect::new(0, 0, w, h)
    }

    pub(super) fn clients_by_ws(&self) -> HashMap<WorkspaceId, usize> {
        let mut m = HashMap::new();
        for c in self.clients.values() {
            *m.entry(c.workspace).or_insert(0) += 1;
        }
        m
    }
}
