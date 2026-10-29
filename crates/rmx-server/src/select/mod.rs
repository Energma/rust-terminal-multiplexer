//! Select mode (tmux copy-mode): move a cursor through the screen and
//! history, select text, search, copy.
//!
//! Positions are `(line, col)` with `line` counted from the top of the
//! history, so they stay valid while the view scrolls.

mod motion;
mod render;
mod search;
mod selection;

#[cfg(test)]
mod tests;

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
}
