//! Manager keyboard handling: the filter field, navigation, folding and
//! opening. Keys that operate on sessions are handed to [`Manager::operate`].

use crossterm::event::{KeyCode, KeyEvent};

use super::{Manager, Row};
use crate::ui::{Ctx, Outcome, TextInput};

impl Manager {
    pub fn handle_key(&mut self, key: &KeyEvent, ctx: &Ctx) -> Outcome {
        if let Some(filter) = &mut self.filter {
            match key.code {
                KeyCode::Esc => {
                    self.filter = None;
                    return Outcome::STAY;
                }
                KeyCode::Enter | KeyCode::Up | KeyCode::Down => {}
                _ => {
                    filter.handle(key);
                    return Outcome::STAY;
                }
            }
        }
        let rows = self.rows(ctx);
        if rows.is_empty() {
            return if key.code == KeyCode::Esc {
                Outcome::CLOSE
            } else {
                Outcome::STAY
            };
        }
        let i = self.index(&rows, ctx);
        let row = rows[i];
        // On the "new workspace here" row only opening, `n`, navigation and
        // filtering make sense.
        if row == Row::NewHere {
            match key.code {
                KeyCode::Enter | KeyCode::Right | KeyCode::Char('l') => return self.open(row, ctx),
                KeyCode::Up
                | KeyCode::Down
                | KeyCode::Home
                | KeyCode::End
                | KeyCode::PageUp
                | KeyCode::PageDown
                | KeyCode::Esc
                | KeyCode::Char('k' | 'j' | 'g' | 'G' | 'q' | 'n' | '/' | '1'..='9') => {}
                _ => return Outcome::STAY,
            }
        }
        // 1-9 open that workspace directly, the fastest way to switch.
        if let KeyCode::Char(c @ '1'..='9') = key.code
            && let Some(ws) = ctx.workspaces.get(usize::from(c as u8 - b'1'))
        {
            return self.open(Row::Workspace(ws.id), ctx);
        }
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => return Outcome::CLOSE,
            KeyCode::Up | KeyCode::Char('k') => self.select(&rows, i.saturating_sub(1)),
            KeyCode::Down | KeyCode::Char('j') => self.select(&rows, (i + 1).min(rows.len() - 1)),
            KeyCode::Home | KeyCode::Char('g') => self.select(&rows, 0),
            KeyCode::End | KeyCode::Char('G') => self.select(&rows, rows.len() - 1),
            KeyCode::PageUp => self.select(&rows, i.saturating_sub(10)),
            KeyCode::PageDown => self.select(&rows, (i + 10).min(rows.len() - 1)),
            KeyCode::Right | KeyCode::Char('l') => match row {
                Row::Workspace(w) if !self.expanded_ws.contains(&w) => {
                    self.expanded_ws.insert(w);
                }
                Row::Tab(w, t) if !self.expanded_tabs.contains(&(w, t)) => {
                    self.expanded_tabs.insert((w, t));
                }
                _ => self.select(&rows, (i + 1).min(rows.len() - 1)),
            },
            KeyCode::Left | KeyCode::Char('h') => match row {
                Row::NewHere => {}
                Row::Workspace(w) => {
                    self.expanded_ws.remove(&w);
                }
                Row::Tab(w, t) if self.expanded_tabs.contains(&(w, t)) => {
                    self.expanded_tabs.remove(&(w, t));
                }
                Row::Tab(w, _) => {
                    self.expanded_ws.remove(&w);
                    self.selected = Some(Row::Workspace(w));
                }
                Row::Pane(w, t, _) => {
                    self.expanded_tabs.remove(&(w, t));
                    self.selected = Some(Row::Tab(w, t));
                }
            },
            KeyCode::Char(' ') => match row {
                Row::NewHere => {}
                Row::Workspace(w) => {
                    if !self.expanded_ws.remove(&w) {
                        self.expanded_ws.insert(w);
                    }
                }
                Row::Tab(w, t) => {
                    if !self.expanded_tabs.remove(&(w, t)) {
                        self.expanded_tabs.insert((w, t));
                    }
                }
                Row::Pane(..) => {}
            },
            KeyCode::Enter => return self.open(row, ctx),
            KeyCode::Char('/') => self.filter = Some(TextInput::default()),
            _ => return self.operate(key, row, ctx),
        }
        Outcome::STAY
    }
}
