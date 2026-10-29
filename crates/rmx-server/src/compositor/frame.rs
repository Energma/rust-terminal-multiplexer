//! The per-client compositor: keeps the last frame and turns each new one
//! into a diff (or a full frame after a resize).

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use rmx_proto::{Frame, WireCell};

use super::View;
use super::screen::draw;

pub struct Compositor {
    prev: Buffer,
    next: Buffer,
    /// Set until the client has received a full frame at the current size.
    needs_full: bool,
}

impl Compositor {
    pub fn new(cols: u16, rows: u16) -> Self {
        let area = Rect::new(0, 0, cols, rows);
        Self {
            prev: Buffer::empty(area),
            next: Buffer::empty(area),
            needs_full: true,
        }
    }

    pub fn resize(&mut self, cols: u16, rows: u16) {
        *self = Self::new(cols, rows);
    }

    pub fn area(&self) -> Rect {
        self.next.area
    }

    pub fn compose(&mut self, view: &View) -> Frame {
        let screen = self.next.area;
        self.next.reset();
        let cursor = draw(&mut self.next, screen, view);

        let full = std::mem::take(&mut self.needs_full);
        if full {
            // A full frame is a diff against the blank buffer the client
            // starts from, so default cells needn't be sent.
            self.prev = Buffer::empty(screen);
        }
        let cells = self
            .prev
            .diff(&self.next)
            .into_iter()
            .map(|(x, y, c)| (x, y, WireCell::from(c)))
            .collect();
        std::mem::swap(&mut self.prev, &mut self.next);

        let tab = view.workspace.active();
        let title = format!(
            "{} · {}",
            crate::tab_title(tab, view.ctx.panes),
            view.workspace.name
        );
        Frame {
            full,
            cols: screen.width,
            rows: screen.height,
            cells,
            cursor,
            title,
        }
    }
}
