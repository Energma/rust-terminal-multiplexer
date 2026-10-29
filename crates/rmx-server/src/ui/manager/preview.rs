//! The manager's right-hand preview: the selected tab rendered live, or an
//! explanation when the "new workspace here" row is selected.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;

use super::{Manager, Row};
use crate::compositor::draw_separators;
use crate::ui::{Ctx, panel, put};

impl Manager {
    pub(super) fn render_preview(
        ctx: &Ctx,
        rows: &[Row],
        sel: usize,
        preview: Rect,
        buf: &mut Buffer,
    ) {
        let theme = ctx.theme;
        let base = Style::default().fg(theme.menu_fg).bg(theme.menu_bg);
        let accent = base.fg(theme.accent);

        // Preview of the selected (or the workspace's active) tab, live.
        if preview.width >= 10
            && let Some(&row) = rows.get(sel)
            && let Some(ws) = ctx
                .workspaces
                .iter()
                .find(|w| Some(w.id) == row.workspace())
        {
            let t = match row {
                Row::Tab(_, t) | Row::Pane(_, t, _) => t,
                Row::Workspace(_) | Row::NewHere => ws.active_tab,
            };
            let tab = &ws.tabs[t];
            let focus = match row {
                Row::Pane(_, _, p) => p,
                _ => tab.active_pane,
            };
            let title = format!(
                "{} › {} {}",
                ws.name,
                t + 1,
                crate::tab_title(tab, ctx.panes)
            );
            let inner = panel(buf, preview, &title, theme);
            let (rects, seps) = tab.layout.compute(inner);
            for (pane_id, rect) in &rects {
                if let Some(pane) = ctx.panes.get(pane_id) {
                    pane.emulator.render(*rect, buf);
                }
            }
            draw_separators(buf, inner, &seps, &rects, focus, theme);
            if let Some(pane) = ctx.panes.get(&focus) {
                let info = format!(" {} ", crate::shorten_home(&pane.cwd()));
                let w = (info.chars().count() as u16).min(preview.width.saturating_sub(4));
                put(buf, preview.x + 2, preview.bottom() - 1, &info, w, accent);
            }
        }

        if preview.width >= 10 && rows.get(sel) == Some(&Row::NewHere) {
            let inner = panel(buf, preview, "New workspace", theme);
            let dir = ctx.launch_cwd.map(crate::shorten_home).unwrap_or_default();
            let lines = [
                format!("Start a new workspace in {dir}"),
                String::new(),
                "Enter  create it, named after the folder".to_owned(),
                "n      create it with a name you choose".to_owned(),
                "1-9    or jump to an existing workspace".to_owned(),
            ];
            for (k, line) in lines.iter().enumerate() {
                put(
                    buf,
                    inner.x + 1,
                    inner.y + 1 + k as u16,
                    line,
                    inner.width.saturating_sub(2),
                    base,
                );
            }
        }
    }
}
