//! The core state machine. See the crate docs for the concurrency model.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;
use rmx_config::Config;
use rmx_config::action::{Action, Dir, RunIn};
use rmx_config::keymap::{KeyEngine, KeyOutcome, Keymap};
use rmx_core::encode::{encode_key, encode_paste};
use rmx_core::layout::{PRESETS, PaneId, Side};
use rmx_proto::{Attach, ClientMsg, Hello, PROTOCOL_VERSION, Request, ServerEnd, ServerMsg};
use tokio::sync::mpsc;

use crate::compositor::{self, Compositor, View, tab_rects};
use crate::pane::{Pane, PaneEvent, SpawnSpec};
use crate::persist::{self, PaneSnap, Snapshot, TabSnap, WorkspaceSnap};
use crate::select::Select;
use crate::state::{Tab, Target, Workspace, WorkspaceId};
use crate::ui::{self, Cmd, Ctx, Effect, Outcome, Overlay, PromptThen};
use crate::{ServerOptions, api, mouse, tab_title};

/// Caps redraws at ~60 fps; output bursts between ticks coalesce into one frame.
const FRAME_INTERVAL: Duration = Duration::from_millis(16);
const AUTOSAVE_INTERVAL: Duration = Duration::from_secs(30);
const DEFAULT_SIZE: (u16, u16) = (80, 24);

pub type ClientId = u32;

enum Event {
    Connected(ServerEnd),
    Message(ClientId, ClientMsg),
    Disconnected(ClientId),
    Pane(PaneEvent),
    ConfigChanged,
}

struct Client {
    tx: mpsc::Sender<ServerMsg>,
    /// Where the user ran `rmx`: the session manager offers it for new workspaces.
    launch_cwd: Option<PathBuf>,
    size: (u16, u16),
    workspace: WorkspaceId,
    engine: KeyEngine,
    keymap: Keymap,
    enhanced_keys: bool,
    compositor: Compositor,
    overlays: Vec<Overlay>,
    hints: Option<Vec<(String, String)>>,
    toast: Option<(String, Instant, bool)>,
    select: Option<Select>,
    drag: Option<Drag>,
    dirty: bool,
}

enum Drag {
    Border {
        pane: PaneId,
        vertical: bool,
        last: u16,
    },
    Select,
    /// The program in the pane asked for mouse events; keep sending them.
    App(PaneId),
}

pub struct Core {
    opts: ServerOptions,
    config: Config,
    workspaces: Vec<Workspace>,
    panes: HashMap<PaneId, Pane>,
    clients: HashMap<ClientId, Client>,
    /// Pane-area size each workspace is laid out for.
    sizes: HashMap<WorkspaceId, (u16, u16)>,
    /// Interactive clients per workspace, for the session manager.
    attached: HashMap<WorkspaceId, usize>,
    subscribers: Vec<mpsc::Sender<ServerMsg>>,
    next_pane: PaneId,
    next_workspace: WorkspaceId,
    next_client: ClientId,
    clock: u64,
    paste_buffer: String,
    pane_tx: mpsc::Sender<PaneEvent>,
    pane_rx: Option<mpsc::Receiver<PaneEvent>>,
    events_tx: mpsc::Sender<Event>,
    events_rx: Option<mpsc::Receiver<Event>>,
    session_changed: bool,
    last_save: Instant,
    reload_at: Option<Instant>,
    had_workspaces: bool,
    stop: bool,
    /// Connections that haven't said Hello: control clients, or interactive
    /// ones still handshaking. Interactive ones move to `clients`.
    pending: HashMap<ClientId, mpsc::Sender<ServerMsg>>,
    /// Set after a partial restore: don't overwrite the session file until
    /// the user saves explicitly.
    protect_session_file: bool,
    _watcher: Option<notify::RecommendedWatcher>,
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

    /// Registers a connection; its messages arrive later as [`Event::Message`].
    fn accept(&mut self, mut conn: ServerEnd) {
        let id = self.next_client;
        self.next_client += 1;
        let events = self.events_tx.clone();
        let tx = conn.tx.clone();
        tokio::spawn(async move {
            while let Some(msg) = conn.rx.recv().await {
                if events.send(Event::Message(id, msg)).await.is_err() {
                    return;
                }
            }
            let _ = events.send(Event::Disconnected(id)).await;
        });
        // Not yet a `Client`: that happens on Hello. Control requests reply
        // through this sender.
        self.pending_conns().insert(id, tx);
    }

    fn pending_conns(&mut self) -> &mut HashMap<ClientId, mpsc::Sender<ServerMsg>> {
        &mut self.pending
    }

    fn on_message(&mut self, id: ClientId, msg: ClientMsg) {
        match msg {
            ClientMsg::Hello(hello) => self.hello(id, hello),
            ClientMsg::Request { version, .. } if version != PROTOCOL_VERSION => {
                if let Some(tx) = self.pending.get(&id) {
                    let body = format!(
                        "this rmx server is protocol v{PROTOCOL_VERSION} but the command is v{version}; \
                         run `rmx stop` and try again"
                    );
                    let _ = tx.try_send(ServerMsg::Response { ok: false, body });
                }
            }
            ClientMsg::Request { request, .. } => self.request(id, request),
            ClientMsg::Key(key) => self.on_key(id, key),
            ClientMsg::Mouse(ev) => self.on_mouse(id, ev),
            ClientMsg::Paste(text) => self.paste_text(id, &text),
            ClientMsg::Clipboard(text) => {
                let text = text
                    .filter(|t| !t.is_empty())
                    .unwrap_or_else(|| self.paste_buffer.clone());
                self.paste_text(id, &text);
            }
            ClientMsg::Resize { cols, rows } => {
                let (cols, rows) = clamp_size(cols, rows);
                if let Some(c) = self.clients.get_mut(&id) {
                    c.size = (cols, rows);
                    c.compositor.resize(cols, rows);
                    c.dirty = true;
                }
                self.recompute_sizes();
            }
            ClientMsg::Detach => self.detach(id),
        }
    }

