//! Select (copy) mode: entering and leaving it, scrolling and copying.

use rmx_core::layout::PaneId;
use rmx_proto::ServerMsg;

use super::{ClientId, Core};
use crate::select::Select;

impl Core {
    pub(super) fn enter_select(&mut self, id: ClientId, pane: PaneId) {
        let Some(c) = self.clients.get_mut(&id) else {
            return;
        };
        c.engine.set_mode("select", true);
        if c.select.as_ref().is_none_or(|s| s.pane != pane)
            && let Some(p) = self.panes.get(&pane)
        {
            c.select = Some(Select::new(pane, p.emulator.as_ref()));
        }
    }

    pub(super) fn exit_select(&mut self, id: ClientId) {
        if let Some(c) = self.clients.get_mut(&id) {
            c.engine.exit_mode();
        }
        self.sync_select(id);
    }

    /// Runs `f` on the client's select state, entering select mode first.
    pub(super) fn with_select(
        &mut self,
        id: ClientId,
        pane: PaneId,
        f: impl FnOnce(&mut Select, &mut dyn rmx_core::emulator::Emulator, u16),
    ) {
        self.enter_select(id, pane);
        let Some(c) = self.clients.get_mut(&id) else {
            return;
        };
        let (Some(sel), Some(p)) = (c.select.as_mut(), self.panes.get_mut(&pane)) else {
            return;
        };
        // The pane's own width, so the cursor can't leave a narrow pane.
        let cols = p.emulator.cols();
        f(sel, p.emulator.as_mut(), cols);
    }

    pub(super) fn scroll(&mut self, id: ClientId, pane: PaneId, delta: isize) {
        let in_select = self.clients.get(&id).is_some_and(|c| c.select.is_some());
        if in_select {
            self.with_select(id, pane, |sel, emu, _| sel.scroll(delta, emu));
        } else if let Some(p) = self.panes.get_mut(&pane) {
            let offset = p.emulator.view_offset().saturating_add_signed(-delta);
            p.emulator
                .set_view_offset(offset.min(p.emulator.history_len()));
        }
    }

    pub(super) fn copy_selection(&mut self, id: ClientId) {
        let Some(c) = self.clients.get_mut(&id) else {
            return;
        };
        let Some(sel) = c.select.as_ref() else { return };
        let Some(p) = self.panes.get_mut(&sel.pane) else {
            return;
        };
        let Some(text) = sel.text(p.emulator.as_mut()).filter(|t| !t.is_empty()) else {
            return;
        };
        let n = text.chars().count();
        let _ = c.tx.try_send(ServerMsg::SetClipboard(text.clone()));
        self.paste_buffer = text;
        self.toast(
            id,
            format!("Copied {n} character{}", if n == 1 { "" } else { "s" }),
            false,
        );
    }
}
