//! Item-list behaviour shared by drop-downs and the context menu: drawing
//! entries with their key hints, and stepping over separators.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};

use super::Item;
use crate::ui::{Ctx, panel, put};

/// The next entry from `selected` in direction `delta`, skipping separators.
pub(super) fn step_entry(items: &[Item], selected: usize, delta: isize) -> usize {
    let n = items.len() as isize;
    let mut i = selected as isize;
    for _ in 0..n {
        i = (i + delta).rem_euclid(n);
        if matches!(items[i as usize], Item::Entry(..)) {
            break;
        }
    }
    i as usize
}

pub(super) fn render_items(
    buf: &mut Buffer,
    rect: Rect,
    ctx: &Ctx,
    items: &[Item],
    selected: usize,
    title: &str,
) {
    let theme = ctx.theme;
    let inner = panel(buf, rect, title, theme);
    let base = Style::default().fg(theme.menu_fg).bg(theme.menu_bg);
    let hl = Style::default()
        .fg(theme.menu_highlight_fg)
        .bg(theme.menu_highlight_bg);
    for (i, item) in items.iter().enumerate() {
        let y = inner.y + i as u16;
        if y >= inner.bottom() {
            break;
        }
        match item {
            Item::Separator => {
                put(
                    buf,
                    inner.x,
                    y,
                    &"─".repeat(usize::from(inner.width)),
                    inner.width,
                    base.fg(theme.border),
                );
            }
            Item::Entry(label, action) => {
                let style = if i == selected { hl } else { base };
                buf.set_style(Rect::new(inner.x, y, inner.width, 1), style);
                // Labels give way to the key hint rather than run under it.
                let hint = ctx.keymap.key_hint(action);
                let hint_w = hint.as_ref().map_or(0, |h| h.chars().count() as u16 + 2);
                let label_w = if hint_w + 2 < inner.width {
                    inner.width - 1 - hint_w
                } else {
                    inner.width.saturating_sub(1)
                };
                put(buf, inner.x + 1, y, label, label_w, style);
                if let Some(hint) = hint {
                    let w = hint.chars().count() as u16;
                    if w + 2 < inner.width {
                        put(
                            buf,
                            inner.right() - w - 1,
                            y,
                            &hint,
                            w,
                            style.add_modifier(Modifier::DIM),
                        );
                    }
                }
            }
        }
    }
}
