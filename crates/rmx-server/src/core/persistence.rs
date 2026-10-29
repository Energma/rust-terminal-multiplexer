//! Saving the session to a snapshot and restoring it.

use std::collections::HashMap;

use anyhow::Result;
use rmx_core::layout::PaneId;

use super::{Core, DEFAULT_SIZE};
use crate::pane::SpawnSpec;
use crate::persist::{self, PaneSnap, Snapshot, TabSnap, WorkspaceSnap};
use crate::state::{Tab, Workspace};

impl Core {
    pub(super) fn snapshot(&self) -> Snapshot {
        Snapshot {
            version: persist::SNAPSHOT_VERSION,
            workspaces: self
                .workspaces
                .iter()
                .map(|w| WorkspaceSnap {
                    name: w.name.clone(),
                    active_tab: w.active_tab,
                    tabs: w
                        .tabs
                        .iter()
                        .map(|t| TabSnap {
                            name: t.custom_name.clone(),
                            layout: t.layout.root().clone(),
                            active_pane: t.active_pane,
                            panes: t
                                .layout
                                .panes()
                                .into_iter()
                                .filter_map(|p| {
                                    let pane = self.panes.get(&p)?;
                                    Some((
                                        p,
                                        PaneSnap {
                                            cwd: pane.cwd(),
                                            command: pane.command.clone(),
                                        },
                                    ))
                                })
                                .collect(),
                        })
                        .collect(),
                })
                .collect(),
        }
    }

    /// Recreates saved workspaces that don't exist yet; returns how many.
    /// Recreates saved workspaces that don't exist yet. Returns (restored,
    /// failed). A workspace that can't be fully recreated is skipped whole and
    /// its partial panes are closed, and autosave is paused so the session
    /// file still has it for another try.
    pub(super) fn restore(&mut self) -> Result<(usize, usize)> {
        let path = self
            .opts
            .session_file
            .clone()
            .ok_or_else(|| anyhow::anyhow!("no session file"))?;
        let Some(snap) = persist::load(&path)? else {
            return Ok((0, 0));
        };
        let (mut restored, mut failed) = (0, 0);
        for ws in snap.workspaces {
            if self.workspaces.iter().any(|w| w.name == ws.name) {
                continue;
            }
            match self.restore_tabs(ws.tabs) {
                Ok(tabs) if !tabs.is_empty() => {
                    let id = self.next_workspace;
                    self.next_workspace += 1;
                    let active_tab = ws.active_tab.min(tabs.len() - 1);
                    self.workspaces.push(Workspace {
                        id,
                        name: ws.name,
                        tabs,
                        active_tab,
                        last_tab: active_tab,
                        last_used: 0,
                    });
                    self.had_workspaces = true;
                    self.relayout(id);
                    restored += 1;
                }
                Ok(_) => {}
                Err(err) => {
                    tracing::warn!(workspace = %ws.name, "restore failed: {err:#}");
                    failed += 1;
                }
            }
        }
        if failed > 0 {
            self.protect_session_file = true;
        }
        Ok((restored, failed))
    }

    fn restore_tabs(&mut self, saved: Vec<TabSnap>) -> Result<Vec<Tab>> {
        let mut spawned: Vec<PaneId> = Vec::new();
        let result = (|| {
            let mut tabs = Vec::new();
            for t in saved {
                // Only panes the layout actually shows; ignore stray entries.
                let leaves = rmx_core::layout::Layout::from_root(t.layout.clone()).panes();
                let mut map = HashMap::new();
                for old in leaves {
                    let Some(p) = t.panes.get(&old) else { continue };
                    let spec = SpawnSpec {
                        cwd: Some(p.cwd.clone()),
                        command: p.command.clone(),
                        ..Default::default()
                    };
                    let new = self.spawn_pane(spec, DEFAULT_SIZE)?;
                    spawned.push(new);
                    map.insert(old, new);
                }
                let mapped: std::collections::HashSet<PaneId> = map.values().copied().collect();
                let remapped = persist::remap(&t.layout, &map);
                // Drop leaves we didn't spawn (unknown ids could alias live panes).
                let Some(layout) =
                    rmx_core::layout::Layout::sanitized(remapped, |p| mapped.contains(&p))
                else {
                    continue;
                };
                let active = map
                    .get(&t.active_pane)
                    .copied()
                    .filter(|p| layout.contains(*p))
                    .unwrap_or_else(|| layout.panes()[0]);
                tabs.push(Tab {
                    custom_name: t.name,
                    layout,
                    active_pane: active,
                    ..Tab::new(active)
                });
            }
            Ok(tabs)
        })();
        if result.is_err() {
            for p in spawned {
                self.panes.remove(&p);
            }
        }
        result
    }
}
