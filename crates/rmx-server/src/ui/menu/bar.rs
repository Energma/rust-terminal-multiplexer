//! The always-visible menu bar row: title layout and drawing.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};

use super::{Menu, menus};
use crate::ui::{Ctx, put};

/// Column and width of each menu title in the bar; shared by drawing and
/// mouse hit-testing so they can't disagree.
pub fn title_positions(menus: &[Menu]) -> Vec<(u16, u16)> {
    let mut x = 7; // after the " ≡ rmx " badge
    menus
        .iter()
        .map(|m| {
            let w = m.title.len() as u16 + 2;
            let pos = (x, w);
            x += w;
            pos
        })
        .collect()
}

/// Draws the menu bar row; `open` highlights the active title.
pub fn render_bar(buf: &mut Buffer, row: Rect, ctx: &Ctx, open: Option<usize>, workspace: &str) {
    let theme = ctx.theme;
    let base = Style::default().fg(theme.status_fg).bg(theme.status_bg);
    buf.set_style(row, base);
    put(
        buf,
        row.x,
        row.y,
        " ≡ rmx",
        6,
        base.fg(theme.accent).add_modifier(Modifier::BOLD),
    );
    let menus = menus();
    for (i, (m, (x, w))) in menus.iter().zip(title_positions(&menus)).enumerate() {
        let style = if open == Some(i) {
            Style::default()
                .fg(theme.menu_highlight_fg)
                .bg(theme.menu_highlight_bg)
        } else {
            base
        };
        put(buf, row.x + x, row.y, &format!(" {} ", m.title), w, style);
        // Underline the accelerator letter, as Windows does.
        if let Some(cell) = buf.cell_mut((row.x + x + 1, row.y)) {
            cell.set_style(style.add_modifier(Modifier::UNDERLINED));
        }
    }
    let right = format!(" {workspace} ");
    let rw = right.chars().count() as u16;
    if rw + 60 < row.width {
        put(
            buf,
            row.right() - rw,
            row.y,
            &right,
            rw,
            base.fg(theme.accent),
        );
    }
}
