//! The session manager: a full-screen tree of workspaces › tabs › panes with
//! a live preview, from which users open, create, rename, close and split.
//!
//! It replaces tmux's `choose-tree`, but is built to be the place you *drive*
//! sessions from, with every operation one key away and listed on screen.

mod keys;
mod labels;
mod mouse;
mod ops;
mod preview;
mod render;
mod rows;

#[cfg(test)]
mod tests;

use std::collections::HashSet;
use std::time::Instant;

use rmx_core::layout::PaneId;

use super::{Ctx, TextInput};
use crate::state::{TabId, WorkspaceId};

fn tab_id(ctx: &Ctx, w: WorkspaceId, t: usize) -> Option<TabId> {
    ctx.workspaces
        .iter()
        .find(|x| x.id == w)?
        .tabs
        .get(t)
        .map(|tab| tab.id)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Row {
    /// "+ New workspace in <folder where rmx was run>".
    NewHere,
    Workspace(WorkspaceId),
    Tab(WorkspaceId, usize),
    Pane(WorkspaceId, usize, PaneId),
}

impl Row {
    fn workspace(self) -> Option<WorkspaceId> {
        match self {
            Row::NewHere => None,
            Row::Workspace(w) | Row::Tab(w, _) | Row::Pane(w, _, _) => Some(w),
        }
    }
}

pub struct Manager {
    selected: Option<Row>,
    expanded_ws: HashSet<WorkspaceId>,
    expanded_tabs: HashSet<(WorkspaceId, usize)>,
    filter: Option<TextInput>,
    last_click: Option<(Instant, u16)>,
}

impl Manager {
    pub fn new(ctx: &Ctx) -> Self {
        let mut expanded_ws = HashSet::new();
        expanded_ws.insert(ctx.current);
        let selected = ctx
            .workspaces
            .iter()
            .find(|w| w.id == ctx.current)
            .map(|w| Row::Tab(w.id, w.active_tab));
        Self {
            selected,
            expanded_ws,
            expanded_tabs: HashSet::new(),
            filter: None,
            last_click: None,
        }
    }

    pub fn paste(&mut self, text: &str) {
        if let Some(filter) = &mut self.filter {
            filter.insert_str(text);
        }
    }
}
