//! The frame tick: delayed config reloads, autosave, key timeouts, toast
//! expiry, and composing frames for dirty clients.

use std::time::{Duration, Instant};

use rmx_proto::ServerMsg;

use super::client::make_ctx;
use super::{ClientId, Core, DEFAULT_SIZE};
use crate::compositor::View;
use crate::persist;

const AUTOSAVE_INTERVAL: Duration = Duration::from_secs(30);

impl Core {
    pub(super) fn on_tick(&mut self) {
        let now = Instant::now();
        if self.reload_at.is_some_and(|t| t <= now) {
            self.reload_at = None;
            self.reload_config(None);
        }
        if self.session_changed
            && !self.protect_session_file
            && now.duration_since(self.last_save) >= AUTOSAVE_INTERVAL
            && let Some(path) = self.opts.session_file.clone()
        {
            if let Err(err) = persist::save(&path, &self.snapshot()) {
                tracing::warn!("autosave failed: {err:#}");
            }
            self.session_changed = false;
            self.last_save = now;
        }
        let ids: Vec<ClientId> = self.clients.keys().copied().collect();
        for id in ids {
            let c = self.clients.get_mut(&id).unwrap();
            if c.engine.tick(&c.keymap, now) {
                c.hints = None;
                c.dirty = true;
            }
            if let Some((_, at, error)) = &c.toast {
                let ttl = if *error {
                    Duration::from_secs(8)
                } else {
                    Duration::from_secs(4)
                };
                if now.duration_since(*at) > ttl {
                    c.toast = None;
                    c.dirty = true;
                }
            }
            if !c.dirty {
                continue;
            }
            // A full channel means the client is behind: skip this frame and
            // send the latest state next tick instead of queueing.
            if c.tx.capacity() == 0 {
                continue;
            }
            let Some(ws) = self.workspaces.iter().find(|w| w.id == c.workspace) else {
                continue;
            };
            let ctx = make_ctx(
                &self.config,
                &c.keymap,
                &self.workspaces,
                &self.panes,
                &self.attached,
                c.workspace,
                c.launch_cwd.as_deref(),
                &self.opts,
            );
            let view_size = self
                .sizes
                .get(&c.workspace)
                .copied()
                .unwrap_or(DEFAULT_SIZE);
            let view = View {
                ctx: &ctx,
                workspace: ws,
                view_size,
                mode: c.engine.mode(),
                hints: c.hints.as_deref(),
                toast: c.toast.as_ref().map(|(t, _, e)| (t.as_str(), *e)),
                select: c.select.as_ref(),
                overlays: &c.overlays,
            };
            // A drawing bug must cost one frame, not every session on the
            // server: the core task would otherwise die with the panic.
            let composed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                c.compositor.compose(&view)
            }));
            c.dirty = false;
            let Ok(frame) = composed else {
                tracing::error!(client = id, size = ?c.size, "composing a frame panicked; frame skipped");
                c.compositor.resize(c.size.0, c.size.1);
                continue;
            };
            if c.tx.try_send(ServerMsg::Frame(frame)).is_err() {
                c.dirty = true;
            }
        }
    }
}
