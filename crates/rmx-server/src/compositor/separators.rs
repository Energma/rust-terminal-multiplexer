//! Pane borders: junction-aware box-drawing between panes.

use std::collections::HashSet;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use rmx_config::Theme;
use rmx_core::layout::{PaneId, Separator};

/// Draws pane borders with proper junction glyphs; borders touching the
/// focused pane use the active colour.
pub fn draw_separators(
    buf: &mut Buffer,
    area: Rect,
    seps: &[Separator],
    rects: &[(PaneId, Rect)],
    focus: PaneId,
    theme: &Theme,
) {
    let mut cells: HashSet<(u16, u16)> = HashSet::new();
    for s in seps {
        for i in 0..s.len {
            cells.insert(if s.vertical {
                (s.x, s.y + i)
            } else {
                (s.x + i, s.y)
            });
        }
    }
    let focus_rect = rects.iter().find(|(p, _)| *p == focus).map(|(_, r)| *r);
    let near_focus = |x: u16, y: u16| {
        focus_rect
            .is_some_and(|r| x + 1 >= r.x && x <= r.right() && y + 1 >= r.y && y <= r.bottom())
    };
    for &(x, y) in &cells {
        let up = y > area.y && cells.contains(&(x, y - 1));
        let down = cells.contains(&(x, y + 1));
        let left = x > area.x && cells.contains(&(x - 1, y));
        let right = cells.contains(&(x + 1, y));
        let glyph = match (up, down, left, right) {
            (true, true, true, true) => "┼",
            (true, true, false, true) => "├",
            (true, true, true, false) => "┤",
            (false, true, true, true) => "┬",
            (true, false, true, true) => "┴",
            (_, _, true, _) | (_, _, _, true) if !(up || down) => "─",
            (true, _, _, _) | (_, true, _, _) if !(left || right) => "│",
            _ => "┼",
        };
        let color = if near_focus(x, y) {
            theme.active_border
        } else {
            theme.border
        };
        if let Some(c) = buf.cell_mut((x, y)) {
            c.reset();
            c.set_symbol(glyph).set_style(Style::default().fg(color));
        }
    }
}
