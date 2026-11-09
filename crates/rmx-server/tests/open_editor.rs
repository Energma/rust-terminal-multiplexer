//! Leader `e` against a real server: the editor starts at the project root.
//!
//! In its own test binary because it sets `VISUAL`, which is process-wide.
//! Unix only: the fake editor below is `sh` syntax.
#![cfg(unix)]

mod common;

use std::time::{Duration, Instant};

use common::{TIMEOUT, TestClient, control, test_options};
use crossterm::event::{KeyCode, KeyModifiers};
use rmx_proto::{Attach, Request};
use rmx_server::run;
use tokio::sync::mpsc;

#[tokio::test(flavor = "multi_thread")]
async fn leader_e_opens_the_editor_at_the_project_root() {
    let base = std::env::temp_dir().join(format!("rmx-open-editor-{}", std::process::id()));
    let repo = base.join("repo");
    let nested = repo.join("src");
    std::fs::create_dir_all(repo.join(".git")).unwrap();
    std::fs::create_dir_all(&nested).unwrap();
    // SAFETY: this binary's only test, and nothing else is running yet that
    // could touch the environment: the runtime's workers are idle.
    unsafe { std::env::set_var("VISUAL", "touch opened; :") };

    let (incoming, rx) = mpsc::channel(8);
    let server = tokio::spawn(run(rx, test_options("open-editor")));
    control(
        &incoming,
        Request::NewWorkspace {
            name: Some("work".into()),
            cwd: Some(nested.to_string_lossy().into_owned()),
            command: None,
        },
    )
    .await
    .unwrap();
    let c = TestClient::attach(&incoming, 80, 24, Attach::Named("work".into())).await;
    c.leader().await;
    c.key(KeyCode::Char('e'), KeyModifiers::NONE).await;

    let deadline = Instant::now() + TIMEOUT;
    while !repo.join("opened").exists() {
        assert!(
            Instant::now() < deadline,
            "the editor never ran at the project root"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(!nested.join("opened").exists());

    control(&incoming, Request::Stop).await.unwrap();
    server.await.unwrap().unwrap();
    std::fs::remove_dir_all(&base).unwrap();
}
