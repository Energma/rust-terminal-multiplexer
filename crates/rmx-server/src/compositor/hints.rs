//! The "which-key" popup shown while a key prefix is pending.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use rmx_config::Theme;

use crate::ui::{self, put};

/// The "which-key" popup listing what can follow the pressed prefix.
pub(super) fn draw_hints(buf: &mut Buffer, area: Rect, hints: &[(String, String)], theme: &Theme) {
    let key_w = hints
        .iter()
        .map(|(k, _)| k.chars().count())
        .max()
        .unwrap_or(1) as u16;
    let col_w = (key_w + 24).min(area.width);
    let cols = (area.width.saturating_sub(4) / col_w).clamp(1, 4);
    let rows = (hints.len() as u16).div_ceil(cols);
    let w = (cols * col_w + 2).min(area.width);
    let h = (rows + 2).min(area.height);
    let rect = Rect::new(
        area.right().saturating_sub(w),
        area.bottom().saturating_sub(h),
        w,
        h,
    );
    let inner = ui::panel(buf, rect, "Keys", theme);
    let base = Style::default().fg(theme.menu_fg).bg(theme.menu_bg);
    for (i, (key, label)) in hints.iter().enumerate() {
        let (c, r) = ((i as u16) / rows, (i as u16) % rows);
        if r >= inner.height {
            continue;
        }
        let x = inner.x + c * col_w;
        put(
            buf,
            x,
            inner.y + r,
            key,
            key_w,
            base.fg(theme.accent).add_modifier(Modifier::BOLD),
        );
        put(
            buf,
            x + key_w + 1,
            inner.y + r,
            label,
            col_w.saturating_sub(key_w + 2),
            base,
        );
    }
}