    fn hello(&mut self, id: ClientId, mut hello: Hello) {
        let Some(tx) = self.pending_conns().remove(&id) else {
            return;
        };
        (hello.cols, hello.rows) = clamp_size(hello.cols, hello.rows);
        if hello.version != PROTOCOL_VERSION {
            let reason = format!(
                "rmx server is protocol v{PROTOCOL_VERSION} but this client is v{}; run `rmx stop` and try again",
                hello.version
            );
            let _ = tx.try_send(ServerMsg::Exit { reason });
            return;
        }
        let pane_size = pane_size_for(hello.cols, hello.rows);
        let workspace = match hello.attach {
            Attach::Last => match self.workspaces.iter().max_by_key(|w| w.last_used) {
                Some(w) => Ok(w.id),
                None => {
                    self.new_workspace(None, hello.cwd.clone().map(PathBuf::from), None, pane_size)
                }
            },
            Attach::Named(name) => self
                .workspaces
                .iter()
                .find(|w| w.name == name)
                .map(|w| w.id)
                .ok_or_else(|| anyhow::anyhow!("no workspace named \"{name}\" (see `rmx list`)")),
            Attach::New { name, cwd } => {
                self.new_workspace(name, cwd.map(PathBuf::from), None, pane_size)
            }
        };
        let workspace = match workspace {
            Ok(w) => w,
            Err(err) => {
                let _ = tx.try_send(ServerMsg::Exit {
                    reason: format!("{err:#}"),
                });
                return;
            }
        };
        let direct = self.config.direct_enabled(hello.enhanced_keys);
        let keymap = self.config.keymap(direct);
        let _ = tx.try_send(ServerMsg::Welcome {
            mouse: self.config.general.mouse,
        });
        let leader = keymap.leader().display_short();
        let mut client = Client {
            tx,
            launch_cwd: hello.cwd.map(PathBuf::from).filter(|d| d.is_dir()),
            size: (hello.cols, hello.rows),
            workspace,
            engine: KeyEngine::new(),
            keymap,
            enhanced_keys: hello.enhanced_keys,
            compositor: Compositor::new(hello.cols, hello.rows),
            overlays: Vec::new(),
            hints: None,
            toast: Some((
                format!("{leader} leader · F10 menu · F1 help"),
                Instant::now(),
                false,
            )),
            select: None,
            drag: None,
            dirty: true,
        };
        if hello.open_manager {
            let ctx = make_ctx(
                &self.config,
                &client.keymap,
                &self.workspaces,
                &self.panes,
                &self.attached,
                workspace,
                client.launch_cwd.as_deref(),
                &self.opts,
            );
            client
                .overlays
                .push(Overlay::Manager(ui::manager::Manager::new(&ctx)));
        }
        self.clients.insert(id, client);
        self.touch(workspace);
        tracing::info!(client = id, workspace, "client attached");
        self.emit(
            "client-attached",
            serde_json::json!({ "client": id, "workspace": workspace }),
        );
        self.recompute_sizes();
    }

    fn detach(&mut self, id: ClientId) {
        if let Some(c) = self.clients.remove(&id) {
            let _ = c.tx.try_send(ServerMsg::Exit {
                reason: "detached".into(),
            });
            self.emit("client-detached", serde_json::json!({ "client": id }));
            self.recompute_sizes();
        }
    }

    // ---- workspaces, tabs, panes -------------------------------------------------

    fn touch(&mut self, ws: WorkspaceId) {
        self.clock += 1;
        let clock = self.clock;
        if let Some(w) = self.workspace_mut(ws) {
            w.last_used = clock;
        }
    }

    fn workspace(&self, id: WorkspaceId) -> Option<&Workspace> {
        self.workspaces.iter().find(|w| w.id == id)
    }

    fn workspace_mut(&mut self, id: WorkspaceId) -> Option<&mut Workspace> {
        self.workspaces.iter_mut().find(|w| w.id == id)
    }

    fn spawn_pane(&mut self, spec: SpawnSpec, size: (u16, u16)) -> Result<PaneId> {
        let id = self.next_pane;
        let mut spec = SpawnSpec {
            shell: spec.shell.or_else(|| self.config.general.shell.clone()),
            ..spec
        };
        // `rmx ls` etc. inside a pane should talk to *this* server (like $TMUX).
        spec.env.push(("RMX_SERVER".into(), self.opts.name.clone()));
        let pane = Pane::spawn(
            id,
            &spec,
            size.1.max(1),
            size.0.max(1),
            self.config.general.scrollback,
            self.pane_tx.clone(),
        )?;
        self.next_pane += 1;
        self.panes.insert(id, pane);
        self.emit("pane-created", serde_json::json!({ "pane": id }));
        Ok(id)
    }

    fn unique_name(&self, wanted: Option<String>) -> String {
        let taken = |n: &str| self.workspaces.iter().any(|w| w.name == n);
        if let Some(name) = wanted.filter(|n| !n.trim().is_empty()) {
            let name = name.trim().to_owned();
            if !taken(&name) {
                return name;
            }
            return (2..)
                .map(|i| format!("{name}-{i}"))
                .find(|n| !taken(n))
                .unwrap();
        }
        let first = if self.workspaces.is_empty() {
            "main".to_owned()
        } else {
            String::new()
        };
        if !first.is_empty() && !taken(&first) {
            return first;
        }
        (1..).map(|i| i.to_string()).find(|n| !taken(n)).unwrap()
    }

    fn new_workspace(
        &mut self,
        name: Option<String>,
        cwd: Option<PathBuf>,
        command: Option<String>,
        size: (u16, u16),
    ) -> Result<WorkspaceId> {
        let cwd_for_name = cwd.clone();
        let pane = self.spawn_pane(
            SpawnSpec {
                cwd,
                command,
                ..Default::default()
            },
            size,
        )?;
        let id = self.next_workspace;
        self.next_workspace += 1;
        // Unnamed workspaces are named after their folder ("api", "rust-mux"),
        // which is what people look for in the session manager.
        let name = self.unique_name(name.or_else(|| folder_name(cwd_for_name.as_deref())));
        self.workspaces.push(Workspace {
            id,
            name: name.clone(),
            tabs: vec![Tab::new(pane)],
            active_tab: 0,
            last_tab: 0,
            last_used: 0,
        });
        self.sizes.insert(id, size);
        self.had_workspaces = true;
        self.session_changed = true;
        self.emit(
            "workspace-created",
            serde_json::json!({ "workspace": id, "name": name }),
        );
        Ok(id)
    }

    fn active_pane_of(&self, ws: WorkspaceId) -> Option<PaneId> {
        self.workspace(ws).map(|w| w.active().active_pane)
    }

    fn cwd_of_active(&self, ws: WorkspaceId) -> Option<PathBuf> {
        self.active_pane_of(ws)
            .and_then(|p| self.panes.get(&p))
            .map(Pane::cwd)
    }

    fn new_tab(&mut self, ws: WorkspaceId, command: Option<String>) -> Result<()> {
        let size = self.sizes.get(&ws).copied().unwrap_or(DEFAULT_SIZE);
        let cwd = self.cwd_of_active(ws);
        let pane = self.spawn_pane(
            SpawnSpec {
                cwd,
                command,
                ..Default::default()
            },
            size,
        )?;
        let w = self.workspace_mut(ws).expect("workspace exists");
        let at = (w.active_tab + 1).min(w.tabs.len());
        w.tabs.insert(at, Tab::new(pane));
        w.last_tab = w.active_tab;
        w.active_tab = at;
        self.session_changed = true;
        self.mark_workspace_dirty(ws);
        Ok(())
    }

    fn split(
        &mut self,
        ws: WorkspaceId,
        tab: usize,
        dir: Dir,
        percent: u16,
        command: Option<String>,
    ) -> Result<()> {
        let size = self.sizes.get(&ws).copied().unwrap_or(DEFAULT_SIZE);
        let Some(target) = self
            .workspace(ws)
            .and_then(|w| w.tabs.get(tab))
            .map(|t| t.active_pane)
        else {
            return Ok(());
        };
        let cwd = self.panes.get(&target).map(Pane::cwd);
        let estimate = (size.0 / 2, size.1 / 2);
        let pane = self.spawn_pane(
            SpawnSpec {
                cwd,
                command,
                ..Default::default()
            },
            estimate,
        )?;
        let w = self.workspace_mut(ws).expect("workspace exists");
        let t = &mut w.tabs[tab];
        t.zoomed = false;
        t.layout.split(target, pane, side(dir), percent);
        t.active_pane = pane;
        self.session_changed = true;
        self.relayout(ws);
        Ok(())
    }

