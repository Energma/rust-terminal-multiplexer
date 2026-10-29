//! Cursor movement in select mode (by cell, word, line, page, top/bottom),
//! keeping the cursor line scrolled into view.

use rmx_config::action::Dir;
use rmx_core::emulator::Emulator;

use super::{Select, view_top};

impl Select {
    /// Scrolls the view just enough to show the cursor line.
    pub fn ensure_visible(&self, emu: &mut dyn Emulator) {
        let top = view_top(emu);
        let rows = usize::from(emu.rows());
        let line = self.cursor.0;
        let history = emu.history_len();
        if line < top {
            emu.set_view_offset(history - line);
        } else if line >= top + rows {
            emu.set_view_offset(history.saturating_sub(line + 1 - rows));
        }
    }

    pub fn move_cursor(&mut self, dir: Dir, cols: u16, emu: &mut dyn Emulator) {
        let last = Self::last_line(emu);
        let (line, col) = &mut self.cursor;
        match dir {
            Dir::Up => *line = line.saturating_sub(1),
            Dir::Down => *line = (*line + 1).min(last),
            Dir::Left => *col = col.saturating_sub(1),
            Dir::Right => *col = (*col + 1).min(cols.saturating_sub(1)),
        }
        self.ensure_visible(emu);
    }

    pub fn move_word(&mut self, forward: bool, emu: &mut dyn Emulator) {
        let text: Vec<char> = emu.line_text(self.cursor.0).chars().collect();
        let mut c = usize::from(self.cursor.1);
        let is_word = |i: usize| text.get(i).is_some_and(|ch| !ch.is_whitespace());
        if forward {
            while c < text.len() && is_word(c) {
                c += 1;
            }
            while c < text.len() && !is_word(c) {
                c += 1;
            }
        } else {
            c = c.saturating_sub(1);
            while c > 0 && !is_word(c) {
                c -= 1;
            }
            while c > 0 && is_word(c - 1) {
                c -= 1;
            }
        }
        self.cursor.1 = c as u16;
    }

    pub fn line_start(&mut self) {
        self.cursor.1 = 0;
    }

    /// Last non-blank column, like End in an editor.
    pub fn line_end(&mut self, emu: &mut dyn Emulator) {
        let len = emu.line_text(self.cursor.0).trim_end().chars().count();
        self.cursor.1 = len.saturating_sub(1) as u16;
    }

    /// Shift+arrow: start selecting here if nothing is selected, then move.
    pub fn extend(&mut self, dir: Dir, cols: u16, emu: &mut dyn Emulator) {
        if self.anchor.is_none() {
            self.anchor = Some(self.cursor);
            self.line_mode = false;
        }
        self.move_cursor(dir, cols, emu);
    }

    pub fn scroll(&mut self, delta: isize, emu: &mut dyn Emulator) {
        let last = Self::last_line(emu);
        self.cursor.0 = self.cursor.0.saturating_add_signed(delta).min(last);
        self.ensure_visible(emu);
    }

    pub fn go_top(&mut self, emu: &mut dyn Emulator) {
        self.cursor = (0, 0);
        self.ensure_visible(emu);
    }

    pub fn go_bottom(&mut self, emu: &mut dyn Emulator) {
        self.cursor = (Self::last_line(emu), 0);
        self.ensure_visible(emu);
    }
}
