//! The selection itself: starting/toggling it, its ordered range and the
//! text it covers.

use rmx_core::emulator::Emulator;

use super::{Pos, Select};

impl Select {
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
}
