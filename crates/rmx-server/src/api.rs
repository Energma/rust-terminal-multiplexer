//! JSON shapes for `rmx list --json` and friends. Kept small and stable:
//! scripts depend on these field names.

use std::collections::HashMap;

use rmx_core::layout::PaneId;
use serde_json::json;

use crate::pane::Pane;
use crate::state::{Workspace, WorkspaceId};
use crate::{pane_title, tab_title};

pub fn list(
    workspaces: &[Workspace],
    panes: &HashMap<PaneId, Pane>,
    attached: &HashMap<WorkspaceId, usize>,
) -> String {
    let items: Vec<_> = workspaces
        .iter()
        .map(|w| {
            json!({
                "id": w.id,
                "name": w.name,
                "attached": attached.get(&w.id).copied().unwrap_or(0),
                "last_used": w.last_used,
                "tabs": w.tabs.iter().enumerate().map(|(i, t)| json!({
                    "index": i + 1,
                    "name": tab_title(t, panes),
                    "active": i == w.active_tab,
                    "zoomed": t.zoomed,
                    "panes": t.layout.panes().iter().enumerate().filter_map(|(j, p)| {
                        let pane = panes.get(p)?;
                        Some(json!({
                            "id": p,
                            "index": j + 1,
                            "title": pane_title(pane),
                            "cwd": pane.cwd(),
                            "pid": pane.pid,
                            "active": *p == t.active_pane,
                        }))
                    }).collect::<Vec<_>>(),
                })).collect::<Vec<_>>(),
            })
        })
        .collect();
    serde_json::to_string(&json!({ "workspaces": items })).expect("json")
}
