//! Connections: accepting them, routing their messages, and detaching.

use std::collections::HashMap;

use rmx_proto::{ClientMsg, PROTOCOL_VERSION, ServerEnd, ServerMsg};
use tokio::sync::mpsc;

use super::event_loop::Event;
use super::handshake::clamp_size;
use super::{ClientId, Core};

impl Core {
    /// Registers a connection; its messages arrive later as [`Event::Message`].
    pub(super) fn accept(&mut self, mut conn: ServerEnd) {
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

    pub(super) fn pending_conns(&mut self) -> &mut HashMap<ClientId, mpsc::Sender<ServerMsg>> {
        &mut self.pending
    }

    pub(super) fn on_message(&mut self, id: ClientId, msg: ClientMsg) {
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

    pub(super) fn detach(&mut self, id: ClientId) {
        if let Some(c) = self.clients.remove(&id) {
            let _ = c.tx.try_send(ServerMsg::Exit {
                reason: "detached".into(),
            });
            self.emit("client-detached", serde_json::json!({ "client": id }));
            self.recompute_sizes();
        }
    }
}
