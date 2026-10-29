//! Ongoing mouse drags: border resizes, text selection, and drags owned by
//! a program in a pane.

use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;
use rmx_core::layout::{PaneId, Side};

use crate::core::client::Drag;
use crate::core::{ClientId, Core};
use crate::state::{Workspace, WorkspaceId};

impl Core {
    /// Feeds `ev` to the client's ongoing drag, if any; true if the drag
    /// consumed it. A drag that doesn't match the event is dropped.
    pub(super) fn continue_drag(
        &mut self,
        id: ClientId,
        ws_id: WorkspaceId,
        ev: &MouseEvent,
        active_pane: PaneId,
        area: Rect,
        rects: &[(PaneId, Rect)],
    ) -> bool {
        let (x, y) = (ev.column, ev.row);
        let drag = self.clients.get_mut(&id).and_then(|c| c.drag.take());
        match (drag, ev.kind) {
            (
                Some(Drag::Border {
                    pane,
                    vertical,
                    last,
                }),
                MouseEventKind::Drag(MouseButton::Left),
            ) => {
                let pos = if vertical { x } else { y };
                let delta = i32::from(pos) - i32::from(last);
                if delta != 0 {
                    let side = match (vertical, delta > 0) {
                        (true, true) => Side::Right,
                        (true, false) => Side::Left,
                        (false, true) => Side::Down,
                        (false, false) => Side::Up,
                    };
                    let view = self.view_rect(ws_id);
                    if let Some(t) = self.workspace_mut(ws_id).map(Workspace::active_mut) {
                        t.layout
                            .resize(pane, side, delta.unsigned_abs() as u16, view);
                    }
                    self.session_changed = true;
                    self.relayout(ws_id);
                }
                self.clients.get_mut(&id).unwrap().drag = Some(Drag::Border {
                    pane,
                    vertical,
                    last: pos,
                });
                true
            }
            (Some(Drag::Border { .. }), MouseEventKind::Up(_)) => true,
            (Some(Drag::Select), MouseEventKind::Drag(MouseButton::Left)) => {
                self.drag_select(id, active_pane, area, rects, x, y);
                self.clients.get_mut(&id).unwrap().drag = Some(Drag::Select);
                true
            }
            (Some(Drag::Select), MouseEventKind::Up(MouseButton::Left)) => {
                let (has, by_mouse) = self
                    .clients
                    .get(&id)
                    .and_then(|c| c.select.as_ref())
                    .map(|s| (s.anchor.is_some_and(|a| a != s.cursor), s.by_mouse))
                    .unwrap_or((false, false));
                if has && self.config.general.copy_on_select {
                    self.copy_selection(id);
                }
                if by_mouse && (!has || self.config.general.copy_on_select) {
                    self.exit_select(id);
                }
                true
            }
            (Some(Drag::App(pane)), kind) => {
                let rect = rects.iter().find(|(p, _)| *p == pane).map(|(_, r)| *r);
                if let Some(rect) = rect {
                    self.forward_mouse(pane, ev, rect);
                }
                if !matches!(kind, MouseEventKind::Up(_)) {
                    self.clients.get_mut(&id).unwrap().drag = Some(Drag::App(pane));
                }
                true
            }
            _ => false,
        }
    }

    fn drag_select(
        &mut self,
        id: ClientId,
        pane: PaneId,
        _area: Rect,
        rects: &[(PaneId, Rect)],
        x: u16,
        y: u16,
    ) {
        let Some(rect) = rects.iter().find(|(p, _)| *p == pane).map(|(_, r)| *r) else {
            return;
        };
        let Some(c) = self.clients.get_mut(&id) else {
            return;
        };
        let (Some(sel), Some(p)) = (c.select.as_mut(), self.panes.get_mut(&pane)) else {
            return;
        };
        let emu = p.emulator.as_mut();
        // Dragging past the top or bottom edge scrolls.
        if y < rect.y {
            sel.scroll(-1, emu);
        } else if y >= rect.bottom() {
            sel.scroll(1, emu);
        } else {
            let top = crate::select::view_top(emu);
            sel.cursor = (
                top + usize::from(y - rect.y),
                x.saturating_sub(rect.x).min(rect.width.saturating_sub(1)),
            );
        }
    }
}
