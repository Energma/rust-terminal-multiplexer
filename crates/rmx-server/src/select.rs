//! Select mode (tmux copy-mode): move a cursor through the screen and
//! history, select text, search, copy.
//!
//! Positions are `(line, col)` with `line` counted from the top of the
//! history, so they stay valid while the view scrolls.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier};
use rmx_config::Theme;
use rmx_config::action::Dir;
use rmx_core::emulator::Emulator;
use rmx_core::layout::PaneId;

pub type Pos = (usize, u16);

pub struct Select {
    pub pane: PaneId,
    pub cursor: Pos,
    pub anchor: Option<Pos>,
    pub line_mode: bool,
    pub search: Option<String>,
    /// Started by a mouse drag: copy and leave on release.
    pub by_mouse: bool,
    /// Feedback for the badge, e.g. "no match".
    pub note: Option<String>,
    /// `Emulator::lines_dropped` when positions were last adjusted.
    dropped_at: u64,
}

/// The first line currently on screen.
pub fn view_top(emu: &dyn Emulator) -> usize {
    emu.history_len().saturating_sub(emu.view_offset())
}

impl Select {
    pub fn new(pane: PaneId, emu: &dyn Emulator) -> Self {
        let (row, col) = emu.cursor().unwrap_or((emu.rows().saturating_sub(1), 0));
        Self {
            pane,
            cursor: (view_top(emu) + usize::from(row), col),
            anchor: None,
            line_mode: false,
            search: None,
            by_mouse: false,
            note: None,
            dropped_at: emu.lines_dropped(),
        }
    }

    pub fn at(pane: PaneId, emu: &dyn Emulator, row: u16, col: u16) -> Self {
        let mut s = Self::new(pane, emu);
        s.cursor = (view_top(emu) + usize::from(row), col);
        s
    }

    /// Keeps positions on the same text after lines fell off the top of a
    /// full history (output kept streaming while selecting).
    pub fn rebase(&mut self, emu: &dyn Emulator) {
        let dropped = emu.lines_dropped();
        let shift = dropped.saturating_sub(self.dropped_at) as usize;
        self.dropped_at = dropped;
        if shift > 0 {
            self.cursor.0 = self.cursor.0.saturating_sub(shift);
            if let Some(a) = &mut self.anchor {
                a.0 = a.0.saturating_sub(shift);
            }
        }
    }

    fn last_line(emu: &dyn Emulator) -> usize {
        emu.history_len() + usize::from(emu.rows()) - 1
    }

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

    /// Starts a selection; pressing the same kind again cancels it, and the
    /// other kind switches between char and line selection (as in vim).
    pub fn start(&mut self, line_mode: bool) {
        match self.anchor {
            Some(_) if self.line_mode == line_mode => self.anchor = None,
            Some(_) => self.line_mode = line_mode,
            None => {
                self.anchor = Some(self.cursor);
                self.line_mode = line_mode;
            }
        }
    }

    /// The selection as an ordered `(start, end)`, end inclusive.
    pub fn range(&self) -> Option<(Pos, Pos)> {
        let a = self.anchor?;
        let (mut s, mut e) = if a <= self.cursor {
            (a, self.cursor)
        } else {
            (self.cursor, a)
        };
        if self.line_mode {
            s.1 = 0;
            e.1 = u16::MAX;
        }
        Some((s, e))
    }

    pub fn has_selection(&self) -> bool {
        self.anchor.is_some()
    }

    /// The selected text; lines are trimmed of trailing blanks like terminals do.
    pub fn text(&self, emu: &mut dyn Emulator) -> Option<String> {
        let ((sl, sc), (el, ec)) = self.range()?;
        let mut out = Vec::new();
        for line in sl..=el {
            let chars: Vec<char> = emu.line_text(line).chars().collect();
            let from = if line == sl { usize::from(sc) } else { 0 };
            let to = if line == el {
                usize::from(ec).saturating_add(1)
            } else {
                usize::MAX
            };
            let piece: String = chars
                .iter()
                .skip(from)
                .take(to.saturating_sub(from))
                .collect();
            out.push(piece.trim_end().to_owned());
        }
        Some(out.join("\n"))
    }

