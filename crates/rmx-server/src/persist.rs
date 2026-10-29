//! Built-in session save/restore (what tmux needs resurrect + continuum for).
//!
//! We save structure, not processes: layouts, names, each pane's directory
//! and any explicit command. Restoring starts fresh shells (or re-runs those
//! commands) in the same places, which is what survives a reboot anyway.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use rmx_core::layout::{Node, PaneId};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Snapshot {
    pub version: u32,
    pub workspaces: Vec<WorkspaceSnap>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkspaceSnap {
    pub name: String,
    pub active_tab: usize,
    pub tabs: Vec<TabSnap>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TabSnap {
    pub name: Option<String>,
    /// The layout tree with the *saved* pane ids, mapped to `panes`.
    pub layout: Node,
    pub active_pane: PaneId,
    pub panes: HashMap<PaneId, PaneSnap>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PaneSnap {
    pub cwd: PathBuf,
    pub command: Option<String>,
}

pub const SNAPSHOT_VERSION: u32 = 1;

/// `<local data dir>/rmx/<server name>-session.json`.
pub fn default_path(server_name: &str) -> PathBuf {
    dirs_data()
        .join("rmx")
        .join(format!("{server_name}-session.json"))
}

fn dirs_data() -> PathBuf {
    // Same base as the log file (see src/logging.rs) so users find both together.
    std::env::var_os(if cfg!(windows) {
        "LOCALAPPDATA"
    } else {
        "XDG_DATA_HOME"
    })
    .map(PathBuf::from)
    .or_else(|| {
        std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).map(|h| {
            let h = PathBuf::from(h);
            if cfg!(target_os = "macos") {
                h.join("Library/Application Support")
            } else if cfg!(windows) {
                h.join("AppData/Local")
            } else {
                h.join(".local/share")
            }
        })
    })
    .unwrap_or_else(std::env::temp_dir)
}

/// Writes atomically (temp file + rename) so a crash mid-save never leaves a
/// truncated session file behind.
pub fn save(path: &Path, snap: &Snapshot) -> Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    }
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, serde_json::to_vec_pretty(snap)?)
        .with_context(|| format!("writing {}", tmp.display()))?;
    fs::rename(&tmp, path).with_context(|| format!("replacing {}", path.display()))?;
    Ok(())
}

pub fn load(path: &Path) -> Result<Option<Snapshot>> {
    let bytes = match fs::read(path) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e).with_context(|| format!("reading {}", path.display())),
    };
    let snap: Snapshot =
        serde_json::from_slice(&bytes).with_context(|| format!("parsing {}", path.display()))?;
    anyhow::ensure!(
        snap.version == SNAPSHOT_VERSION,
        "unsupported session file version {}",
        snap.version
    );
    Ok(Some(snap))
}

/// Rewrites pane ids in a saved layout through `map` (saved id → new id).
pub fn remap(node: &Node, map: &HashMap<PaneId, PaneId>) -> Node {
    match node {
        Node::Leaf(p) => Node::Leaf(map.get(p).copied().unwrap_or(*p)),
        Node::Split { axis, children } => Node::Split {
            axis: *axis,
            children: children.iter().map(|(c, w)| (remap(c, map), *w)).collect(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rmx_core::layout::{Layout, Side};

    #[test]
    fn save_load_round_trip_and_remap() {
        let dir = std::env::temp_dir().join(format!("rmx-persist-{}", std::process::id()));
        let path = dir.join("s.json");
        let mut layout = Layout::new(1);
        layout.split(1, 2, Side::Right, 50);
        let snap = Snapshot {
            version: SNAPSHOT_VERSION,
            workspaces: vec![WorkspaceSnap {
                name: "main".into(),
                active_tab: 0,
                tabs: vec![TabSnap {
                    name: Some("code".into()),
                    layout: layout.root().clone(),
                    active_pane: 2,
                    panes: HashMap::from([
                        (
                            1,
                            PaneSnap {
                                cwd: "/tmp".into(),
                                command: None,
                            },
                        ),
                        (
                            2,
                            PaneSnap {
                                cwd: "/".into(),
                                command: Some("htop".into()),
                            },
                        ),
                    ]),
                }],
            }],
        };
        save(&path, &snap).unwrap();
        assert_eq!(load(&path).unwrap(), Some(snap.clone()));
        let remapped = Layout::from_root(remap(
            &snap.workspaces[0].tabs[0].layout,
            &HashMap::from([(1, 10), (2, 20)]),
        ));
        assert_eq!(remapped.panes(), vec![10, 20]);
        assert_eq!(load(&dir.join("missing.json")).unwrap(), None);
        fs::remove_dir_all(dir).unwrap();
    }
}
