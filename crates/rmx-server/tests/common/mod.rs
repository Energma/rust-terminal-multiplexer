//! Shared harness for the real-PTY integration tests: an isolated server
//! config, a test client that rebuilds the screen from diffs, and a one-shot
//! control-API request helper.

// Each test binary uses a different subset of these helpers.
#![allow(dead_code)]

use std::time::{Duration, Instant};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use rmx_proto::{
    Attach, ClientEnd, ClientMsg, Hello, PROTOCOL_VERSION, Request, ServerEnd, ServerMsg, WireCell,
};
use rmx_server::ServerOptions;
use tokio::sync::mpsc;

pub const TIMEOUT: Duration = Duration::from_secs(20);

/// A plain shell with no user rc files, so prompts can't interfere.
pub fn test_shell() -> String {
    if cfg!(windows) {
        "cmd.exe".into()
    } else {
        "/bin/sh".into()
    }
}

/// Each test gets its own config (pinning the shell) so the developer's own
/// config can't affect results, and no session file or config watcher.
pub fn test_options(name: &str) -> ServerOptions {
    let dir = std::env::temp_dir().join(format!("rmx-test-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let config = dir.join("config.toml");
    std::fs::write(&config, format!("[general]\nshell = {:?}\n", test_shell())).unwrap();
    ServerOptions {
        session_file: None,
        watch_config: false,
        ..ServerOptions::new("test", config)
    }
}

/// Drives the server like the real client: rebuilds the screen from diffs.
pub struct TestClient {
    pub conn: ClientEnd,
    screen: Vec<Vec<String>>,
    cols: u16,
    rows: u16,
}

impl TestClient {
    pub async fn attach(
        incoming: &mpsc::Sender<ServerEnd>,
        cols: u16,
        rows: u16,
        attach: Attach,
    ) -> Self {
        let (conn, server_end) = rmx_proto::in_memory();
        incoming.send(server_end).await.unwrap();
        let hello = Hello {
            version: PROTOCOL_VERSION,
            cols,
            rows,
            enhanced_keys: false,
            attach,
            open_manager: false,
            cwd: None,
        };
        conn.tx.send(ClientMsg::Hello(hello)).await.unwrap();
        Self {
            conn,
            screen: vec![vec![" ".into(); cols.into()]; rows.into()],
            cols,
            rows,
        }
    }

    pub fn text(&self) -> String {
        self.screen
            .iter()
            .map(|r| r.concat())
            .collect::<Vec<_>>()
            .join("\n")
    }

    pub async fn key(&self, code: KeyCode, mods: KeyModifiers) {
        self.conn
            .tx
            .send(ClientMsg::Key(KeyEvent::new(code, mods)))
            .await
            .unwrap();
    }

    pub async fn leader(&self) {
        self.key(KeyCode::Char(' '), KeyModifiers::CONTROL).await;
    }

    pub async fn type_line(&self, s: &str) {
        for c in s.chars() {
            self.key(KeyCode::Char(c), KeyModifiers::NONE).await;
        }
        self.key(KeyCode::Enter, KeyModifiers::NONE).await;
    }

    pub async fn wait_for(&mut self, what: &str, done: impl Fn(&str) -> bool) {
        let deadline = Instant::now() + TIMEOUT;
        while !done(&self.text()) {
            let msg = tokio::time::timeout_at(deadline.into(), self.conn.rx.recv())
                .await
                .unwrap_or_else(|_| {
                    panic!("timed out waiting for {what}; screen:\n{}", self.text())
                })
                .expect("server hung up");
            match msg {
                ServerMsg::Frame(frame) => {
                    assert_eq!((frame.cols, frame.rows), (self.cols, self.rows));
                    if frame.full {
                        self.screen = vec![vec![" ".into(); self.cols.into()]; self.rows.into()];
                    }
                    for (x, y, WireCell { symbol, .. }) in frame.cells {
                        self.screen[usize::from(y)][usize::from(x)] = symbol;
                    }
                }
                ServerMsg::Exit { reason } => panic!("exited ({reason}) waiting for {what}"),
                _ => {}
            }
        }
    }

    pub async fn wait_exit(&mut self) -> String {
        let deadline = Instant::now() + TIMEOUT;
        loop {
            match tokio::time::timeout_at(deadline.into(), self.conn.rx.recv())
                .await
                .expect("no Exit")
            {
                Some(ServerMsg::Exit { reason }) => return reason,
                Some(_) => {}
                None => return "disconnected".into(),
            }
        }
    }
}

pub async fn control(incoming: &mpsc::Sender<ServerEnd>, req: Request) -> Result<String, String> {
    let (mut conn, server_end) = rmx_proto::in_memory();
    incoming.send(server_end).await.unwrap();
    conn.tx.send(ClientMsg::request(req)).await.unwrap();
    loop {
        match tokio::time::timeout(TIMEOUT, conn.rx.recv()).await.unwrap() {
            Some(ServerMsg::Response { ok: true, body }) => return Ok(body),
            Some(ServerMsg::Response { ok: false, body }) => return Err(body),
            Some(_) => {}
            None => panic!("closed"),
        }
    }
}

pub fn new_ws(name: &str) -> Request {
    Request::NewWorkspace {
        name: Some(name.into()),
        cwd: None,
        command: None,
    }
}