    /// Removes a pane from wherever it is, closing its tab and workspace if
    /// it was the last; moves clients off a closed workspace.
    fn remove_pane(&mut self, pane: PaneId) {
        self.panes.remove(&pane);
        for client in self.clients.values_mut() {
            if client.select.as_ref().is_some_and(|s| s.pane == pane) {
                client.select = None;
                client.engine.exit_mode();
            }
        }
        let Some(wi) = self
            .workspaces
            .iter()
            .position(|w| w.tab_of(pane).is_some())
        else {
            return;
        };
        let ws_id = self.workspaces[wi].id;
        let ti = self.workspaces[wi].tab_of(pane).unwrap();
        let tab = &mut self.workspaces[wi].tabs[ti];
        if tab.layout.remove(pane) {
            if tab.active_pane == pane {
                tab.active_pane = tab.layout.panes()[0];
            }
            tab.zoomed = false;
        } else {
            self.workspaces[wi].remove_tab(ti);
        }
        self.session_changed = true;
        if self.workspaces[wi].tabs.is_empty() {
            self.close_workspace(ws_id);
        } else {
            self.relayout(ws_id);
        }
    }

    fn close_tab(&mut self, ws: WorkspaceId, tab: usize) {
        let panes = self
            .workspace(ws)
            .and_then(|w| w.tabs.get(tab))
            .map(|t| t.layout.panes())
            .unwrap_or_default();
        for p in panes {
            self.remove_pane(p);
        }
    }

    fn close_workspace(&mut self, ws: WorkspaceId) {
        let Some(i) = self.workspaces.iter().position(|w| w.id == ws) else {
            return;
        };
        let removed = self.workspaces.remove(i);
        for p in removed.pane_ids() {
            self.panes.remove(&p);
        }
        self.sizes.remove(&ws);
        self.session_changed = true;
        self.emit(
            "workspace-closed",
            serde_json::json!({ "workspace": ws, "name": removed.name }),
        );
        // Clients move to the most recently used workspace, or leave.
        let fallback = self
            .workspaces
            .iter()
            .max_by_key(|w| w.last_used)
            .map(|w| w.id);
        let ids: Vec<ClientId> = self
            .clients
            .iter()
            .filter(|(_, c)| c.workspace == ws)
            .map(|(id, _)| *id)
            .collect();
        for id in ids {
            match fallback {
                Some(to) => self.switch_workspace(id, to),
                None => {
                    if let Some(c) = self.clients.remove(&id) {
                        let _ = c.tx.try_send(ServerMsg::Exit {
                            reason: "workspace closed".into(),
                        });
                        self.emit("client-detached", serde_json::json!({ "client": id }));
                        self.recompute_sizes();
                    }
                }
            }
        }
    }

    fn switch_workspace(&mut self, client: ClientId, ws: WorkspaceId) {
        if let Some(c) = self.clients.get_mut(&client) {
            end_select(c, &mut self.panes);
            c.workspace = ws;
            c.dirty = true;
        }
        self.touch(ws);
        self.recompute_sizes();
    }

    /// Each workspace is laid out for the smallest client viewing it, so
    /// every client sees the whole thing (tmux's `window-size smallest`).
    fn recompute_sizes(&mut self) {
        self.attached = self.clients_by_ws();
        let mut sizes: HashMap<WorkspaceId, (u16, u16)> = HashMap::new();
        for c in self.clients.values() {
            let area = compositor::pane_area(Rect::new(0, 0, c.size.0, c.size.1));
            let s = sizes
                .entry(c.workspace)
                .or_insert((area.width, area.height));
            *s = (s.0.min(area.width), s.1.min(area.height));
        }
        for (ws, size) in sizes {
            if self.sizes.get(&ws) != Some(&size) {
                self.sizes.insert(ws, size);
            }
        }
        let ids: Vec<WorkspaceId> = self.workspaces.iter().map(|w| w.id).collect();
        for ws in ids {
            self.relayout(ws);
        }
    }

    /// Resizes every pane of a workspace to its layout and marks viewers dirty.
    fn relayout(&mut self, ws: WorkspaceId) {
        let (w, h) = self.sizes.get(&ws).copied().unwrap_or(DEFAULT_SIZE);
        let area = Rect::new(0, 0, w.max(1), h.max(1));
        if let Some(workspace) = self.workspaces.iter().find(|x| x.id == ws) {
            for tab in &workspace.tabs {
                for (id, rect) in tab_rects(tab, area).0 {
                    if let Some(p) = self.panes.get_mut(&id) {
                        p.resize(rect.height, rect.width);
                    }
                }
            }
        }
        self.mark_workspace_dirty(ws);
    }

    fn mark_workspace_dirty(&mut self, ws: WorkspaceId) {
        for c in self.clients.values_mut().filter(|c| c.workspace == ws) {
            c.dirty = true;
        }
    }

    fn mark_pane_dirty(&mut self, pane: PaneId) {
        // Viewers of the pane's workspace, plus anyone with an overlay open
        // (the session manager previews any workspace).
        let ws = self
            .workspaces
            .iter()
            .find(|w| w.tab_of(pane).is_some())
            .map(|w| w.id);
        for c in self.clients.values_mut() {
            if Some(c.workspace) == ws || !c.overlays.is_empty() {
                c.dirty = true;
            }
        }
    }

    // ---- input ---------------------------------------------------------------------

    fn on_key(&mut self, id: ClientId, key: KeyEvent) {
        let Some(client) = self.clients.get_mut(&id) else {
            return;
        };
        client.dirty = true;
        if !client.overlays.is_empty() {
            let ctx = make_ctx(
                &self.config,
                &client.keymap,
                &self.workspaces,
                &self.panes,
                &self.attached,
                client.workspace,
                client.launch_cwd.as_deref(),
                &self.opts,
            );
            let outcome = client.overlays.last_mut().unwrap().handle_key(&key, &ctx);
            self.apply_outcome(id, outcome);
            return;
        }
        let outcome = client.engine.feed(&client.keymap, &key, Instant::now());
        client.hints = None;
        match outcome {
            KeyOutcome::Action(action) => self.dispatch(id, action),
            KeyOutcome::Pending { hints } => {
                if self.config.leader.show_hints {
                    client.hints = Some(hints);
                }
            }
            KeyOutcome::PassThrough => self.key_to_pane(id, &key),
            KeyOutcome::Ignored => {}
        }
        self.sync_select(id);
    }

    /// Ends select state when the key engine left select mode (Esc etc.).
    fn sync_select(&mut self, id: ClientId) {
        let Some(c) = self.clients.get_mut(&id) else {
            return;
        };
        // Also when focus moved elsewhere (tab switch, click, close): select
        // state belongs to the pane it started in.
        let active = self
            .workspaces
            .iter()
            .find(|w| w.id == c.workspace)
            .map(|w| w.active().active_pane);
        let elsewhere = c.select.as_ref().is_some_and(|s| Some(s.pane) != active);
        if c.select.is_some() && (c.engine.mode() != "select" || elsewhere) {
            end_select(c, &mut self.panes);
        }
    }

