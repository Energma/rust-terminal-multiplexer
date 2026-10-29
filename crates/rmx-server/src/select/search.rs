//! Find in select mode: wrap-around, case-insensitive search through the
//! screen and history.

use rmx_core::emulator::Emulator;

use super::Select;

impl Select {
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
}

pub(super) fn match_columns(chars: &[char], query: &str) -> Vec<usize> {
    let q: Vec<char> = query.chars().collect();
    if q.is_empty() || q.len() > chars.len() {
        return Vec::new();
    }
    (0..=chars.len() - q.len())
        .filter(|&i| chars[i..i + q.len()] == q[..])
        .collect()
}
