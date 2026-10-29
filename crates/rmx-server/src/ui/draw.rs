//! Drawing and geometry helpers shared by every overlay: centring, boxed
//! panels, clipped text, list scrolling and hit-testing.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::widgets::{Block, BorderType, Clear, Widget};
use rmx_config::Theme;

/// A rect of at most `w`×`h` centred in `area`.
pub fn centered(area: Rect, w: u16, h: u16) -> Rect {
    let w = w.min(area.width);
    let h = h.min(area.height);
    Rect::new(
        area.x + (area.width - w) / 2,
        area.y + (area.height - h) / 2,
        w,
        h,
    )
}

/// Clears `rect` and draws a rounded, titled box; returns the inner area.
pub fn panel(buf: &mut Buffer, rect: Rect, title: &str, theme: &Theme) -> Rect {
    Clear.render(rect, buf);
    let base = Style::default().fg(theme.menu_fg).bg(theme.menu_bg);
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(base.fg(theme.accent))
        .title_style(base.fg(theme.accent))
        .style(base);
    let block = if title.is_empty() {
        block
    } else {
        block.title(format!(" {title} "))
    };
    let inner = block.inner(rect);
    block.render(rect, buf);
    inner
}

/// Writes `text` clipped to `width` cells.
/// Writes `text` clipped to `width` cells and to the buffer, so a too-small
/// screen truncates the UI instead of panicking.
pub fn put(buf: &mut Buffer, x: u16, y: u16, text: &str, width: u16, style: Style) {
    let area = buf.area;
    if y < area.y || y >= area.bottom() || x < area.x || x >= area.right() {
        return;
    }
    let width = width.min(area.right() - x);
    if width > 0 {
        buf.set_stringn(x, y, text, usize::from(width), style);
    }
}

/// First visible row of a list so `selected` stays in view; shared by
/// drawing and mouse hit-testing so clicks land on what's shown.
pub fn list_scroll(selected: usize, visible_rows: usize) -> usize {
    selected.saturating_sub(visible_rows.saturating_sub(1))
}

pub fn contains(r: Rect, x: u16, y: u16) -> bool {
    x >= r.x && x < r.right() && y >= r.y && y < r.bottom()
}
