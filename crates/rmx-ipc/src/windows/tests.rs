use std::path::Path;

use windows_sys::Win32::System::Threading::CREATE_BREAKAWAY_FROM_JOB;

use super::spawn::{creation_flags, detached_command};
use super::{Listener, connect, endpoint_at, is_alive};
use crate::Endpoint;
use rmx_proto::{ClientMsg, ServerMsg};
use std::sync::atomic::{AtomicU32, Ordering};

fn unique_ep() -> Endpoint {
    static N: AtomicU32 = AtomicU32::new(0);
    endpoint_at(format!(
        r"\\.\pipe\rmx-ipc-test-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::Relaxed)
    ))
}

#[test]
fn endpoint_contains_sid_and_name() {
    let ep = crate::endpoint("work").unwrap();
    assert!(
        ep.display().starts_with(r"\\.\pipe\rmx-S-1-"),
        "{}",
        ep.display()
    );
    assert!(ep.display().ends_with("-work"));
}

#[test]
fn breakaway_flag_is_optional() {
    assert_ne!(creation_flags(true) & CREATE_BREAKAWAY_FROM_JOB, 0);
    assert_eq!(creation_flags(false) & CREATE_BREAKAWAY_FROM_JOB, 0);
    let cmd = detached_command(Path::new(r"C:\rmx.exe"), &["__server"], true);
    assert_eq!(cmd.get_args().collect::<Vec<_>>(), ["__server"]);
}

#[tokio::test]
async fn round_trips_messages_both_ways() {
    let ep = unique_ep();
    let mut l = Listener::bind(&ep).await.unwrap();
    let (client, server) = tokio::join!(connect(&ep), l.accept());
    let (mut client, mut server) = (client.unwrap(), server.unwrap());
    client.tx.send(ClientMsg::Detach).await.unwrap();
    client
        .tx
        .send(ClientMsg::Paste("x".repeat(1 << 20)))
        .await
        .unwrap();
    assert_eq!(server.rx.recv().await, Some(ClientMsg::Detach));
    assert_eq!(
        server.rx.recv().await,
        Some(ClientMsg::Paste("x".repeat(1 << 20)))
    );
    server.tx.send(ServerMsg::Bell).await.unwrap();
    assert_eq!(client.rx.recv().await, Some(ServerMsg::Bell));
    drop(server.tx);
    assert_eq!(client.rx.recv().await, None);
    drop(client);
    assert_eq!(server.rx.recv().await, None);
}

#[tokio::test]
async fn second_bind_errors_and_is_alive_tracks_listener() {
    let ep = unique_ep();
    assert!(!is_alive(&ep).await);
    let l = Listener::bind(&ep).await.unwrap();
    assert!(is_alive(&ep).await);
    assert!(Listener::bind(&ep).await.is_err());
    // The probes above connected and left; accept must skip them.
    let mut l = l;
    let (client, server) = tokio::join!(connect(&ep), l.accept());
    client.unwrap().tx.send(ClientMsg::Detach).await.unwrap();
    assert_eq!(server.unwrap().rx.recv().await, Some(ClientMsg::Detach));
    drop(l);
    assert!(!is_alive(&ep).await);
}
