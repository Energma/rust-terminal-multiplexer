//! The right-click context menu for a pane.

use crossterm::event::{KeyCode, KeyEvent, MouseButton, MouseEvent, MouseEventKind};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use rmx_config::action::{Action, Dir};

use super::Item;
use super::catalog::split;
use super::items::{render_items, step_entry};
use crate::ui::{Ctx, Outcome, contains};

/// Right-click menu for a pane.
pub struct ContextMenu {
    x: u16,
    y: u16,
    items: Vec<Item>,
    selected: usize,
}

impl ContextMenu {
    pub fn new(x: u16, y: u16, has_selection: bool) -> Self {
        use Item::{Entry, Separator};
        let mut items = vec![
            Entry("Split Right", split(Dir::Right)),
            Entry("Split Down", split(Dir::Down)),
            Entry("Zoom Pane", Action::TogglePaneZoom),
            Separator,
        ];
        if has_selection {
            items.push(Entry("Copy", Action::Copy));
        }
        items.extend([
            Entry("Paste", Action::Paste),
            Entry("Select / Scroll Mode", Action::SelectMode),
            Separator,
            Entry("Rename Tab…", Action::RenameTab),
            Entry("Session Manager…", Action::WorkspaceSwitcher),
            Entry("Close Pane", Action::ClosePane),
        ]);
        Self {
            x,
            y,
            items,
            selected: 0,
        }
    }

    fn rect(&self, screen: Rect) -> Rect {
        // Fits the longest label plus a leader-key hint.
        let w = 36;
        let h = self.items.len() as u16 + 2;
        // Flip towards the inside near the right/bottom edges.
        let x = if self.x + w > screen.right() {
            self.x.saturating_sub(w)
        } else {
            self.x
        };
        let y = if self.y + h > screen.bottom() {
            self.y.saturating_sub(h)
        } else {
            self.y
        };
        Rect::new(x, y, w, h).intersection(screen)
    }

    fn step(&mut self, delta: isize) {
        self.selected = step_entry(&self.items, self.selected, delta);
    }

    pub fn handle_key(&mut self, key: &KeyEvent) -> Outcome {
        match key.code {
            KeyCode::Esc => Outcome::CLOSE,
            KeyCode::Up => {
                self.step(-1);
                Outcome::STAY
            }
            KeyCode::Down => {
                self.step(1);
                Outcome::STAY
            }
            KeyCode::Enter => match &self.items[self.selected] {
                Item::Entry(_, a) => Outcome::run(a.clone()),
                Item::Separator => Outcome::STAY,
            },
            _ => Outcome::STAY,
        }
    }

    pub fn handle_mouse(&mut self, ev: &MouseEvent, screen: Rect) -> Outcome {
        let rect = self.rect(screen);
        let inside = contains(rect, ev.column, ev.row);
        let idx = if ev.row > rect.y {
            usize::from(ev.row - rect.y - 1)
        } else {
            usize::MAX
        };
        match ev.kind {
            MouseEventKind::Moved | MouseEventKind::Drag(_) if inside => {
                if matches!(self.items.get(idx), Some(Item::Entry(..))) {
                    self.selected = idx;
                }
                Outcome::STAY
            }
            MouseEventKind::Up(MouseButton::Left) if inside => match self.items.get(idx) {
                Some(Item::Entry(_, a)) => Outcome::run(a.clone()),
                _ => Outcome::STAY,
            },
            MouseEventKind::Down(_) if !inside => Outcome::CLOSE,
            _ => Outcome::STAY,
        }
    }

    pub fn render(&self, ctx: &Ctx, screen: Rect, buf: &mut Buffer) {
        render_items(buf, self.rect(screen), ctx, &self.items, self.selected, "");
    }
}
