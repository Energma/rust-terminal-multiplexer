//! Mouse input, in priority order: overlays, an ongoing drag, the menu and
//! status bars, border grabs, and finally the pane under the pointer.

mod drag;
mod pane;

use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;
use rmx_config::action::Action;
use rmx_core::layout::{PaneId, Separator};

use super::client::{Drag, make_ctx};
use super::{ClientId, Core, DEFAULT_SIZE};
use crate::compositor::{self, tab_rects};
use crate::state::WorkspaceId;
use crate::ui::{self, Overlay};

impl Core {
    pub(super) fn on_mouse(&mut self, id: ClientId, ev: MouseEvent) {
        let Some(c) = self.clients.get_mut(&id) else {
            return;
        };
        c.dirty = true;
        let screen = Rect::new(0, 0, c.size.0, c.size.1);
        if !c.overlays.is_empty() {
            let ctx = make_ctx(
                &self.config,
                &c.keymap,
                &self.workspaces,
                &self.panes,
                &self.attached,
                c.workspace,
                c.launch_cwd.as_deref(),
                &self.opts,
            );
            let outcome = c
                .overlays
                .last_mut()
                .unwrap()
                .handle_mouse(&ev, &ctx, screen);
            self.apply_outcome(id, outcome);
            return;
        }
        let ws_id = c.workspace;
        let (x, y) = (ev.column, ev.row);
        let left_down = matches!(ev.kind, MouseEventKind::Down(MouseButton::Left));

        let pane_area = compositor::pane_area(screen);
        let (vw, vh) = self.sizes.get(&ws_id).copied().unwrap_or(DEFAULT_SIZE);
        let area = Rect::new(
            pane_area.x,
            pane_area.y,
            vw.min(pane_area.width),
            vh.min(pane_area.height),
        );
        let Some(ws) = self.workspace(ws_id) else {
            return;
        };
        let active_pane = ws.active().active_pane;
        let (rects, seps) = tab_rects(ws.active(), area);
        let hit = rects.iter().find(|(_, r)| ui::contains(*r, x, y)).copied();

        // Continue an ongoing drag first: it owns the mouse until release.
        if self.continue_drag(id, ws_id, &ev, active_pane, area, &rects) {
            return;
        }
        // Bars (after drags, so a drag released over a bar still ends
        // properly).
        if self.on_bars(id, ws_id, &ev, screen) {
            return;
        }
        // Grab a border to resize.
        if left_down && self.grab_border(id, &seps, &rects, x, y) {
            return;
        }

        let Some((pane_id, rect)) = hit else { return };
        self.on_pane_mouse(id, ws_id, &ev, active_pane, pane_id, rect);
    }

    /// Clicks and wheel on the menu bar and status bar; true if `ev` was on
    /// one of them.
    fn on_bars(&mut self, id: ClientId, ws_id: WorkspaceId, ev: &MouseEvent, screen: Rect) -> bool {
        let (x, y) = (ev.column, ev.row);
        let left_down = matches!(ev.kind, MouseEventKind::Down(MouseButton::Left));
        // Menu bar:
        if y == 0 {
            if left_down {
                let menus = ui::menu::menus();
                if let Some(i) = ui::menu::title_positions(&menus)
                    .iter()
                    .position(|&(tx, w)| x >= tx && x < tx + w)
                {
                    self.push_overlay(id, |_| Overlay::Menu(ui::menu::MenuBar::new(i)));
                }
            }
            return true;
        }
        // Status bar: click a tab, wheel through tabs.
        if y == screen.bottom().saturating_sub(1) {
            match ev.kind {
                MouseEventKind::Down(MouseButton::Left) => {
                    let hit = self.workspace(ws_id).and_then(|w| {
                        compositor::tab_spans(w, &self.panes)
                            .iter()
                            .position(|&(tx, tw, _)| x >= tx && x < tx + tw)
                    });
                    if let Some(i) = hit {
                        self.dispatch(id, Action::GoToTab { index: i + 1 });
                    }
                }
                MouseEventKind::ScrollUp => self.dispatch(id, Action::PrevTab),
                MouseEventKind::ScrollDown => self.dispatch(id, Action::NextTab),
                _ => {}
            }
            return true;
        }
        false
    }

    /// Starts a border drag if `(x, y)` is on a separator; true if it was.
    fn grab_border(
        &mut self,
        id: ClientId,
        seps: &[Separator],
        rects: &[(PaneId, Rect)],
        x: u16,
        y: u16,
    ) -> bool {
        let Some(sep) = seps.iter().find(|s| {
            if s.vertical {
                x == s.x && y >= s.y && y < s.y + s.len
            } else {
                y == s.y && x >= s.x && x < s.x + s.len
            }
        }) else {
            return false;
        };
        // The pane just before the border (left of it / above it) moves it.
        let probe = if sep.vertical {
            (sep.x.saturating_sub(1), y)
        } else {
            (x, sep.y.saturating_sub(1))
        };
        if let Some((pane, _)) = rects
            .iter()
            .find(|(_, r)| ui::contains(*r, probe.0, probe.1))
        {
            let last = if sep.vertical { x } else { y };
            self.clients.get_mut(&id).unwrap().drag = Some(Drag::Border {
                pane: *pane,
                vertical: sep.vertical,
                last,
            });
        }
        true
    }
}
