//! The text of each manager row: the name on the left and the details
//! (folder, counts, viewers, pane id) on the right.

use super::{Manager, Row};
use crate::state::Workspace;
use crate::ui::Ctx;

impl Manager {
    /// `(text, meta)` for a workspace, tab or pane row of workspace `ws`;
    /// `None` for the "new workspace here" row, which is drawn separately.
    pub(super) fn row_label(
        &self,
        row: Row,
        ws: &Workspace,
        ctx: &Ctx,
    ) -> Option<(String, String)> {
        let label = match row {
            Row::NewHere => return None,
            Row::Workspace(w) => {
                let arrow = if self.expanded_ws.contains(&w) {
                    "▾"
                } else {
                    "▸"
                };
                let dot = if w == ctx.current { "●" } else { " " };
                let n = ws.tabs.len();
                let number = ctx
                    .workspaces
                    .iter()
                    .position(|x| x.id == w)
                    .filter(|&i| i < 9)
                    .map_or("  ".to_owned(), |i| format!("{} ", i + 1));
                // Who else is looking at it (this client counts for the current one).
                let viewers = ctx
                    .attached
                    .get(&w)
                    .copied()
                    .unwrap_or(0)
                    .saturating_sub(usize::from(w == ctx.current));
                // Where it lives, so sessions are easy to tell apart.
                let dir = ctx
                    .panes
                    .get(&ws.active().active_pane)
                    .map(|p| crate::shorten_home(&p.cwd()))
                    .unwrap_or_default();
                let mut meta = format!("{dir} · {n} tab{}", if n == 1 { "" } else { "s" });
                if viewers > 0 {
                    meta.push_str(&format!(" · {viewers} attached"));
                }
                (format!("{arrow} {number}{dot} {}", ws.name), meta)
            }
            Row::Tab(w, t) => {
                let tab = &ws.tabs[t];
                let arrow = if self.expanded_tabs.contains(&(w, t)) {
                    "▾"
                } else {
                    "▸"
                };
                let star = if t == ws.active_tab { "*" } else { " " };
                let n = tab.layout.panes().len();
                (
                    format!(
                        "    {arrow} {}{star} {}",
                        t + 1,
                        crate::tab_title(tab, ctx.panes)
                    ),
                    format!("{n} pane{}", if n == 1 { "" } else { "s" }),
                )
            }
            Row::Pane(_, t, p) => {
                let tab = &ws.tabs[t];
                let n = tab
                    .layout
                    .panes()
                    .iter()
                    .position(|&x| x == p)
                    .map_or(0, |i| i + 1);
                let title = ctx.panes.get(&p).map(crate::pane_title).unwrap_or_default();
                let active = if tab.active_pane == p { "›" } else { " " };
                (format!("        {active}{n} {title}"), format!("%{p}"))
            }
        };
        Some(label)
    }
}

/// Keeps the end of `s` (the useful part of a path) within `max` chars.
pub(super) fn ellipsize_start(s: &str, max: usize) -> String {
    let n = s.chars().count();
    if n <= max || max == 0 {
        return if max == 0 {
            String::new()
        } else {
            s.to_owned()
        };
    }
    let tail: String = s.chars().skip(n - (max - 1)).collect();
    format!("…{tail}")
}