    fn key_to_pane(&mut self, id: ClientId, key: &KeyEvent) {
        let Some(c) = self.clients.get(&id) else {
            return;
        };
        let Some(ws) = self.workspace(c.workspace) else {
            return;
        };
        let tab = ws.active();
        let targets = if tab.sync_input {
            tab.layout.panes()
        } else {
            vec![tab.active_pane]
        };
        for p in targets {
            if let Some(pane) = self.panes.get_mut(&p)
                && let Some(bytes) = encode_key(key, pane.emulator.application_cursor())
            {
                // Typing snaps a scrolled-back view to the live screen.
                pane.emulator.set_view_offset(0);
                pane.write(bytes);
            }
        }
    }

    fn paste_text(&mut self, id: ClientId, text: &str) {
        if text.is_empty() {
            return;
        }
        if let Some(c) = self.clients.get_mut(&id)
            && let Some(top) = c.overlays.last_mut()
        {
            // Only into a text field; never replayed as keys, where a pasted
            // "y" would answer a confirmation.
            top.paste(text);
            c.dirty = true;
            return;
        }
        if let Some(c) = self.clients.get_mut(&id) {
            c.engine.exit_mode();
        }
        self.sync_select(id);
        if let Some(pane) = self.client_pane(id).and_then(|p| self.panes.get_mut(&p)) {
            pane.emulator.set_view_offset(0);
            let bytes = encode_paste(text, pane.emulator.bracketed_paste());
            pane.write(bytes);
        }
    }

    fn client_pane(&self, id: ClientId) -> Option<PaneId> {
        self.clients
            .get(&id)
            .and_then(|c| self.active_pane_of(c.workspace))
    }

    fn apply_outcome(&mut self, id: ClientId, outcome: Outcome) {
        if let Some(c) = self.clients.get_mut(&id) {
            c.dirty = true;
            if outcome.pop {
                c.overlays.pop();
            }
        }
        match outcome.effect {
            Some(Effect::Action(action)) => self.dispatch(id, action),
            Some(Effect::Cmd(cmd)) => self.run_cmd(id, cmd),
            None => {}
        }
    }

