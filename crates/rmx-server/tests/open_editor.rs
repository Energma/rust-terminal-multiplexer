//! Leader `e` against a real server: the editor starts at the project root,
//! whether it runs on its own (like VS Code) or falls back to a terminal one.
//!
//! In its own test binary because it sets `VISUAL`, which is process-wide.
//! Unix only: the fake editors below are `sh` syntax.
#![cfg(unix)]

mod common;

use std::io::Write;
use std::path::Path;
use std::time::{Duration, Instant};

use common::{TIMEOUT, TestClient, control, test_options};
use crossterm::event::{KeyCode, KeyModifiers};
use rmx_proto::{Attach, Request};
use rmx_server::run;
use tokio::sync::mpsc;

/// Presses Leader `e` in a pane inside `repo/src` with `editor` as the
/// `[editor]` section, and waits for the editor to touch `opened`.
async fn leader_e_runs_at_root(name: &str, editor: &str) {
    let base = std::env::temp_dir().join(format!("rmx-{name}-{}", std::process::id()));
    let repo = base.join("repo");
    let nested = repo.join("src");
    std::fs::create_dir_all(repo.join(".git")).unwrap();
    std::fs::create_dir_all(&nested).unwrap();

    let opts = test_options(name);
    let mut config = std::fs::OpenOptions::new()
        .append(true)
        .open(&opts.config_path)
        .unwrap();
    writeln!(config, "[editor]\n{editor}").unwrap();

    let (incoming, rx) = mpsc::channel(8);
    let server = tokio::spawn(run(rx, opts));
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

    wait_for(&repo.join("opened"), name).await;
    assert!(!nested.join("opened").exists());

    control(&incoming, Request::Stop).await.unwrap();
    server.await.unwrap().unwrap();
    std::fs::remove_dir_all(&base).unwrap();
}

async fn wait_for(file: &Path, case: &str) {
    let deadline = Instant::now() + TIMEOUT;
    while !file.exists() {
        assert!(
            Instant::now() < deadline,
            "{case}: the editor never ran at the project root"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn leader_e_opens_the_editor_at_the_project_root() {
    // A GUI editor such as the default VS Code: launched on its own.
    leader_e_runs_at_root(
        "editor-window",
        "command = \"touch opened; :\"\nterminal = false",
    )
    .await;
    // A terminal editor in a new tab.
    leader_e_runs_at_root("editor-tab", "command = \"touch opened; :\"").await;

    // SAFETY: the servers above have stopped and this binary has no other
    // test, so nothing else can be reading the environment.
    unsafe { std::env::set_var("VISUAL", "touch opened; :") };
    // Not installed: falls back to $VISUAL instead of doing nothing.
    leader_e_runs_at_root("editor-missing", "command = \"rmx-no-such-editor\"").await;
}
