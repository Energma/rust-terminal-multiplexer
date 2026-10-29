//! Drawing the palette: the query line and the ranked, scrolled entry list.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};

use super::{Entry, Palette};
use crate::ui::{Ctx, list_scroll, panel, put};

impl Palette {
    pub fn render(&self, ctx: &Ctx, screen: Rect, buf: &mut Buffer) {
        let theme = ctx.theme;
        let rect = Self::rect(screen);
        let inner = panel(buf, rect, "Command Palette", theme);
        if inner.height < 3 {
            return;
        }
        let base = Style::default().fg(theme.menu_fg).bg(theme.menu_bg);
        put(buf, inner.x + 1, inner.y, "›", 1, base.fg(theme.accent));
        self.input.render(
            buf,
            inner.x + 3,
            inner.y,
            inner.width.saturating_sub(4),
            base,
        );
        put(
            buf,
            inner.x,
            inner.y + 1,
            &"─".repeat(usize::from(inner.width)),
            inner.width,
            base.fg(theme.border),
        );

        let entries = self.entries(ctx);
        let rows = usize::from(inner.height - 2);
        let selected = self.selected.min(entries.len().saturating_sub(1));
        // Keep the selection in view; `scroll` is recomputed each frame.
        let scroll = list_scroll(selected, rows);
        let hl = Style::default()
            .fg(theme.menu_highlight_fg)
            .bg(theme.menu_highlight_bg);
        for (i, entry) in entries.iter().enumerate().skip(scroll).take(rows) {
            let y = inner.y + 2 + (i - scroll) as u16;
            let style = if i == selected { hl } else { base };
            buf.set_style(Rect::new(inner.x, y, inner.width, 1), style);
            let (label, detail, hint) = match entry {
                Entry::Action {
                    label,
                    detail,
                    action,
                } => (label.clone(), detail.clone(), ctx.keymap.key_hint(action)),
                Entry::Workspace { name, .. } => {
                    (format!("Switch to {name}"), "Workspace".into(), None)
                }
            };
            put(
                buf,
                inner.x + 1,
                y,
                &format!("{detail:<9}"),
                9,
                style.add_modifier(Modifier::DIM),
            );
            put(
                buf,
                inner.x + 11,
                y,
                &label,
                inner.width.saturating_sub(12),
                style,
            );
            if let Some(hint) = hint {
                let w = hint.chars().count() as u16;
                if w + 30 < inner.width {
                    put(
                        buf,
                        inner.right() - w - 1,
                        y,
                        &hint,
                        w,
                        style.fg(theme.accent),
                    );
                }
            }
        }
        if entries.is_empty() {
            put(
                buf,
                inner.x + 1,
                inner.y + 2,
                "No matching commands",
                inner.width,
                base.add_modifier(Modifier::DIM),
            );
        }
    }
}
