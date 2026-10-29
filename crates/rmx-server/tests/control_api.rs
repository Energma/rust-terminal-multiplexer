//! The control API (what `rmx` subcommands use) against a real server:
//! workspace lifecycle, send-keys and capture, and attaching by name.

mod common;

use std::time::{Duration, Instant};

use common::{TIMEOUT, TestClient, control, new_ws, test_options};
use rmx_proto::{Attach, Request};
use rmx_server::run;
use tokio::sync::mpsc;

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
