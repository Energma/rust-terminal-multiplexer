//! Manager screen geometry and mouse input (scrolling, click, double-click).

use std::time::{Duration, Instant};

use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;

use super::Manager;
use crate::ui::{Ctx, Outcome, contains, list_scroll};

impl Manager {
    /// `(outer, list, preview, footer)` rects; shared by drawing and
    /// hit-testing.
    pub(super) fn layout(screen: Rect) -> (Rect, Rect, Rect, Rect) {
        let outer = Rect::new(
            screen.x + 1,
            screen.y + 1,
            screen.width.saturating_sub(2),
            screen.height.saturating_sub(2),
        );
        let inner = Rect::new(
            outer.x + 1,
            outer.y + 1,
            outer.width.saturating_sub(2),
            outer.height.saturating_sub(2),
        );
        // Narrow screens get the list alone; the preview needs room to help.
        let list_w = if inner.width < 60 {
            inner.width
        } else {
            (inner.width * 2 / 5).clamp(24, 48)
        };
        let body_h = inner.height.saturating_sub(2);
        let list = Rect::new(inner.x, inner.y + 1, list_w, body_h.saturating_sub(1));
        let preview = Rect::new(
            inner.x + list_w + 1,
            inner.y,
            inner.width.saturating_sub(list_w + 1),
            body_h,
        );
        let footer = Rect::new(inner.x, inner.bottom().saturating_sub(1), inner.width, 1);
        (outer, list, preview, footer)
    }

    pub fn handle_mouse(&mut self, ev: &MouseEvent, ctx: &Ctx, screen: Rect) -> Outcome {
        let (_, list, _, _) = Self::layout(screen);
        let rows = self.rows(ctx);
        let i = self.index(&rows, ctx);
        match ev.kind {
            MouseEventKind::ScrollUp => self.select(&rows, i.saturating_sub(1)),
            MouseEventKind::ScrollDown => {
                self.select(&rows, (i + 1).min(rows.len().saturating_sub(1)))
            }
            MouseEventKind::Down(MouseButton::Left) if contains(list, ev.column, ev.row) => {
                let sel = self.index(&rows, ctx);
                let idx = list_scroll(sel, usize::from(list.height)) + usize::from(ev.row - list.y);
                if let Some(&row) = rows.get(idx) {
                    let double = self.last_click.is_some_and(|(t, y)| {
                        y == ev.row && t.elapsed() < Duration::from_millis(400)
                    });
                    self.selected = Some(row);
                    self.last_click = Some((Instant::now(), ev.row));
                    if double {
                        return self.open(row, ctx);
                    }
                }
            }
            _ => {}
        }
        Outcome::STAY
    }
}