    fn push_overlay(&mut self, id: ClientId, make: impl FnOnce(&Ctx) -> Overlay) {
        let Some(c) = self.clients.get_mut(&id) else {
            return;
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
        let overlay = make(&ctx);
        c.overlays.push(overlay);
        c.dirty = true;
    }

    fn toast(&mut self, id: ClientId, text: impl Into<String>, error: bool) {
        if let Some(c) = self.clients.get_mut(&id) {
            c.toast = Some((text.into(), Instant::now(), error));
            c.dirty = true;
        }
    }

    fn view_rect(&self, ws: WorkspaceId) -> Rect {
        let (w, h) = self.sizes.get(&ws).copied().unwrap_or(DEFAULT_SIZE);
        Rect::new(0, 0, w, h)
    }

    // ---- actions ---------------------------------------------------------------------

    fn dispatch(&mut self, id: ClientId, action: Action) {
        let Some(c) = self.clients.get(&id) else {
            return;
        };
        let ws = c.workspace;
        let Some(w) = self.workspace(ws) else { return };
        let tab_idx = w.active_tab;
        let pane = w.active().active_pane;
        let area = self.view_rect(ws);
        let result: Result<()> = (|| {
            match action {
                Action::NewTab => self.new_tab(ws, None)?,
                Action::CloseTab => self.close_tab(ws, tab_idx),
                Action::RenameTab => {
                    let initial = self
                        .workspace(ws)
                        .map(|w| tab_title(w.active(), &self.panes))
                        .unwrap_or_default();
                    self.push_prompt(
                        id,
                        "Rename tab",
                        initial,
                        PromptThen::RenameTab {
                            workspace: ws,
                            tab: self
                                .workspace(ws)
                                .map(|w| w.active().id)
                                .unwrap_or_default(),
                        },
                    );
                }
                Action::NextTab | Action::PrevTab => {
                    let w = self.workspace_mut(ws).unwrap();
                    let n = w.tabs.len();
                    let next = if action == Action::NextTab {
                        (w.active_tab + 1) % n
                    } else {
                        (w.active_tab + n - 1) % n
                    };
                    w.select_tab(next);
                }
                Action::GoToTab { index } => {
                    let w = self.workspace_mut(ws).unwrap();
                    w.select_tab(index.saturating_sub(1).min(w.tabs.len() - 1));
                }
                Action::MoveTab { delta } => {
                    let w = self.workspace_mut(ws).unwrap();
                    let to = (w.active_tab as i64 + i64::from(delta))
                        .clamp(0, w.tabs.len() as i64 - 1) as usize;
                    w.tabs.swap(w.active_tab, to);
                    w.active_tab = to;
                    self.session_changed = true;
                }
                Action::SplitPane {
                    direction,
                    size,
                    command,
                } => {
                    self.split(ws, tab_idx, direction, size.unwrap_or(50), command)?;
                }
                Action::ClosePane => self.remove_pane(pane),
                Action::MoveFocus { direction } => {
                    let t = self.workspace_mut(ws).unwrap().active_mut();
                    t.zoomed = false;
                    if let Some(n) = t.layout.neighbor(pane, side(direction), area) {
                        t.active_pane = n;
                    }
                    self.relayout(ws);
                }
                Action::ResizePane { direction, amount } => {
                    let t = self.workspace_mut(ws).unwrap().active_mut();
                    if t.layout.resize(pane, side(direction), amount, area) {
                        self.session_changed = true;
                        self.relayout(ws);
                    }
                }
                Action::TogglePaneZoom => {
                    let t = self.workspace_mut(ws).unwrap().active_mut();
                    t.zoomed = !t.zoomed && t.layout.panes().len() > 1;
                    self.relayout(ws);
                }
                Action::SwapPane { direction } => {
                    let t = self.workspace_mut(ws).unwrap().active_mut();
                    t.layout.swap(pane, side(direction), area);
                    self.session_changed = true;
                    self.relayout(ws);
                }
                Action::ApplyLayout { name } => {
                    let t = self.workspace_mut(ws).unwrap().active_mut();
                    let name = if name == "next" {
                        let n = PRESETS[t.preset_cursor % PRESETS.len()];
                        t.preset_cursor += 1;
                        n.to_owned()
                    } else {
                        name
                    };
                    t.zoomed = false;
                    if !t.layout.apply_preset(&name) {
                        anyhow::bail!("unknown layout \"{name}\" (try: {})", PRESETS.join(", "));
                    }
                    self.session_changed = true;
                    self.relayout(ws);
                }
                Action::ToggleSyncInput => {
                    let t = self.workspace_mut(ws).unwrap().active_mut();
                    t.sync_input = !t.sync_input;
                    let on = t.sync_input;
                    self.toast(
                        id,
                        if on {
                            "Typing goes to all panes in this tab"
                        } else {
                            "Typing goes to the focused pane"
                        },
                        false,
                    );
                }
                Action::SelectMode => self.enter_select(id, pane),
                Action::Find => self.push_prompt(id, "Find", String::new(), PromptThen::Find),
                Action::FindNext | Action::FindPrev => {
                    let forward = action == Action::FindPrev;
                    self.with_select(id, pane, |sel, emu, _| {
                        if let Some(q) = sel.search.clone() {
                            sel.find(&q, forward, emu);
                        }
                    });
                }
                Action::Copy | Action::CopyAndExit => {
                    self.copy_selection(id);
                    if action == Action::CopyAndExit {
                        self.exit_select(id);
                    }
                }
                Action::Paste => {
                    if let Some(c) = self.clients.get(&id) {
                        let _ = c.tx.try_send(ServerMsg::GetClipboard);
                    }
                }
                Action::SelectMove { direction } => {
                    self.with_select(id, pane, |sel, emu, cols| {
                        sel.move_cursor(direction, cols, emu)
                    });
                }
                Action::SelectExtend { direction } => {
                    self.with_select(id, pane, |sel, emu, cols| sel.extend(direction, cols, emu));
                }
                Action::SelectLineStart => self.with_select(id, pane, |sel, _, _| sel.line_start()),
                Action::SelectLineEnd => {
                    self.with_select(id, pane, |sel, emu, _| sel.line_end(emu))
                }
                Action::SelectStart => self.with_select(id, pane, |sel, _, _| sel.start(false)),
                Action::SelectLine => self.with_select(id, pane, |sel, _, _| sel.start(true)),
                Action::SelectWordLeft => {
                    self.with_select(id, pane, |sel, emu, _| sel.move_word(false, emu))
                }
                Action::SelectWordRight => {
                    self.with_select(id, pane, |sel, emu, _| sel.move_word(true, emu))
                }
                Action::ScrollUp { lines } => {
                    self.scroll(id, pane, isize::from(lines as i16).saturating_neg())
                }
                Action::ScrollDown { lines } => self.scroll(id, pane, lines as isize),
                Action::ScrollPageUp => {
                    self.scroll(id, pane, -(isize::from(area.height as i16) - 1))
                }
                Action::ScrollPageDown => {
                    self.scroll(id, pane, isize::from(area.height as i16) - 1)
                }
                Action::ScrollToTop => self.with_select(id, pane, |sel, emu, _| sel.go_top(emu)),
                Action::ScrollToBottom => {
                    self.with_select(id, pane, |sel, emu, _| sel.go_bottom(emu))
                }
                Action::CommandPalette => {
                    self.push_overlay(id, |_| Overlay::Palette(Default::default()))
                }
                Action::OpenMenu => {
                    self.push_overlay(id, |_| Overlay::Menu(ui::menu::MenuBar::new(0)))
                }
                Action::ShowHelp => self.push_overlay(id, |_| Overlay::Help(Default::default())),
                Action::WorkspaceSwitcher => {
                    self.push_overlay(id, |ctx| Overlay::Manager(ui::manager::Manager::new(ctx)))
                }
                Action::NewWorkspace => {
                    let size = self.sizes.get(&ws).copied().unwrap_or(DEFAULT_SIZE);
                    let cwd = self.cwd_of_active(ws);
                    let new = self.new_workspace(None, cwd, None, size)?;
                    self.switch_workspace(id, new);
                }
                Action::RenameWorkspace => {
                    let initial = self
                        .workspace(ws)
                        .map(|w| w.name.clone())
                        .unwrap_or_default();
                    self.push_prompt(
                        id,
                        "Rename workspace",
                        initial,
                        PromptThen::RenameWorkspace { workspace: ws },
                    );
                }
                Action::NextWorkspace | Action::PrevWorkspace => {
                    let i = self.workspaces.iter().position(|w| w.id == ws).unwrap_or(0);
                    let n = self.workspaces.len();
                    let j = if action == Action::NextWorkspace {
                        (i + 1) % n
                    } else {
                        (i + n - 1) % n
                    };
                    let to = self.workspaces[j].id;
                    self.switch_workspace(id, to);
                }
                Action::Detach => self.detach(id),
                Action::Quit => self.run_cmd(
                    id,
                    Cmd::Confirm {
                        message: "Quit rmx and close every workspace?".into(),
                        then: Box::new(Cmd::QuitServer),
                    },
                ),
                Action::ReloadConfig => self.reload_config(Some(id)),
                Action::OpenConfig => {
                    let path = self.opts.config_path.clone();
                    if !path.exists() {
                        if let Some(dir) = path.parent() {
                            std::fs::create_dir_all(dir)?;
                        }
                        std::fs::write(&path, rmx_config::DEFAULT_CONFIG_TOML)?;
                    }
                    self.new_tab(ws, Some(editor_command(&path)))?;
                }
                Action::EnterMode { mode, sticky } => {
                    if let Some(c) = self.clients.get_mut(&id) {
                        c.engine.set_mode(&mode, sticky);
                    }
                }
                Action::ExitMode => {
                    if let Some(c) = self.clients.get_mut(&id) {
                        c.engine.exit_mode();
                    }
                }
                Action::Lock => {
                    if let Some(c) = self.clients.get_mut(&id) {
                        c.engine.set_mode("locked", true);
                    }
                    let leader = self
                        .clients
                        .get(&id)
                        .map(|c| c.keymap.leader().display_short())
                        .unwrap_or_default();
                    self.toast(
                        id,
                        format!("Keys locked to the pane · {leader} to unlock"),
                        false,
                    );
                }
                Action::Run { command, r#in } => match r#in {
                    RunIn::NewTab => self.new_tab(ws, Some(command))?,
                    RunIn::SplitRight => self.split(ws, tab_idx, Dir::Right, 50, Some(command))?,
                    RunIn::SplitDown => self.split(ws, tab_idx, Dir::Down, 50, Some(command))?,
                    RunIn::Background => run_background(&command, self.cwd_of_active(ws))?,
                },
                Action::SendKeys { keys } => {
                    if let Some(p) = self.panes.get(&pane) {
                        p.write(unescape(&keys).into_bytes());
                    }
                }
            }
            Ok(())
        })();
        if let Err(err) = result {
            tracing::warn!("action failed: {err:#}");
            self.toast(id, format!("{err:#}"), true);
        }
        self.sync_select(id);
        self.mark_workspace_dirty(ws);
    }

    fn push_prompt(&mut self, id: ClientId, title: &str, initial: String, then: PromptThen) {
        let title = title.to_owned();
        self.push_overlay(id, move |_| {
            Overlay::Prompt(ui::dialog::Prompt::new(title, &initial, then))
        });
    }

    fn enter_select(&mut self, id: ClientId, pane: PaneId) {
        let Some(c) = self.clients.get_mut(&id) else {
            return;
        };
        c.engine.set_mode("select", true);
        if c.select.as_ref().is_none_or(|s| s.pane != pane)
            && let Some(p) = self.panes.get(&pane)
        {
            c.select = Some(Select::new(pane, p.emulator.as_ref()));
        }
    }

    fn exit_select(&mut self, id: ClientId) {
        if let Some(c) = self.clients.get_mut(&id) {
            c.engine.exit_mode();
        }
        self.sync_select(id);
    }

    /// Runs `f` on the client's select state, entering select mode first.
    fn with_select(
        &mut self,
        id: ClientId,
        pane: PaneId,
        f: impl FnOnce(&mut Select, &mut dyn rmx_core::emulator::Emulator, u16),
    ) {
        self.enter_select(id, pane);
        let Some(c) = self.clients.get_mut(&id) else {
            return;
        };
        let (Some(sel), Some(p)) = (c.select.as_mut(), self.panes.get_mut(&pane)) else {
            return;
        };
        // The pane's own width, so the cursor can't leave a narrow pane.
        let cols = p.emulator.cols();
        f(sel, p.emulator.as_mut(), cols);
    }

    fn scroll(&mut self, id: ClientId, pane: PaneId, delta: isize) {
        let in_select = self.clients.get(&id).is_some_and(|c| c.select.is_some());
        if in_select {
            self.with_select(id, pane, |sel, emu, _| sel.scroll(delta, emu));
        } else if let Some(p) = self.panes.get_mut(&pane) {
            let offset = p.emulator.view_offset().saturating_add_signed(-delta);
            p.emulator
                .set_view_offset(offset.min(p.emulator.history_len()));
        }
    }

    fn copy_selection(&mut self, id: ClientId) {
        let Some(c) = self.clients.get_mut(&id) else {
            return;
        };
        let Some(sel) = c.select.as_ref() else { return };
        let Some(p) = self.panes.get_mut(&sel.pane) else {
            return;
        };
        let Some(text) = sel.text(p.emulator.as_mut()).filter(|t| !t.is_empty()) else {
            return;
        };
        let n = text.chars().count();
        let _ = c.tx.try_send(ServerMsg::SetClipboard(text.clone()));
        self.paste_buffer = text;
        self.toast(
            id,
            format!("Copied {n} character{}", if n == 1 { "" } else { "s" }),
            false,
        );
    }

    fn run_cmd(&mut self, id: ClientId, cmd: Cmd) {
        let result: Result<()> = (|| {
            match cmd {
                Cmd::RenameTab {
                    workspace,
                    tab,
                    name,
                } => {
                    if let Some(t) = self
                        .workspace_mut(workspace)
                        .and_then(|w| w.tab_index(tab).map(|i| &mut w.tabs[i]))
                    {
                        t.custom_name = Some(name);
                        self.session_changed = true;
                    }
                }
                Cmd::RenameWorkspace { workspace, name } => {
                    if self
                        .workspaces
                        .iter()
                        .any(|w| w.name == name && w.id != workspace)
                    {
                        anyhow::bail!("a workspace named \"{name}\" already exists");
                    }
                    if let Some(w) = self.workspace_mut(workspace) {
                        w.name = name.clone();
                        self.session_changed = true;
                        self.emit(
                            "workspace-renamed",
                            serde_json::json!({ "workspace": workspace, "name": name }),
                        );
                    }
                }
                Cmd::NewWorkspace { name, cwd } => {
                    let ws = self.clients.get(&id).map(|c| c.workspace).unwrap_or(0);
                    let size = self.sizes.get(&ws).copied().unwrap_or(DEFAULT_SIZE);
                    let cwd = cwd.or_else(|| self.cwd_of_active(ws));
                    let new = self.new_workspace(name, cwd, None, size)?;
                    self.switch_workspace(id, new);
                }
                Cmd::Find(query) => {
                    if let Some(pane) = self.client_pane(id) {
                        self.with_select(id, pane, |sel, emu, _| {
                            sel.find(&query, false, emu);
                        });
                    }
                }
                Cmd::Open {
                    workspace,
                    tab,
                    pane,
                } => {
                    if let Some(w) = self.workspace_mut(workspace) {
                        if let Some(i) = tab.and_then(|t| w.tab_index(t)) {
                            w.select_tab(i);
                        }
                        if let Some(p) = pane {
                            let t = w.active_mut();
                            if t.layout.contains(p) {
                                t.active_pane = p;
                            }
                        }
                    }
                    self.switch_workspace(id, workspace);
                }
                Cmd::NewTab { workspace } => self.new_tab(workspace, None)?,
                Cmd::Split {
                    workspace,
                    tab,
                    right,
                } => {
                    let Some(tab) = self.workspace(workspace).and_then(|w| w.tab_index(tab)) else {
                        return Ok(());
                    };
                    self.split(
                        workspace,
                        tab,
                        if right { Dir::Right } else { Dir::Down },
                        50,
                        None,
                    )?;
                }
                Cmd::CloseWorkspace { workspace } => self.close_workspace(workspace),
                Cmd::CloseTab { workspace, tab } => {
                    if let Some(i) = self.workspace(workspace).and_then(|w| w.tab_index(tab)) {
                        self.close_tab(workspace, i);
                    }
                }
                Cmd::Confirm { message, then } => {
                    self.push_overlay(id, move |_| {
                        Overlay::Confirm(ui::dialog::Confirm::new(message, *then))
                    });
                }
                Cmd::Prompt {
                    title,
                    initial,
                    then,
                } => self.push_prompt(id, &title, initial, then),
                Cmd::QuitServer => self.stop = true,
            }
            Ok(())
        })();
        if let Err(err) = result {
            self.toast(id, format!("{err:#}"), true);
        }
        let ids: Vec<WorkspaceId> = self.workspaces.iter().map(|w| w.id).collect();
        for ws in ids {
            self.mark_workspace_dirty(ws);
        }
        if let Some(c) = self.clients.get_mut(&id) {
            c.dirty = true;
        }
    }

    // ---- mouse -----------------------------------------------------------------------

    fn on_mouse(&mut self, id: ClientId, ev: MouseEvent) {
        let Some(c) = self.clients.get_mut(&id) else {
            return;
        };
        c.dirty = true;
        let screen = Rect::new(0, 0, c.size.0, c.size.1);
        if !c.overlays.is_empty() {
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
            let outcome = c
                .overlays
                .last_mut()
                .unwrap()
                .handle_mouse(&ev, &ctx, screen);
            self.apply_outcome(id, outcome);
            return;
        }
        let ws_id = c.workspace;
        let (x, y) = (ev.column, ev.row);
        let left_down = matches!(ev.kind, MouseEventKind::Down(MouseButton::Left));

        let pane_area = compositor::pane_area(screen);
        let (vw, vh) = self.sizes.get(&ws_id).copied().unwrap_or(DEFAULT_SIZE);
        let area = Rect::new(
            pane_area.x,
            pane_area.y,
            vw.min(pane_area.width),
            vh.min(pane_area.height),
        );
        let Some(ws) = self.workspace(ws_id) else {
            return;
        };
        let active_pane = ws.active().active_pane;
        let (rects, seps) = tab_rects(ws.active(), area);
        let hit = rects.iter().find(|(_, r)| ui::contains(*r, x, y)).copied();

        // Continue an ongoing drag first: it owns the mouse until release.
        let drag = self.clients.get_mut(&id).and_then(|c| c.drag.take());
        match (drag, ev.kind) {
            (
                Some(Drag::Border {
                    pane,
                    vertical,
                    last,
                }),
                MouseEventKind::Drag(MouseButton::Left),
            ) => {
                let pos = if vertical { x } else { y };
                let delta = i32::from(pos) - i32::from(last);
                if delta != 0 {
                    let side = match (vertical, delta > 0) {
                        (true, true) => Side::Right,
                        (true, false) => Side::Left,
                        (false, true) => Side::Down,
                        (false, false) => Side::Up,
                    };
                    let view = self.view_rect(ws_id);
                    if let Some(t) = self.workspace_mut(ws_id).map(Workspace::active_mut) {
                        t.layout
                            .resize(pane, side, delta.unsigned_abs() as u16, view);
                    }
                    self.session_changed = true;
                    self.relayout(ws_id);
                }
                self.clients.get_mut(&id).unwrap().drag = Some(Drag::Border {
                    pane,
                    vertical,
                    last: pos,
                });
                return;
            }
            (Some(Drag::Border { .. }), MouseEventKind::Up(_)) => return,
            (Some(Drag::Select), MouseEventKind::Drag(MouseButton::Left)) => {
                self.drag_select(id, active_pane, area, &rects, x, y);
                self.clients.get_mut(&id).unwrap().drag = Some(Drag::Select);
                return;
            }
            (Some(Drag::Select), MouseEventKind::Up(MouseButton::Left)) => {
                let (has, by_mouse) = self
                    .clients
                    .get(&id)
                    .and_then(|c| c.select.as_ref())
                    .map(|s| (s.anchor.is_some_and(|a| a != s.cursor), s.by_mouse))
                    .unwrap_or((false, false));
                if has && self.config.general.copy_on_select {
                    self.copy_selection(id);
                }
                if by_mouse && (!has || self.config.general.copy_on_select) {
                    self.exit_select(id);
                }
                return;
            }
            (Some(Drag::App(pane)), kind) => {
                let rect = rects.iter().find(|(p, _)| *p == pane).map(|(_, r)| *r);
                if let Some(rect) = rect {
                    self.forward_mouse(pane, &ev, rect);
                }
                if !matches!(kind, MouseEventKind::Up(_)) {
                    self.clients.get_mut(&id).unwrap().drag = Some(Drag::App(pane));
                }
                return;
            }
            _ => {}
        }

        // Bars (after drags, so a drag released over a bar still ends
        // properly). Menu bar:
        if y == 0 {
            if left_down {
                let menus = ui::menu::menus();
                if let Some(i) = ui::menu::title_positions(&menus)
                    .iter()
                    .position(|&(tx, w)| x >= tx && x < tx + w)
                {
                    self.push_overlay(id, |_| Overlay::Menu(ui::menu::MenuBar::new(i)));
                }
            }
            return;
        }
        // Status bar: click a tab, wheel through tabs.
        if y == screen.bottom().saturating_sub(1) {
            match ev.kind {
                MouseEventKind::Down(MouseButton::Left) => {
                    let hit = self.workspace(ws_id).and_then(|w| {
                        compositor::tab_spans(w, &self.panes)
                            .iter()
                            .position(|&(tx, tw, _)| x >= tx && x < tx + tw)
                    });
                    if let Some(i) = hit {
                        self.dispatch(id, Action::GoToTab { index: i + 1 });
                    }
                }
                MouseEventKind::ScrollUp => self.dispatch(id, Action::PrevTab),
                MouseEventKind::ScrollDown => self.dispatch(id, Action::NextTab),
                _ => {}
            }
            return;
        }

        // Grab a border to resize.
        if left_down
            && let Some(sep) = seps.iter().find(|s| {
                if s.vertical {
                    x == s.x && y >= s.y && y < s.y + s.len
                } else {
                    y == s.y && x >= s.x && x < s.x + s.len
                }
            })
        {
            // The pane just before the border (left of it / above it) moves it.
            let probe = if sep.vertical {
                (sep.x.saturating_sub(1), y)
            } else {
                (x, sep.y.saturating_sub(1))
            };
            if let Some((pane, _)) = rects
                .iter()
                .find(|(_, r)| ui::contains(*r, probe.0, probe.1))
            {
                let last = if sep.vertical { x } else { y };
                self.clients.get_mut(&id).unwrap().drag = Some(Drag::Border {
                    pane: *pane,
                    vertical: sep.vertical,
                    last,
                });
            }
            return;
        }

        let Some((pane_id, rect)) = hit else { return };
        let shift = ev.modifiers.contains(KeyModifiers::SHIFT);
        let wants_mouse = self
            .panes
            .get(&pane_id)
            .is_some_and(|p| p.emulator.mouse_mode() != rmx_core::emulator::MouseMode::None);

        if matches!(ev.kind, MouseEventKind::Down(_)) && pane_id != active_pane {
            if let Some(t) = self.workspace_mut(ws_id).map(Workspace::active_mut) {
                t.active_pane = pane_id;
            }
            self.exit_select(id);
            self.mark_workspace_dirty(ws_id);
        }

        // Programs that asked for the mouse get it (Shift overrides, as in
        // most terminals).
        if wants_mouse && !shift {
            self.forward_mouse(pane_id, &ev, rect);
            if matches!(ev.kind, MouseEventKind::Down(_)) {
                self.clients.get_mut(&id).unwrap().drag = Some(Drag::App(pane_id));
            }
            return;
        }

        match ev.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                let (row, col) = (y - rect.y, x - rect.x);
                if let Some(p) = self.panes.get(&pane_id) {
                    let mut sel = Select::at(pane_id, p.emulator.as_ref(), row, col);
                    sel.anchor = Some(sel.cursor);
                    sel.by_mouse = true;
                    let c = self.clients.get_mut(&id).unwrap();
                    c.engine.set_mode("select", true);
                    c.select = Some(sel);
                    c.drag = Some(Drag::Select);
                }
            }
            MouseEventKind::Down(MouseButton::Right) => {
                let has_selection = self
                    .clients
                    .get(&id)
                    .and_then(|c| c.select.as_ref())
                    .is_some_and(Select::has_selection);
                self.push_overlay(id, |_| {
                    Overlay::Context(ui::menu::ContextMenu::new(x, y, has_selection))
                });
            }
            MouseEventKind::Down(MouseButton::Middle) => self.dispatch(id, Action::Paste),
            MouseEventKind::ScrollUp | MouseEventKind::ScrollDown => {
                let up = matches!(ev.kind, MouseEventKind::ScrollUp);
                let alt = self
                    .panes
                    .get(&pane_id)
                    .is_some_and(|p| p.emulator.alternate_screen());
                if alt {
                    // Full-screen programs without mouse support (less, man)
                    // scroll with arrow keys, as terminals do.
                    let key = KeyEvent::from(if up { KeyCode::Up } else { KeyCode::Down });
                    if let Some(p) = self.panes.get(&pane_id)
                        && let Some(bytes) = encode_key(&key, p.emulator.application_cursor())
                    {
                        p.write(bytes.repeat(3));
                    }
                } else {
                    self.scroll(id, pane_id, if up { -3 } else { 3 });
                }
            }
            _ => {}
        }
    }

    fn drag_select(
        &mut self,
        id: ClientId,
        pane: PaneId,
        _area: Rect,
        rects: &[(PaneId, Rect)],
        x: u16,
        y: u16,
    ) {
        let Some(rect) = rects.iter().find(|(p, _)| *p == pane).map(|(_, r)| *r) else {
            return;
        };
        let Some(c) = self.clients.get_mut(&id) else {
            return;
        };
        let (Some(sel), Some(p)) = (c.select.as_mut(), self.panes.get_mut(&pane)) else {
            return;
        };
        let emu = p.emulator.as_mut();
        // Dragging past the top or bottom edge scrolls.
        if y < rect.y {
            sel.scroll(-1, emu);
        } else if y >= rect.bottom() {
            sel.scroll(1, emu);
        } else {
            let top = crate::select::view_top(emu);
            sel.cursor = (
                top + usize::from(y - rect.y),
                x.saturating_sub(rect.x).min(rect.width.saturating_sub(1)),
            );
        }
    }

    fn forward_mouse(&self, pane: PaneId, ev: &MouseEvent, rect: Rect) {
        let Some(p) = self.panes.get(&pane) else {
            return;
        };
        let col = ev
            .column
            .saturating_sub(rect.x)
            .min(rect.width.saturating_sub(1));
        let row = ev
            .row
            .saturating_sub(rect.y)
            .min(rect.height.saturating_sub(1));
        if let Some(bytes) = mouse::encode(
            ev,
            col,
            row,
            p.emulator.mouse_mode(),
            p.emulator.mouse_sgr(),
        ) {
            p.write(bytes);
        }
    }

    // ---- config ----------------------------------------------------------------------

    fn reload_config(&mut self, requested_by: Option<ClientId>) {
        match Config::load(&self.opts.config_path) {
            Ok((config, warnings)) => {
                self.config = config;
                for c in self.clients.values_mut() {
                    let direct = self.config.direct_enabled(c.enhanced_keys);
                    c.keymap = self.config.keymap(direct);
                    end_select(c, &mut self.panes);
                    c.engine = KeyEngine::new();
                    c.dirty = true;
                }
                let msg = match warnings.first() {
                    Some(w) => (
                        format!("Settings reloaded · {} warning(s): {w}", warnings.len()),
                        true,
                    ),
                    None => ("Settings reloaded".to_owned(), false),
                };
                let ids: Vec<ClientId> = self.clients.keys().copied().collect();
                for id in ids {
                    self.toast(id, msg.0.clone(), msg.1);
                }
                tracing::info!("config reloaded");
            }
            Err(err) => {
                // Keep running with the old config; show where the problem
                // is, with just the file name so the message fits.
                let full = self.opts.config_path.display().to_string();
                let name = self
                    .opts
                    .config_path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                let first = err
                    .to_string()
                    .lines()
                    .next()
                    .unwrap_or_default()
                    .replace(&full, &name);
                tracing::warn!("config reload failed: {err}");
                let ids: Vec<ClientId> = match requested_by {
                    Some(id) => vec![id],
                    None => self.clients.keys().copied().collect(),
                };
                for id in ids {
                    self.toast(id, format!("Settings not applied: {first}"), true);
                }
            }
        }
    }

    // ---- control API -----------------------------------------------------------------

    fn request(&mut self, id: ClientId, req: Request) {
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

    fn clients_by_ws(&self) -> HashMap<WorkspaceId, usize> {
        let mut m = HashMap::new();
        for c in self.clients.values() {
            *m.entry(c.workspace).or_insert(0) += 1;
        }
        m
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

    fn emit(&mut self, kind: &str, mut data: serde_json::Value) {
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

    // ---- persistence -----------------------------------------------------------------

    fn snapshot(&self) -> Snapshot {
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
    fn restore(&mut self) -> Result<(usize, usize)> {
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

    // ---- frames ----------------------------------------------------------------------

    fn on_tick(&mut self) {
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

/// Leaves select mode and returns the pane's view to the live screen.
fn end_select(c: &mut Client, panes: &mut HashMap<PaneId, Pane>) {
    if let Some(sel) = c.select.take()
        && let Some(p) = panes.get_mut(&sel.pane)
    {
        p.emulator.set_view_offset(0);
    }
    c.drag = None;
    c.engine.exit_mode();
}

// Takes the core's fields one by one (not `&self`) so callers can hold a
// `&mut` client at the same time; hence the long argument list.
#[allow(clippy::too_many_arguments)]
fn make_ctx<'a>(
    config: &'a Config,
    keymap: &'a Keymap,
    workspaces: &'a [Workspace],
    panes: &'a HashMap<PaneId, Pane>,
    attached: &'a HashMap<WorkspaceId, usize>,
    current: WorkspaceId,
    launch_cwd: Option<&'a std::path::Path>,
    opts: &'a ServerOptions,
) -> Ctx<'a> {
    Ctx {
        theme: &config.theme,
        keymap,
        workspaces,
        panes,
        attached,
        current,
        launch_cwd,
        config_path: &opts.config_path,
        commands: &config.commands,
    }
}

/// Smallest screen we lay out for. Smaller terminals show the top-left of
/// it; every layout and overlay can assume at least this much room.
const MIN_SIZE: (u16, u16) = (40, 10);

fn clamp_size(cols: u16, rows: u16) -> (u16, u16) {
    (cols.max(MIN_SIZE.0), rows.max(MIN_SIZE.1))
}

fn folder_name(dir: Option<&std::path::Path>) -> Option<String> {
    dir?.file_name().map(|n| n.to_string_lossy().into_owned())
}

fn pane_size_for(cols: u16, rows: u16) -> (u16, u16) {
    let a = compositor::pane_area(Rect::new(0, 0, cols, rows));
    (a.width.max(1), a.height.max(1))
}

fn side(d: Dir) -> Side {
    match d {
        Dir::Left => Side::Left,
        Dir::Right => Side::Right,
        Dir::Up => Side::Up,
        Dir::Down => Side::Down,
    }
}

/// `\n`, `\r`, `\t`, `\e` and `\\` escapes for `send-keys` style input.
fn unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('r') => out.push('\r'),
            Some('t') => out.push('\t'),
            Some('e') => out.push('\x1b'),
            Some(other) => out.push(other),
            None => out.push('\\'),
        }
    }
    out
}

/// The user's editor, or a sensible per-OS default.
fn editor_command(path: &std::path::Path) -> String {
    let editor = std::env::var("VISUAL")
        .or_else(|_| std::env::var("EDITOR"))
        .unwrap_or_else(|_| {
            if cfg!(windows) {
                "notepad".into()
            } else {
                "nano".into()
            }
        });
    format!("{editor} \"{}\"", path.display())
}

fn run_background(command: &str, cwd: Option<PathBuf>) -> Result<()> {
    let mut cmd = if cfg!(windows) {
        let mut c = std::process::Command::new("cmd.exe");
        c.args(["/C", command]);
        c
    } else {
        let mut c = std::process::Command::new("/bin/sh");
        c.args(["-c", command]);
        c
    };
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // The server has no console; without this a window would flash.
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    if let Some(dir) = cwd {
        cmd.current_dir(dir);
    }
    cmd.stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    cmd.spawn()?;
    Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unescapes_send_keys() {
        assert_eq!(unescape(r"ls\r\tx\\y\e"), "ls\r\tx\\y\x1b");
    }
}
