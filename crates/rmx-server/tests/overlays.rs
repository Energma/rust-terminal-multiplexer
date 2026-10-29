//! Overlays driven through a real client connection: the session manager,
//! and every overlay on screens too small to fit it.

mod common;

use std::time::Duration;

use common::{TestClient, control, new_ws, test_options};
use crossterm::event::{KeyCode, KeyModifiers};
use rmx_proto::{Attach, ClientMsg, Request};
use rmx_server::run;
use tokio::sync::mpsc;

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
