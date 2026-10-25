//! Workspaces (tmux sessions) and tabs (tmux windows). Panes themselves live
//! in a flat map on the core; tabs refer to them by id through their layout.

use rmx_core::layout::{Layout, PaneId};

use std::sync::atomic::{AtomicU32, Ordering};

pub type WorkspaceId = u32;
/// Stable for a tab's lifetime, unlike its index, so a dialog answered
/// after other tabs closed still acts on the tab it was opened for.
pub type TabId = u32;

static NEXT_TAB: AtomicU32 = AtomicU32::new(1);

pub struct Workspace {
    pub id: WorkspaceId,
    pub name: String,
    pub tabs: Vec<Tab>,
    pub active_tab: usize,
    /// For "go back to the last tab" and sensible focus after closing one.
    pub last_tab: usize,
    /// Monotonic stamp of last use, so `rmx attach` picks the latest.
    pub last_used: u64,
}

pub struct Tab {
    pub id: TabId,
    /// Set when the user renamed the tab; otherwise it follows the title of
    /// its active pane, like Windows Terminal.
    pub custom_name: Option<String>,
    pub layout: Layout,
    pub active_pane: PaneId,
    pub zoomed: bool,
    /// Typed keys go to every pane in the tab (tmux synchronize-panes).
    pub sync_input: bool,
    /// Position in the layout presets for "next layout".
    pub preset_cursor: usize,
}

impl Tab {
    pub fn new(pane: PaneId) -> Self {
        Self {
            id: NEXT_TAB.fetch_add(1, Ordering::Relaxed),
            custom_name: None,
            layout: Layout::new(pane),
            active_pane: pane,
            zoomed: false,
            sync_input: false,
            preset_cursor: 0,
        }
    }
}

impl Workspace {
    pub fn active(&self) -> &Tab {
        &self.tabs[self.active_tab]
    }

    pub fn active_mut(&mut self) -> &mut Tab {
        &mut self.tabs[self.active_tab]
    }

    pub fn select_tab(&mut self, index: usize) {
        if index < self.tabs.len() && index != self.active_tab {
            self.last_tab = self.active_tab;
            self.active_tab = index;
        }
    }

    pub fn tab_index(&self, id: TabId) -> Option<usize> {
        self.tabs.iter().position(|t| t.id == id)
    }

    pub fn tab_of(&self, pane: PaneId) -> Option<usize> {
        self.tabs.iter().position(|t| t.layout.contains(pane))
    }

    /// Removes tab `index`, keeping the selection on a neighbour.
    pub fn remove_tab(&mut self, index: usize) -> Tab {
        let tab = self.tabs.remove(index);
        if self.tabs.is_empty() {
            self.active_tab = 0;
        } else if self.active_tab > index || self.active_tab >= self.tabs.len() {
            self.active_tab = self.active_tab.saturating_sub(1);
        }
        self.last_tab = self.last_tab.min(self.tabs.len().saturating_sub(1));
        tab
    }

    pub fn pane_ids(&self) -> Vec<PaneId> {
        self.tabs.iter().flat_map(|t| t.layout.panes()).collect()
    }
}

/// Resolves a CLI target: `workspace`, `workspace:tab`, `workspace:tab.pane`
/// (tab and pane 1-based, as shown in the UI) or `%<pane id>`.
#[derive(Debug, PartialEq, Eq)]
pub enum Target {
    Pane(PaneId),
    Path {
        workspace: Option<String>,
        tab: Option<usize>,
        pane: Option<usize>,
    },
}

impl Target {
    pub fn parse(s: &str) -> Result<Self, String> {
        if let Some(id) = s.strip_prefix('%') {
            return id
                .parse()
                .map(Target::Pane)
                .map_err(|_| format!("bad pane id in {s:?}"));
        }
        let (ws, rest) = match s.split_once(':') {
            Some((w, r)) => (w, Some(r)),
            None => (s, None),
        };
        let index = |v: &str| -> Result<usize, String> {
            v.parse::<usize>()
                .ok()
                .filter(|&n| n >= 1)
                .ok_or_else(|| format!("expected a number from 1 in {s:?}"))
        };
        let (tab, pane) = match rest {
            None | Some("") => (None, None),
            Some(r) => match r.split_once('.') {
                Some((t, p)) => (
                    if t.is_empty() { None } else { Some(index(t)?) },
                    Some(index(p)?),
                ),
                None => (Some(index(r)?), None),
            },
        };
        Ok(Target::Path {
            workspace: (!ws.is_empty()).then(|| ws.to_owned()),
            tab,
            pane,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_targets() {
        assert_eq!(Target::parse("%7"), Ok(Target::Pane(7)));
        assert_eq!(
            Target::parse("main"),
            Ok(Target::Path {
                workspace: Some("main".into()),
                tab: None,
                pane: None
            })
        );
        assert_eq!(
            Target::parse("main:2.3"),
            Ok(Target::Path {
                workspace: Some("main".into()),
                tab: Some(2),
                pane: Some(3)
            })
        );
        assert_eq!(
            Target::parse(":2"),
            Ok(Target::Path {
                workspace: None,
                tab: Some(2),
                pane: None
            })
        );
        assert!(Target::parse("main:0").is_err());
        assert!(Target::parse("%x").is_err());
    }

    #[test]
    fn removing_tabs_keeps_a_valid_selection() {
        let mut ws = Workspace {
            id: 1,
            name: "w".into(),
            tabs: vec![],
            active_tab: 0,
            last_tab: 0,
            last_used: 0,
        };
        for p in 1..=3 {
            ws.tabs.push(Tab::new(p));
        }
        ws.select_tab(2);
        ws.remove_tab(2);
        assert_eq!(ws.active_tab, 1);
        ws.remove_tab(0);
        assert_eq!(ws.active_tab, 0);
        assert_eq!(ws.tab_of(2), Some(0));
    }
}
