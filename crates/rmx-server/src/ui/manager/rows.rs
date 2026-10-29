//! The flattened, filtered tree the manager shows, the selection within it,
//! and what opening or operating on a row targets.

use std::path::Path;

use super::{Manager, Row, tab_id};
use crate::state::{TabId, WorkspaceId};
use crate::ui::{Cmd, Ctx, Outcome};

impl Manager {
    pub(super) fn rows(&self, ctx: &Ctx) -> Vec<Row> {
        let query = self
            .filter
            .as_ref()
            .map(|f| f.text.to_lowercase())
            .unwrap_or_default();
        let mut rows = Vec::new();
        if ctx.launch_cwd.is_some() && query.is_empty() {
            rows.push(Row::NewHere);
        }
        for ws in ctx.workspaces {
            let tab_name = |t: usize| crate::tab_title(&ws.tabs[t], ctx.panes).to_lowercase();
            let ws_match = ws.name.to_lowercase().contains(&query);
            let tab_matches: Vec<usize> = (0..ws.tabs.len())
                .filter(|&t| tab_name(t).contains(&query))
                .collect();
            if !query.is_empty() && !ws_match && tab_matches.is_empty() {
                continue;
            }
            rows.push(Row::Workspace(ws.id));
            // While filtering, expand everything that matched.
            if !self.expanded_ws.contains(&ws.id) && query.is_empty() {
                continue;
            }
            for (t, tab) in ws.tabs.iter().enumerate() {
                if !query.is_empty() && !ws_match && !tab_matches.contains(&t) {
                    continue;
                }
                rows.push(Row::Tab(ws.id, t));
                if self.expanded_tabs.contains(&(ws.id, t)) {
                    rows.extend(
                        tab.layout
                            .panes()
                            .into_iter()
                            .map(|p| Row::Pane(ws.id, t, p)),
                    );
                }
            }
        }
        rows
    }

    /// Index of the selection, falling back to the current workspace (e.g.
    /// after the selected item was closed).
    pub(super) fn index(&self, rows: &[Row], ctx: &Ctx) -> usize {
        self.selected
            .and_then(|s| rows.iter().position(|r| *r == s))
            .or_else(|| rows.iter().position(|r| r.workspace() == Some(ctx.current)))
            .unwrap_or(0)
    }

    pub(super) fn select(&mut self, rows: &[Row], index: usize) {
        self.selected = rows.get(index).copied();
    }

    pub(super) fn open(&self, row: Row, ctx: &Ctx) -> Outcome {
        let tab = |w, t| tab_id(ctx, w, t);
        let cmd = match row {
            Row::NewHere => Cmd::NewWorkspace {
                name: None,
                cwd: ctx.launch_cwd.map(Path::to_path_buf),
            },
            Row::Workspace(w) => Cmd::Open {
                workspace: w,
                tab: None,
                pane: None,
            },
            Row::Tab(w, t) => Cmd::Open {
                workspace: w,
                tab: tab(w, t),
                pane: None,
            },
            Row::Pane(w, t, p) => Cmd::Open {
                workspace: w,
                tab: tab(w, t),
                pane: Some(p),
            },
        };
        Outcome::cmd(true, cmd)
    }

    /// The tab an operation on `row` applies to: the row's own tab, or the
    /// workspace's active tab.
    pub(super) fn tab_for(row: Row, ctx: &Ctx) -> Option<(WorkspaceId, TabId)> {
        let (w, t) = match row {
            Row::Tab(w, t) | Row::Pane(w, t, _) => (w, t),
            Row::NewHere => return None,
            Row::Workspace(w) => (w, ctx.workspaces.iter().find(|x| x.id == w)?.active_tab),
        };
        Some((w, tab_id(ctx, w, t)?))
    }
}
