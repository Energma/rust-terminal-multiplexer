//! Palette keyboard, paste and mouse handling.

use crossterm::event::{KeyCode, KeyEvent, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;

use super::Palette;
use crate::ui::{Ctx, Outcome, contains, list_scroll};

impl Palette {
    pub fn paste(&mut self, text: &str) {
        self.input.insert_str(text);
        self.selected = 0;
    }

    pub fn handle_key(&mut self, key: &KeyEvent, ctx: &Ctx) -> Outcome {
        let count = self.entries(ctx).len();
        match key.code {
            KeyCode::Esc => return Outcome::CLOSE,
            KeyCode::Enter => return self.choose(ctx, self.selected),
            KeyCode::Up => self.selected = self.selected.saturating_sub(1),
            KeyCode::Down | KeyCode::Tab => {
                self.selected = (self.selected + 1).min(count.saturating_sub(1))
            }
            KeyCode::PageUp => self.selected = self.selected.saturating_sub(10),
            KeyCode::PageDown => self.selected = (self.selected + 10).min(count.saturating_sub(1)),
            _ => {
                if self.input.handle(key) {
                    self.selected = 0;
                }
            }
        }
        Outcome::STAY
    }

    pub fn handle_mouse(&mut self, ev: &MouseEvent, ctx: &Ctx, screen: Rect) -> Outcome {
        let rect = Self::rect(screen);
        // Same geometry as `render`: border, input row, divider, then the list.
        let list = Rect::new(
            rect.x + 1,
            rect.y + 3,
            rect.width.saturating_sub(2),
            rect.height.saturating_sub(4),
        );
        let count = self.entries(ctx).len();
        let scroll = list_scroll(
            self.selected.min(count.saturating_sub(1)),
            usize::from(list.height),
        );
        match ev.kind {
            MouseEventKind::Down(MouseButton::Left) if !contains(rect, ev.column, ev.row) => {
                Outcome::CLOSE
            }
            MouseEventKind::Up(MouseButton::Left) if contains(list, ev.column, ev.row) => {
                self.choose(ctx, scroll + usize::from(ev.row - list.y))
            }
            MouseEventKind::ScrollDown => {
                self.selected = (self.selected + 1).min(count.saturating_sub(1));
                Outcome::STAY
            }
            MouseEventKind::ScrollUp => {
                self.selected = self.selected.saturating_sub(1);
                Outcome::STAY
            }
            _ => Outcome::STAY,
        }
    }
}
