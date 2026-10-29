//! The control API: `rmx` CLI requests, target resolution and the event
//! stream for subscribers.

use std::path::PathBuf;

use anyhow::Result;
use rmx_config::Config;
use rmx_core::layout::PaneId;
use rmx_proto::{Request, ServerMsg};
use tokio::sync::mpsc;

use super::helpers::unescape;
use super::{ClientId, Core, DEFAULT_SIZE};
use crate::state::{Target, WorkspaceId};
use crate::{api, persist};

impl Core {
    pub(super) fn request(&mut self, id: ClientId, req: Request) {
        let Some(tx) = self.pending_conns().get(&id).cloned() else {
            return;
        };
        if req == Request::Subscribe {
            self.subscribers.push(tx);
            self.pending_conns().remove(&id);
            return;
        }
        let result = self.handle_request(req);
        let msg = match result {
            Ok(body) => ServerMsg::Response { ok: true, body },
            Err(err) => ServerMsg::Response {
                ok: false,
                body: format!("{err:#}"),
            },
        };
        let _ = tx.try_send(msg);
    }

    fn handle_request(&mut self, req: Request) -> Result<String> {
        match req {
            Request::List => Ok(api::list(
                &self.workspaces,
                &self.panes,
                &self.clients_by_ws(),
            )),
            Request::NewWorkspace { name, cwd, command } => {
                let id = self.new_workspace(name, cwd.map(PathBuf::from), command, DEFAULT_SIZE)?;
                let name = self
                    .workspace(id)
                    .map(|w| w.name.clone())
                    .unwrap_or_default();
                Ok(serde_json::json!({ "workspace": id, "name": name }).to_string())
            }
            Request::CloseWorkspace { name } => {
                let id = self.find_workspace(&name)?;
                self.close_workspace(id);
                Ok("{}".into())
            }
            Request::RenameWorkspace { from, to } => {
                let id = self.find_workspace(&from)?;
                anyhow::ensure!(
                    self.workspaces.iter().all(|w| w.name != to),
                    "a workspace named \"{to}\" already exists"
                );
                self.workspace_mut(id).unwrap().name = to;
                self.session_changed = true;
                Ok("{}".into())
            }
            Request::SendKeys { target, text } => {
                let pane = self.resolve(target.as_deref())?;
                self.panes[&pane].write(unescape(&text).into_bytes());
                Ok("{}".into())
            }
            Request::Capture { target } => {
                let pane = self.resolve(target.as_deref())?;
                Ok(serde_json::json!({ "pane": pane, "text": self.panes[&pane].emulator.contents() }).to_string())
            }
            Request::Save => {
                let path = self
                    .opts
                    .session_file
                    .clone()
                    .ok_or_else(|| anyhow::anyhow!("saving is disabled"))?;
                persist::save(&path, &self.snapshot())?;
                self.session_changed = false;
                self.protect_session_file = false;
                Ok(serde_json::json!({ "path": path }).to_string())
            }
            Request::Restore => {
                let (restored, failed) = self.restore()?;
                Ok(serde_json::json!({ "restored_workspaces": restored, "failed_workspaces": failed }).to_string())
            }
            Request::ReloadConfig => {
                Config::load(&self.opts.config_path).map_err(|e| anyhow::anyhow!("{e}"))?;
                self.reload_config(None);
                Ok("{}".into())
            }
            Request::Stop => {
                self.stop = true;
                Ok("{}".into())
            }
            Request::Subscribe => unreachable!("handled in request()"),
        }
    }

    fn find_workspace(&self, name: &str) -> Result<WorkspaceId> {
        self.workspaces
            .iter()
            .find(|w| w.name == name)
            .map(|w| w.id)
            .ok_or_else(|| anyhow::anyhow!("no workspace named \"{name}\""))
    }

    /// Resolves a CLI target; no target means the most recently used pane.
    fn resolve(&self, target: Option<&str>) -> Result<PaneId> {
        let latest = || self.workspaces.iter().max_by_key(|w| w.last_used);
        let target = match target {
            None => {
                return latest()
                    .map(|w| w.active().active_pane)
                    .ok_or_else(|| anyhow::anyhow!("no workspaces"));
            }
            Some(t) => Target::parse(t).map_err(anyhow::Error::msg)?,
        };
        match target {
            Target::Pane(p) if self.panes.contains_key(&p) => Ok(p),
            Target::Pane(p) => anyhow::bail!("no pane %{p}"),
            Target::Path {
                workspace,
                tab,
                pane,
            } => {
                let ws = match workspace {
                    Some(name) => self.workspace(self.find_workspace(&name)?).unwrap(),
                    None => latest().ok_or_else(|| anyhow::anyhow!("no workspaces"))?,
                };
                let t = match tab {
                    Some(i) => ws.tabs.get(i - 1).ok_or_else(|| {
                        anyhow::anyhow!("workspace \"{}\" has no tab {i}", ws.name)
                    })?,
                    None => ws.active(),
                };
                match pane {
                    Some(i) => t
                        .layout
                        .panes()
                        .get(i - 1)
                        .copied()
                        .ok_or_else(|| anyhow::anyhow!("tab has no pane {i}")),
                    None => Ok(t.active_pane),
                }
            }
        }
    }

    pub(super) fn emit(&mut self, kind: &str, mut data: serde_json::Value) {
        if self.subscribers.is_empty() {
            return;
        }
        data["event"] = kind.into();
        let line = data.to_string();
        self.subscribers.retain(|tx| {
            !matches!(
                tx.try_send(ServerMsg::Event(line.clone())),
                Err(mpsc::error::TrySendError::Closed(_))
            )
        });
    }
}
