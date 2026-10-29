//! The two halves of a connection as typed channels, and the in-process pair.

use tokio::sync::mpsc;

use crate::{ClientMsg, ServerMsg};

/// The client's half of a connection.
pub struct ClientEnd {
    pub tx: mpsc::Sender<ClientMsg>,
    pub rx: mpsc::Receiver<ServerMsg>,
}

/// The server's half of a connection.
pub struct ServerEnd {
    pub tx: mpsc::Sender<ServerMsg>,
    pub rx: mpsc::Receiver<ClientMsg>,
}

/// An in-process connection. Phase 1 runs the server on a task inside the
/// client process; phase 5 bridges these same channels over a local socket.
pub fn in_memory() -> (ClientEnd, ServerEnd) {
    // Small buffers: a slow client should apply back-pressure, not queue
    // frames without bound.
    let (client_tx, server_rx) = mpsc::channel(256);
    let (server_tx, client_rx) = mpsc::channel(8);
    (
        ClientEnd {
            tx: client_tx,
            rx: client_rx,
        },
        ServerEnd {
            tx: server_tx,
            rx: server_rx,
        },
    )
}
