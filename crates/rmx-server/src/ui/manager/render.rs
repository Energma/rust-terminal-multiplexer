//! Drawing the manager: header (filter or counts), the row list and the
//! footer of key hints. The preview pane lives in `preview`.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};

use super::labels::ellipsize_start;
use super::{Manager, Row};
use crate::ui::{Ctx, list_scroll, panel, put};

const FOOTER: &[(&str, &str)] = &[
    ("Enter", "open"),
    ("1-9", "jump"),
    ("←→", "fold"),
    ("n", "new workspace"),
    ("t", "new tab"),
    ("|", "split right"),
    ("-", "split down"),
    ("r", "rename"),
    ("x", "close"),
    ("/", "filter"),
    ("Esc", "back"),
];

impl Manager {
    pub fn render(&self, ctx: &Ctx, screen: Rect, buf: &mut Buffer) {
        let theme = ctx.theme;
        let (outer, list, preview, footer) = Self::layout(screen);
        if list.height == 0 || list.width < 10 {
            return;
        }
        panel(buf, outer, "Session Manager", theme);
        let base = Style::default().fg(theme.menu_fg).bg(theme.menu_bg);
        let dim = base.add_modifier(Modifier::DIM);
        let accent = base.fg(theme.accent);
        let hl = Style::default()
            .fg(theme.menu_highlight_fg)
            .bg(theme.menu_highlight_bg);

        // Header: filter or counts.
        let header_y = list.y - 1;
        match &self.filter {
            Some(f) => {
                put(buf, list.x, header_y, "Filter:", 7, accent);
                f.render(
                    buf,
                    list.x + 8,
                    header_y,
                    list.width.saturating_sub(8),
                    base,
                );
            }
            None => {
                let tabs: usize = ctx.workspaces.iter().map(|w| w.tabs.len()).sum();
                let text = format!(
                    "{} workspaces · {} tabs · {} panes · ● = here",
                    ctx.workspaces.len(),
                    tabs,
                    ctx.panes.len()
                );
                put(buf, list.x, header_y, &text, list.width, dim);
            }
        }

        let rows = self.rows(ctx);
        let sel = self.index(&rows, ctx);
        let height = usize::from(list.height);
        let scroll = list_scroll(sel, height);
        for (i, row) in rows.iter().enumerate().skip(scroll).take(height) {
            let y = list.y + (i - scroll) as u16;
            let style = if i == sel { hl } else { base };
            buf.set_style(Rect::new(list.x, y, list.width, 1), style);
            if *row == Row::NewHere {
                let dir = ctx.launch_cwd.map(crate::shorten_home).unwrap_or_default();
                let room = usize::from(list.width).saturating_sub(19);
                let text = format!("+ New workspace in {}", ellipsize_start(&dir, room));
                let st = if i == sel {
                    style
                } else {
                    style.fg(theme.accent)
                };
                put(
                    buf,
                    list.x,
                    y,
                    &text,
                    list.width,
                    st.add_modifier(Modifier::BOLD),
                );
                continue;
            }
            let Some(ws) = ctx
                .workspaces
                .iter()
                .find(|w| Some(w.id) == row.workspace())
            else {
                continue;
            };
            let Some((text, meta)) = self.row_label(*row, ws, ctx) else {
                continue;
            };
            // Details never take more than half the row, so names stay readable.
            let meta = ellipsize_start(&meta, usize::from(list.width / 2));
            let meta_w = meta.chars().count() as u16;
            put(
                buf,
                list.x,
                y,
                &text,
                list.width.saturating_sub(meta_w + 1),
                style,
            );
            put(
                buf,
                list.right().saturating_sub(meta_w + 1),
                y,
                &meta,
                meta_w,
                if i == sel { style } else { dim },
            );
        }

        Self::render_preview(ctx, &rows, sel, preview, buf);

        // Footer: every operation, always visible.
        let mut x = footer.x;
        for (key, what) in FOOTER {
            let k = format!(" {key} ");
            let w = format!("{what}  ");
            let need = (k.chars().count() + w.chars().count()) as u16;
            if x + need > footer.right() {
                break;
            }
            put(buf, x, footer.y, &k, need, hl);
            x += k.chars().count() as u16;
            put(buf, x, footer.y, &w, need, dim);
            x += w.chars().count() as u16;
        }
    }
}
