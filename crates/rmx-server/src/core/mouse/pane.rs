//! Mouse events on a pane: focus, forwarding to programs that asked for
//! the mouse, selection, the context menu, paste and the wheel.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;
use rmx_config::action::Action;
use rmx_core::encode::encode_key;
use rmx_core::layout::PaneId;

use crate::core::client::Drag;
use crate::core::{ClientId, Core};
use crate::select::Select;
use crate::state::{Workspace, WorkspaceId};
use crate::ui::{self, Overlay};

impl Core {
    pub(super) fn on_pane_mouse(
        &mut self,
        id: ClientId,
        ws_id: WorkspaceId,
        ev: &MouseEvent,
        active_pane: PaneId,
        pane_id: PaneId,
        rect: Rect,
    ) {
        let (x, y) = (ev.column, ev.row);
        let shift = ev.modifiers.contains(KeyModifiers::SHIFT);
        let wants_mouse = self
            .panes
            .get(&pane_id)
            .is_some_and(|p| p.emulator.mouse_mode() != rmx_core::emulator::MouseMode::None);

        if matches!(ev.kind, MouseEventKind::Down(_)) && pane_id != active_pane {
            if let Some(t) = self.workspace_mut(ws_id).map(Workspace::active_mut) {
                t.active_pane = pane_id;
            }
            self.exit_select(id);
            self.mark_workspace_dirty(ws_id);
        }

        // Programs that asked for the mouse get it (Shift overrides, as in
        // most terminals).
        if wants_mouse && !shift {
            self.forward_mouse(pane_id, ev, rect);
            if matches!(ev.kind, MouseEventKind::Down(_)) {
                self.clients.get_mut(&id).unwrap().drag = Some(Drag::App(pane_id));
            }
            return;
        }

        match ev.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                let (row, col) = (y - rect.y, x - rect.x);
                if let Some(p) = self.panes.get(&pane_id) {
                    let mut sel = Select::at(pane_id, p.emulator.as_ref(), row, col);
                    sel.anchor = Some(sel.cursor);
                    sel.by_mouse = true;
                    let c = self.clients.get_mut(&id).unwrap();
                    c.engine.set_mode("select", true);
                    c.select = Some(sel);
                    c.drag = Some(Drag::Select);
                }
            }
            MouseEventKind::Down(MouseButton::Right) => {
                let has_selection = self
                    .clients
                    .get(&id)
                    .and_then(|c| c.select.as_ref())
                    .is_some_and(Select::has_selection);
                self.push_overlay(id, |_| {
                    Overlay::Context(ui::menu::ContextMenu::new(x, y, has_selection))
                });
            }
            MouseEventKind::Down(MouseButton::Middle) => self.dispatch(id, Action::Paste),
            MouseEventKind::ScrollUp | MouseEventKind::ScrollDown => {
                let up = matches!(ev.kind, MouseEventKind::ScrollUp);
                let alt = self
                    .panes
                    .get(&pane_id)
                    .is_some_and(|p| p.emulator.alternate_screen());
                if alt {
                    // Full-screen programs without mouse support (less, man)
                    // scroll with arrow keys, as terminals do.
                    let key = KeyEvent::from(if up { KeyCode::Up } else { KeyCode::Down });
                    if let Some(p) = self.panes.get(&pane_id)
                        && let Some(bytes) = encode_key(&key, p.emulator.application_cursor())
                    {
                        p.write(bytes.repeat(3));
                    }
                } else {
                    self.scroll(id, pane_id, if up { -3 } else { 3 });
                }
            }
            _ => {}
        }
    }

    pub(super) fn forward_mouse(&self, pane: PaneId, ev: &MouseEvent, rect: Rect) {
        let Some(p) = self.panes.get(&pane) else {
            return;
        };
        let col = ev
            .column
            .saturating_sub(rect.x)
            .min(rect.width.saturating_sub(1));
        let row = ev
            .row
            .saturating_sub(rect.y)
            .min(rect.height.saturating_sub(1));
        if let Some(bytes) = crate::mouse::encode(
            ev,
            col,
            row,
            p.emulator.mouse_mode(),
            p.emulator.mouse_sgr(),
        ) {
            p.write(bytes);
        }
    }
}
