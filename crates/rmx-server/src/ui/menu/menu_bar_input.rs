//! Menu bar keyboard (arrows, accelerators) and mouse (hover, click) input.

use crossterm::event::{KeyCode, KeyEvent, MouseButton, MouseEvent, MouseEventKind};

use super::{Item, MenuBar, menus, title_positions};
use crate::ui::{Ctx, Outcome, contains};

impl MenuBar {
    pub fn handle_key(&mut self, key: &KeyEvent, _ctx: &Ctx) -> Outcome {
        match key.code {
            KeyCode::Esc | KeyCode::F(10) => Outcome::CLOSE,
            KeyCode::Up => {
                self.step(-1);
                Outcome::STAY
            }
            KeyCode::Down => {
                self.step(1);
                Outcome::STAY
            }
            KeyCode::Left => {
                self.switch(-1);
                Outcome::STAY
            }
            KeyCode::Right | KeyCode::Tab => {
                self.switch(1);
                Outcome::STAY
            }
            KeyCode::Enter | KeyCode::Char(' ') => match &menus()[self.open].items[self.selected] {
                Item::Entry(_, action) => Outcome::run(action.clone()),
                Item::Separator => Outcome::STAY,
            },
            // Accelerators: a letter jumps to the menu or item starting with it.
            KeyCode::Char(c) => {
                let c = c.to_ascii_lowercase();
                let menus = menus();
                let hit = menus[self.open].items.iter().position(|i| {
                    matches!(i, Item::Entry(label, _) if label.to_ascii_lowercase().starts_with(c))
                });
                match hit {
                    Some(i) => match &menus[self.open].items[i] {
                        Item::Entry(_, action) => Outcome::run(action.clone()),
                        Item::Separator => Outcome::STAY,
                    },
                    None => {
                        if let Some(m) = menus
                            .iter()
                            .position(|m| m.title.to_ascii_lowercase().starts_with(c))
                        {
                            self.open = m;
                            self.selected = self.first_entry();
                        }
                        Outcome::STAY
                    }
                }
            }
            _ => Outcome::STAY,
        }
    }

    pub fn handle_mouse(&mut self, ev: &MouseEvent, ctx: &Ctx) -> Outcome {
        let (x, y) = (ev.column, ev.row);
        let menus = menus();
        let titles = title_positions(&menus);
        match ev.kind {
            MouseEventKind::Down(MouseButton::Left)
            | MouseEventKind::Moved
            | MouseEventKind::Drag(_) => {
                if y == 0 {
                    if let Some(i) = titles.iter().position(|&(tx, w)| x >= tx && x < tx + w) {
                        if i == self.open && matches!(ev.kind, MouseEventKind::Down(_)) {
                            return Outcome::CLOSE;
                        }
                        self.open = i;
                        self.selected = self.first_entry();
                    } else if matches!(ev.kind, MouseEventKind::Down(_)) {
                        return Outcome::CLOSE;
                    }
                    return Outcome::STAY;
                }
                let rect = self.dropdown_rect(ctx);
                if contains(rect, x, y) {
                    let idx = if y > rect.y {
                        usize::from(y - rect.y - 1)
                    } else {
                        usize::MAX
                    };
                    if idx < menus[self.open].items.len()
                        && matches!(menus[self.open].items[idx], Item::Entry(..))
                    {
                        self.selected = idx;
                    }
                    Outcome::STAY
                } else if matches!(ev.kind, MouseEventKind::Down(_)) {
                    Outcome::CLOSE
                } else {
                    Outcome::STAY
                }
            }
            MouseEventKind::Up(MouseButton::Left) => {
                let rect = self.dropdown_rect(ctx);
                if contains(rect, x, y) {
                    let idx = if y > rect.y {
                        usize::from(y - rect.y - 1)
                    } else {
                        usize::MAX
                    };
                    if let Some(Item::Entry(_, action)) = menus[self.open].items.get(idx) {
                        return Outcome::run(action.clone());
                    }
                }
                Outcome::STAY
            }
            _ => Outcome::STAY,
        }
    }
}
