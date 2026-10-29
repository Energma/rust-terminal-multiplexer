//! The single-line text field behind prompts, the palette and filters.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::buffer::Buffer;
use ratatui::style::Style;

use super::put;

/// Single-line text editing shared by prompts, the palette and filters.
#[derive(Debug, Default, Clone)]
pub struct TextInput {
    pub text: String,
    /// Cursor position in chars.
    pub cursor: usize,
}

impl TextInput {
    pub fn new(text: &str) -> Self {
        Self {
            text: text.to_owned(),
            cursor: text.chars().count(),
        }
    }

    /// Inserts text at the cursor, dropping line breaks and control chars.
    pub fn insert_str(&mut self, text: &str) {
        for c in text.chars().filter(|c| !c.is_control()) {
            let at = self
                .text
                .char_indices()
                .nth(self.cursor)
                .map_or(self.text.len(), |(i, _)| i);
            self.text.insert(at, c);
            self.cursor += 1;
        }
    }

    /// Returns true if the key edited the text or moved the cursor.
    pub fn handle(&mut self, key: &KeyEvent) -> bool {
        let byte = |s: &str, c: usize| s.char_indices().nth(c).map_or(s.len(), |(i, _)| i);
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        match key.code {
            KeyCode::Char('u') if ctrl => {
                self.text.clear();
                self.cursor = 0;
            }
            KeyCode::Char('w') if ctrl => {
                let before: String = self.text.chars().take(self.cursor).collect();
                let trimmed = before.trim_end();
                let keep = trimmed.rfind(' ').map_or(0, |i| i + 1);
                let removed = before.chars().count() - trimmed[..keep].chars().count();
                let start = byte(&self.text, self.cursor - removed);
                let end = byte(&self.text, self.cursor);
                self.text.replace_range(start..end, "");
                self.cursor -= removed;
            }
            KeyCode::Char(c) if !ctrl => {
                let at = byte(&self.text, self.cursor);
                self.text.insert(at, c);
                self.cursor += 1;
            }
            KeyCode::Backspace if self.cursor > 0 => {
                let at = byte(&self.text, self.cursor - 1);
                self.text.remove(at);
                self.cursor -= 1;
            }
            KeyCode::Delete if self.cursor < self.text.chars().count() => {
                let at = byte(&self.text, self.cursor);
                self.text.remove(at);
            }
            KeyCode::Left => self.cursor = self.cursor.saturating_sub(1),
            KeyCode::Right => self.cursor = (self.cursor + 1).min(self.text.chars().count()),
            KeyCode::Home => self.cursor = 0,
            KeyCode::End => self.cursor = self.text.chars().count(),
            _ => return false,
        }
        true
    }

    /// Draws the text with a block cursor.
    pub fn render(&self, buf: &mut Buffer, x: u16, y: u16, width: u16, style: Style) {
        if width == 0 {
            return;
        }
        let chars: Vec<char> = self.text.chars().collect();
        // Scroll horizontally so the cursor stays visible.
        let start = (self.cursor + 1).saturating_sub(usize::from(width));
        let visible: String = chars.iter().skip(start).take(usize::from(width)).collect();
        put(buf, x, y, &visible, width, style);
        let cx = x + (self.cursor - start) as u16;
        if cx < x + width
            && let Some(cell) = buf.cell_mut((cx, y))
        {
            cell.set_style(style.add_modifier(ratatui::style::Modifier::REVERSED));
        }
    }
}
