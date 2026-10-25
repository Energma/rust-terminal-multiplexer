//! Real-PTY tests through the whole server core. These run on every CI OS;
//! on Windows they also prove ConPTY's startup cursor query gets answered
//! (otherwise the shell hangs and these time out).

use std::time::{Duration, Instant};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use rmx_proto::{
    Attach, ClientEnd, ClientMsg, Hello, PROTOCOL_VERSION, Request, ServerEnd, ServerMsg, WireCell,
};
use rmx_server::pane::{Pane, PaneEvent, SpawnSpec};
use rmx_server::{ServerOptions, run};
use tokio::sync::mpsc;

const TIMEOUT: Duration = Duration::from_secs(20);

/// A plain shell with no user rc files, so prompts can't interfere.
fn test_shell() -> String {
    if cfg!(windows) {
        "cmd.exe".into()
    } else {
        "/bin/sh".into()
    }
}

/// Each test gets its own config (pinning the shell) so the developer's own
/// config can't affect results, and no session file or config watcher.
fn test_options(name: &str) -> ServerOptions {
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

#[tokio::test(flavor = "multi_thread")]
async fn pane_runs_a_command_and_reports_close() {
    let (tx, mut rx) = mpsc::channel(256);
    let spec = SpawnSpec {
        shell: Some(test_shell()),
        ..Default::default()
    };
    let mut pane = Pane::spawn(0, &spec, 24, 80, 100, tx).unwrap();
    pane.write(b"echo rmx-ok\r".to_vec());

    let deadline = Instant::now() + TIMEOUT;
    // The typed command line echoes "rmx-ok" once; the output is the second.
    while pane.emulator.contents().matches("rmx-ok").count() < 2 {
        let event = tokio::time::timeout_at(deadline.into(), rx.recv())
            .await
            .unwrap_or_else(|_| panic!("timed out; screen:\n{}", pane.emulator.contents()));
        match event.unwrap() {
            PaneEvent::Output(_, bytes) => pane.process_output(&bytes),
            PaneEvent::Closed(_) => panic!("shell exited early"),
        }
    }

    pane.write(b"exit\r".to_vec());
    loop {
        let event = tokio::time::timeout_at(deadline.into(), rx.recv())
            .await
            .expect("pane did not close after exit");
        match event.unwrap() {
            PaneEvent::Output(_, bytes) => pane.process_output(&bytes),
            PaneEvent::Closed(_) => break,
        }
    }
}

/// Drives the server like the real client: rebuilds the screen from diffs.
struct TestClient {
    conn: ClientEnd,
    screen: Vec<Vec<String>>,
    cols: u16,
    rows: u16,
}

impl TestClient {
    async fn attach(
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
        };
        conn.tx.send(ClientMsg::Hello(hello)).await.unwrap();
        Self {
            conn,
            screen: vec![vec![" ".into(); cols.into()]; rows.into()],
            cols,
            rows,
        }
    }

    fn text(&self) -> String {
        self.screen
            .iter()
            .map(|r| r.concat())
            .collect::<Vec<_>>()
            .join("\n")
    }

    async fn key(&self, code: KeyCode, mods: KeyModifiers) {
        self.conn
            .tx
            .send(ClientMsg::Key(KeyEvent::new(code, mods)))
            .await
            .unwrap();
    }

    async fn leader(&self) {
        self.key(KeyCode::Char(' '), KeyModifiers::CONTROL).await;
    }

    async fn type_line(&self, s: &str) {
        for c in s.chars() {
            self.key(KeyCode::Char(c), KeyModifiers::NONE).await;
        }
        self.key(KeyCode::Enter, KeyModifiers::NONE).await;
    }

    async fn wait_for(&mut self, what: &str, done: impl Fn(&str) -> bool) {
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

    async fn wait_exit(&mut self) -> String {
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

async fn control(incoming: &mpsc::Sender<ServerEnd>, req: Request) -> Result<String, String> {
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

fn new_ws(name: &str) -> Request {
    Request::NewWorkspace {
        name: Some(name.into()),
        cwd: None,
        command: None,
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn typing_reaches_the_shell_and_shows_chrome() {
    let (incoming, rx) = mpsc::channel(8);
    let server = tokio::spawn(run(rx, test_options("chrome")));
    let mut c = TestClient::attach(&incoming, 100, 30, Attach::Last).await;
    c.type_line("echo rmx-ok").await;
    c.wait_for("echo output", |s| s.matches("rmx-ok").count() >= 2)
        .await;
    let text = c.text();
    assert!(
        text.lines().next().unwrap().contains("File"),
        "menu bar on top:\n{text}"
    );
    assert!(
        text.lines().last().unwrap().contains("F10 menu"),
        "status bar hints:\n{text}"
    );

    c.type_line("exit").await;
    assert_eq!(c.wait_exit().await, "workspace closed");
    server.await.unwrap().unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn leader_split_shows_two_panes_and_exit_returns_to_one() {
    let (incoming, rx) = mpsc::channel(8);
    let server = tokio::spawn(run(rx, test_options("split")));
    let mut c = TestClient::attach(&incoming, 100, 30, Attach::Last).await;
    c.type_line("echo first-pane").await;
    c.wait_for("first pane", |s| s.matches("first-pane").count() >= 2)
        .await;

    c.leader().await;
    c.key(KeyCode::Char('|'), KeyModifiers::SHIFT).await;
    let border = |s: &str| s.lines().nth(5).is_some_and(|l| l.contains('│'));
    c.wait_for("split border", border).await;

    c.type_line("echo second-pane").await;
    c.wait_for("second pane output", |s| {
        s.matches("second-pane").count() >= 2
    })
    .await;
    let v: serde_json::Value =
        serde_json::from_str(&control(&incoming, Request::List).await.unwrap()).unwrap();
    assert_eq!(
        v["workspaces"][0]["tabs"][0]["panes"]
            .as_array()
            .unwrap()
            .len(),
        2
    );

    c.type_line("exit").await;
    c.wait_for("border gone", |s| !border(s)).await;

    control(&incoming, Request::Stop).await.unwrap();
    server.await.unwrap().unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn control_api_manages_workspaces_and_sends_keys() {
    let (incoming, rx) = mpsc::channel(8);
    let server = tokio::spawn(run(rx, test_options("api")));
    assert!(
        control(&incoming, new_ws("work"))
            .await
            .unwrap()
            .contains("\"work\"")
    );
    assert!(
        control(&incoming, new_ws("work"))
            .await
            .unwrap()
            .contains("work-2"),
        "names are made unique"
    );

    control(
        &incoming,
        Request::SendKeys {
            target: Some("work".into()),
            text: "echo via-api\\r".into(),
        },
    )
    .await
    .unwrap();
    let deadline = Instant::now() + TIMEOUT;
    loop {
        let cap = control(
            &incoming,
            Request::Capture {
                target: Some("work:1.1".into()),
            },
        )
        .await
        .unwrap();
        if cap.matches("via-api").count() >= 2 {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "send-keys never showed up: {cap}"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    control(
        &incoming,
        Request::RenameWorkspace {
            from: "work-2".into(),
            to: "other".into(),
        },
    )
    .await
    .unwrap();
    assert!(
        control(
            &incoming,
            Request::RenameWorkspace {
                from: "other".into(),
                to: "work".into()
            }
        )
        .await
        .is_err()
    );
    control(
        &incoming,
        Request::CloseWorkspace {
            name: "other".into(),
        },
    )
    .await
    .unwrap();
    let err = control(
        &incoming,
        Request::Capture {
            target: Some("nope".into()),
        },
    )
    .await
    .unwrap_err();
    assert!(err.contains("no workspace"), "{err}");
    let list = control(&incoming, Request::List).await.unwrap();
    assert!(list.contains("\"work\"") && !list.contains("\"other\""));

    let mut a = TestClient::attach(&incoming, 80, 24, Attach::Named("work".into())).await;
    a.wait_for("workspace name in menu bar", |s| {
        s.lines().next().is_some_and(|l| l.contains("work"))
    })
    .await;

    control(&incoming, Request::Stop).await.unwrap();
    server.await.unwrap().unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn session_manager_navigates_and_opens_workspaces() {
    let (incoming, rx) = mpsc::channel(8);
    let server = tokio::spawn(run(rx, test_options("manager")));
    control(&incoming, new_ws("alpha")).await.unwrap();
    control(&incoming, new_ws("beta")).await.unwrap();
    let mut c = TestClient::attach(&incoming, 120, 32, Attach::Named("alpha".into())).await;
    c.leader().await;
    c.key(KeyCode::Char('s'), KeyModifiers::NONE).await;
    c.wait_for("session manager", |s| {
        s.contains("Session Manager") && s.contains("alpha") && s.contains("beta")
    })
    .await;
    // The selection starts on alpha's tab; down once reaches "beta".
    c.key(KeyCode::Down, KeyModifiers::NONE).await;
    c.key(KeyCode::Enter, KeyModifiers::NONE).await;
    c.wait_for("switched to beta", |s| {
        !s.contains("Session Manager") && s.lines().next().is_some_and(|l| l.contains("beta"))
    })
    .await;

    control(&incoming, Request::Stop).await.unwrap();
    server.await.unwrap().unwrap();
}

/// Regression: a 1-row client, tiny resizes and overlays on tiny screens
/// used to panic the core task and take down every session.
#[tokio::test(flavor = "multi_thread")]
async fn tiny_clients_and_overlays_do_not_crash_the_server() {
    let (incoming, rx) = mpsc::channel(8);
    let server = tokio::spawn(run(rx, test_options("tiny")));
    let c = TestClient::attach(&incoming, 80, 1, Attach::Last).await;
    for (cols, rows) in [(0, 0), (1, 1), (5, 2), (20, 3), (40, 4)] {
        c.conn
            .tx
            .send(ClientMsg::Resize { cols, rows })
            .await
            .unwrap();
        // Split down twice, open a prompt, the palette and the manager.
        c.leader().await;
        c.key(KeyCode::Char('-'), KeyModifiers::NONE).await;
        c.leader().await;
        c.key(KeyCode::Char('-'), KeyModifiers::NONE).await;
        c.leader().await;
        c.key(KeyCode::Char(','), KeyModifiers::NONE).await;
        c.key(KeyCode::Esc, KeyModifiers::NONE).await;
        c.leader().await;
        c.key(KeyCode::Char('p'), KeyModifiers::NONE).await;
        c.key(KeyCode::Esc, KeyModifiers::NONE).await;
        c.leader().await;
        c.key(KeyCode::Char('s'), KeyModifiers::NONE).await;
        c.key(KeyCode::Char('x'), KeyModifiers::NONE).await;
        tokio::time::sleep(Duration::from_millis(100)).await;
        c.key(KeyCode::Esc, KeyModifiers::NONE).await;
        c.key(KeyCode::Esc, KeyModifiers::NONE).await;
        // Select mode with a search badge, then out.
        c.leader().await;
        c.key(KeyCode::Char('['), KeyModifiers::NONE).await;
        c.key(KeyCode::Up, KeyModifiers::NONE).await;
        c.key(KeyCode::Esc, KeyModifiers::NONE).await;
    }
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(!server.is_finished(), "server core died");
    let list = control(&incoming, Request::List).await.unwrap();
    assert!(list.contains("\"main\""), "{list}");
    control(&incoming, Request::Stop).await.unwrap();
    server.await.unwrap().unwrap();
    drop(c);
}
