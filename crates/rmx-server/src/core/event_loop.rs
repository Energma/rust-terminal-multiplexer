//! Startup, the main event loop and shutdown: what wakes the core and where
//! each kind of event goes.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use anyhow::Result;
use rmx_config::Config;
use rmx_proto::{ClientMsg, ServerEnd, ServerMsg};
use tokio::sync::mpsc;

use super::{ClientId, Core};
use crate::ServerOptions;
use crate::pane::PaneEvent;
use crate::persist;

/// Caps redraws at ~60 fps; output bursts between ticks coalesce into one frame.
const FRAME_INTERVAL: Duration = Duration::from_millis(16);

pub(super) enum Event {
    Connected(ServerEnd),
    Message(ClientId, ClientMsg),
    Disconnected(ClientId),
    Pane(PaneEvent),
    ConfigChanged,
}

impl Core {
    pub fn new(opts: ServerOptions) -> Result<Self> {
        let (config, warnings) = match Config::load(&opts.config_path) {
            Ok(loaded) => loaded,
            Err(err) => {
                // A broken config must never stop users reaching their shells.
                tracing::error!("config error, using defaults: {err}");
                (Config::default(), Vec::new())
            }
        };
        for w in &warnings {
            tracing::warn!("{w}");
        }
        let (pane_tx, pane_rx) = mpsc::channel(256);
        let (events_tx, events_rx) = mpsc::channel(256);
        Ok(Self {
            opts,
            config,
            workspaces: Vec::new(),
            panes: HashMap::new(),
            clients: HashMap::new(),
            sizes: HashMap::new(),
            attached: HashMap::new(),
            subscribers: Vec::new(),
            next_pane: 1,
            next_workspace: 1,
            next_client: 1,
            clock: 0,
            paste_buffer: String::new(),
            pane_tx,
            pane_rx: Some(pane_rx),
            events_tx,
            events_rx: Some(events_rx),
            session_changed: false,
            last_save: Instant::now(),
            reload_at: None,
            had_workspaces: false,
            stop: false,
            pending: HashMap::new(),
            protect_session_file: false,
            _watcher: None,
        })
    }

    pub async fn run(mut self, mut incoming: mpsc::Receiver<ServerEnd>) -> Result<()> {
        let mut pane_rx = self.pane_rx.take().expect("run once");
        let mut events_rx = self.events_rx.take().expect("run once");
        if self.opts.watch_config {
            self._watcher = watch_config(&self.opts.config_path, self.events_tx.clone());
        }
        if (self.opts.restore || self.config.general.restore_on_start)
            && let Err(err) = self.restore()
        {
            tracing::warn!("restoring session failed: {err:#}");
        }
        let mut tick = tokio::time::interval(FRAME_INTERVAL);
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

        while !self.stop {
            tokio::select! {
                Some(conn) = incoming.recv() => self.on_event(Event::Connected(conn)),
                Some(ev) = events_rx.recv() => self.on_event(ev),
                Some(ev) = pane_rx.recv() => self.on_event(Event::Pane(ev)),
                _ = tick.tick() => self.on_tick(),
            }
            if self.had_workspaces && self.workspaces.is_empty() {
                tracing::info!("last workspace closed; exiting");
                self.stop = true;
            }
        }
        for client in self.clients.values() {
            let _ = client.tx.try_send(ServerMsg::Exit {
                reason: "server stopped".into(),
            });
        }
        if let Some(path) = self.opts.session_file.clone()
            && !self.workspaces.is_empty()
            && !self.protect_session_file
        {
            let _ = persist::save(&path, &self.snapshot());
        }
        // Give writer tasks a moment to flush the Exit messages.
        tokio::time::sleep(Duration::from_millis(50)).await;
        Ok(())
    }

    fn on_event(&mut self, ev: Event) {
        match ev {
            Event::Connected(conn) => self.accept(conn),
            Event::Message(id, msg) => self.on_message(id, msg),
            Event::Disconnected(id) => {
                self.pending.remove(&id);
                if self.clients.remove(&id).is_some() {
                    tracing::info!(client = id, "client left");
                    self.emit("client-detached", serde_json::json!({ "client": id }));
                    self.recompute_sizes();
                }
            }
            Event::Pane(PaneEvent::Output(id, bytes)) => {
                if let Some(pane) = self.panes.get_mut(&id) {
                    pane.process_output(&bytes);
                    for c in self.clients.values_mut() {
                        if let Some(sel) = c.select.as_mut().filter(|s| s.pane == id) {
                            sel.rebase(pane.emulator.as_ref());
                        }
                    }
                    self.mark_pane_dirty(id);
                }
            }
            Event::Pane(PaneEvent::Closed(id)) => {
                if self.panes.contains_key(&id) {
                    self.emit("pane-exited", serde_json::json!({ "pane": id }));
                    self.remove_pane(id);
                }
            }
            Event::ConfigChanged => {
                self.reload_at = Some(Instant::now() + Duration::from_millis(200))
            }
        }
    }
}

fn watch_config(
    path: &std::path::Path,
    events: mpsc::Sender<Event>,
) -> Option<notify::RecommendedWatcher> {
    use notify::Watcher;
    let dir = path.parent()?.to_path_buf();
    let _ = std::fs::create_dir_all(&dir);
    let file = path.file_name()?.to_owned();
    let mut watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
        // Editors save by rename, so watch the directory and match the name.
        if let Ok(ev) = res
            && ev
                .paths
                .iter()
                .any(|p| p.file_name() == Some(file.as_os_str()))
        {
            let _ = events.try_send(Event::ConfigChanged);
        }
    })
    .ok()?;
    watcher
        .watch(&dir, notify::RecursiveMode::NonRecursive)
        .ok()?;
    Some(watcher)
}