    /// Moves to the next match of `query` (case-insensitive) from the
    /// cursor, wrapping around. Returns false if there is none.
    pub fn find(&mut self, query: &str, forward: bool, emu: &mut dyn Emulator) -> bool {
        self.search = Some(query.to_owned());
        let q = query.to_lowercase();
        let total = Self::last_line(emu) + 1;
        let (start_line, start_col) = self.cursor;
        for step in 0..=total {
            let line = if forward {
                (start_line + step) % total
            } else {
                (start_line + total - step % total) % total
            };
            let text = emu.line_text(line).to_lowercase();
            let chars: Vec<char> = text.chars().collect();
            let hits = match_columns(&chars, &q);
            let pick = if forward {
                hits.into_iter()
                    .find(|&c| step > 0 || c > usize::from(start_col))
            } else {
                hits.into_iter()
                    .rev()
                    .find(|&c| step > 0 || c < usize::from(start_col))
            };
            if let Some(col) = pick {
                self.cursor = (line, col as u16);
                self.ensure_visible(emu);
                self.note = None;
                return true;
            }
        }
        self.note = Some(format!("no match for \"{query}\""));
        false
    }

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

fn match_columns(chars: &[char], query: &str) -> Vec<usize> {
    let q: Vec<char> = query.chars().collect();
    if q.is_empty() || q.len() > chars.len() {
        return Vec::new();
    }
    (0..=chars.len() - q.len())
        .filter(|&i| chars[i..i + q.len()] == q[..])
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use rmx_core::emulator::Vt100Emulator;

    fn emu_with(lines: &[&str], rows: u16) -> Vt100Emulator {
        let mut emu = Vt100Emulator::new(rows, 20, 100);
        emu.process(lines.join("\r\n").as_bytes());
        emu
    }

    #[test]
    fn selects_across_lines_including_history() {
        let mut emu = emu_with(&["alpha one", "beta two", "gamma three", "delta"], 2);
        let mut s = Select::new(0, &emu);
        s.cursor = (0, 6);
        s.start(false);
        s.cursor = (2, 4);
        assert_eq!(s.text(&mut emu).unwrap(), "one\nbeta two\ngamma");
        s.start(true);
        assert_eq!(
            s.text(&mut emu).unwrap(),
            "alpha one\nbeta two\ngamma three"
        );
    }

    #[test]
    fn find_wraps_and_scrolls_into_view() {
        let mut emu = emu_with(&["needle a", "x", "y", "z", "needle b"], 2);
        let mut s = Select::new(0, &emu);
        assert_eq!(s.cursor, (4, 8), "starts at the shell cursor");
        // Backwards (upwards) first finds the match left of the cursor…
        assert!(s.find("NEEDLE", false, &mut emu));
        assert_eq!(s.cursor, (4, 0));
        // …then the one in history.
        assert!(s.find("needle", false, &mut emu));
        assert_eq!(s.cursor, (0, 0));
        assert_eq!(emu.view_offset(), 3, "scrolled back to show line 0");
        assert!(s.find("needle", true, &mut emu));
        assert_eq!(s.cursor, (4, 0));
        assert!(!s.find("absent", true, &mut emu));
        assert!(s.status().unwrap().contains("no match"));
    }

    #[test]
    fn selection_follows_text_when_history_is_full() {
        let mut emu = Vt100Emulator::new(2, 10, 3);
        emu.process(b"a\r\nb\r\nc\r\nd\r\ne");
        let mut s = Select::new(0, &emu);
        s.cursor = (1, 0); // "c"
        s.start(false);
        emu.process(b"\r\nf\r\ng");
        s.rebase(&emu);
        assert_eq!(emu.line_text(s.cursor.0), "c");
        assert_eq!(s.text(&mut emu).unwrap(), "c");
    }

    #[test]
    fn word_motion() {
        let mut emu = emu_with(&["foo bar  baz"], 1);
        let mut s = Select::new(0, &emu);
        s.cursor = (0, 0);
        s.move_word(true, &mut emu);
        assert_eq!(s.cursor.1, 4);
        s.move_word(true, &mut emu);
        assert_eq!(s.cursor.1, 9);
        s.move_word(false, &mut emu);
        assert_eq!(s.cursor.1, 4);
    }
}
