//! How select mode shows itself: the badge text and the highlighting of
//! selection, search matches and cursor over the pane.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier};
use rmx_config::Theme;
use rmx_core::emulator::Emulator;

use super::search::match_columns;
use super::{Select, view_top};

impl Select {
    pub fn status(&self) -> Option<String> {
        if let Some(note) = &self.note {
            return Some(note.clone());
        }
        let label = match (&self.search, self.has_selection()) {
            (_, true) if self.line_mode => "SELECT LINES".to_owned(),
            (_, true) => "SELECTING".to_owned(),
            (Some(q), false) => format!("FIND \"{q}\""),
            (None, false) => return None,
        };
        Some(label)
    }

    /// Highlights selection, search matches and the cursor over the pane.
    pub fn render(&self, emu: &dyn Emulator, rect: Rect, buf: &mut Buffer, theme: &Theme) {
        let top = view_top(emu);
        let range = self.range();
        let query = self.search.as_ref().map(|q| q.to_lowercase());
        for row in 0..rect.height.min(emu.rows()) {
            let line = top + usize::from(row);
            let y = rect.y + row;
            if let Some(q) = &query {
                // Read cells directly: line_text needs &mut and we're drawing.
                let chars: Vec<char> = (0..rect.width)
                    .map(|x| {
                        buf.cell((rect.x + x, y))
                            .and_then(|c| c.symbol().chars().next())
                            .unwrap_or(' ')
                    })
                    .flat_map(char::to_lowercase)
                    .collect();
                for start in match_columns(&chars, q) {
                    for x in start..start + q.chars().count() {
                        if let Some(c) = buf.cell_mut((rect.x + x as u16, y)) {
                            c.set_bg(Color::Yellow).set_fg(Color::Black);
                        }
                    }
                }
            }
            if let Some(((sl, sc), (el, ec))) = range
                && line >= sl
                && line <= el
            {
                let from = if line == sl { sc } else { 0 };
                let to = if line == el {
                    ec.min(rect.width.saturating_sub(1))
                } else {
                    rect.width.saturating_sub(1)
                };
                for x in from..=to.min(rect.width.saturating_sub(1)) {
                    if let Some(c) = buf.cell_mut((rect.x + x, y)) {
                        c.set_bg(theme.menu_highlight_bg)
                            .set_fg(theme.menu_highlight_fg);
                    }
                }
            }
            if line == self.cursor.0
                && self.cursor.1 < rect.width
                && let Some(c) = buf.cell_mut((rect.x + self.cursor.1, y))
            {
                c.modifier.insert(Modifier::REVERSED);
            }
        }
    }
}
