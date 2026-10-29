//! Real-PTY tests through the whole server core. These run on every CI OS;
//! on Windows they also prove ConPTY's startup cursor query gets answered
//! (otherwise the shell hangs and these time out).

mod common;

use std::time::Instant;

use common::{TIMEOUT, TestClient, control, test_options, test_shell};
use crossterm::event::{KeyCode, KeyModifiers};
use rmx_proto::{Attach, Request};
use rmx_server::pane::{Pane, PaneEvent, SpawnSpec};
use rmx_server::run;
use tokio::sync::mpsc;

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
