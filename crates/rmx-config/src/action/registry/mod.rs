//! The static action registry: one [`ActionInfo`] row per action id.
//!
//! Rows live in per-group files (in menu order) and are stitched into a single
//! [`ACTIONS`] table at compile time.

mod select;
mod session;
mod tabs_panes;
mod view;

use std::sync::LazyLock;

/// Menu-bar grouping.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Category {
    File,
    Edit,
    View,
    Tab,
    Pane,
    Workspace,
    Help,
}

/// Static registry row; `label` is the argument-free form.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ActionInfo {
    pub id: &'static str,
    pub label: &'static str,
    pub category: Category,
    pub description: &'static str,
}

const fn info(
    id: &'static str,
    label: &'static str,
    category: Category,
    description: &'static str,
) -> ActionInfo {
    ActionInfo {
        id,
        label,
        category,
        description,
    }
}

/// Row groups in menu order; concatenated into [`ACTIONS`].
const GROUPS: [&[ActionInfo]; 4] = [tabs_panes::ROWS, select::ROWS, view::ROWS, session::ROWS];

const LEN: usize = {
    let mut n = 0;
    let mut i = 0;
    while i < GROUPS.len() {
        n += GROUPS[i].len();
        i += 1;
    }
    n
};

const ALL: [ActionInfo; LEN] = {
    let mut out = [GROUPS[0][0]; LEN];
    let mut k = 0;
    let mut i = 0;
    while i < GROUPS.len() {
        let mut j = 0;
        while j < GROUPS[i].len() {
            out[k] = GROUPS[i][j];
            k += 1;
            j += 1;
        }
        i += 1;
    }
    out
};

/// One row per action id, in menu order.
pub const ACTIONS: &[ActionInfo] = &ALL;

static IDS: LazyLock<Vec<&'static str>> = LazyLock::new(|| ACTIONS.iter().map(|a| a.id).collect());

/// Every action id, for did-you-mean suggestions and completion.
pub fn all_action_ids() -> &'static [&'static str] {
    &IDS
}

pub fn action_info(id: &str) -> Option<&'static ActionInfo> {
    ACTIONS.iter().find(|a| a.id == id)
}
