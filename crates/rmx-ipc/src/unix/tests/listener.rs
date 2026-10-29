use std::fs;
use std::os::unix::fs::{MetadataExt, PermissionsExt};

use rmx_proto::{ClientEnd, ClientMsg, ServerMsg};

use super::{TempDir, pair};
use crate::unix::{Listener, connect, is_alive};

#[tokio::test]
async fn round_trips_messages_both_ways() {
    let dir = TempDir::new();
    let (_l, mut server, client) = pair(&dir.ep()).await;
    let ClientEnd { tx, mut rx } = client;
    for _ in 0..3 {
        tx.send(ClientMsg::Detach).await.unwrap();
    }
    tx.send(ClientMsg::Paste("hi".into())).await.unwrap();
    for _ in 0..3 {
        assert_eq!(server.rx.recv().await, Some(ClientMsg::Detach));
    }
    assert_eq!(server.rx.recv().await, Some(ClientMsg::Paste("hi".into())));
    for _ in 0..20 {
        server.tx.send(ServerMsg::Bell).await.unwrap();
        assert_eq!(rx.recv().await, Some(ServerMsg::Bell));
    }
}

#[tokio::test]
async fn carries_large_messages() {
    let dir = TempDir::new();
    let (_l, mut server, client) = pair(&dir.ep()).await;
    let big = "x".repeat(1024 * 1024 + 17);
    client.tx.send(ClientMsg::Paste(big.clone())).await.unwrap();
    assert_eq!(server.rx.recv().await, Some(ClientMsg::Paste(big)));
}

#[tokio::test]
async fn closing_client_ends_server_rx() {
    let dir = TempDir::new();
    let (_l, mut server, client) = pair(&dir.ep()).await;
    drop(client);
    assert_eq!(server.rx.recv().await, None);
}

#[tokio::test]
async fn dropping_server_tx_ends_client_rx_but_not_its_tx() {
    let dir = TempDir::new();
    let (_l, mut server, mut client) = pair(&dir.ep()).await;
    server.tx.send(ServerMsg::Bell).await.unwrap();
    drop(server.tx);
    assert_eq!(client.rx.recv().await, Some(ServerMsg::Bell));
    assert_eq!(client.rx.recv().await, None);
    // Half-close: the other direction keeps working.
    client.tx.send(ClientMsg::Detach).await.unwrap();
    assert_eq!(server.rx.recv().await, Some(ClientMsg::Detach));
}

#[tokio::test]
async fn bind_refuses_an_unsafe_directory() {
    let dir = TempDir::new();
    fs::set_permissions(&dir.0, fs::Permissions::from_mode(0o755)).unwrap();
    assert!(Listener::bind(&dir.ep()).await.is_err());
    assert!(connect(&dir.ep()).await.is_err());
}

#[tokio::test]
async fn socket_is_private() {
    let dir = TempDir::new();
    let ep = dir.ep();
    let _l = Listener::bind(&ep).await.unwrap();
    let mode = fs::metadata(ep.path()).unwrap().mode() & 0o777;
    assert_eq!(mode, 0o600);
}

#[tokio::test]
async fn replaces_a_stale_socket() {
    let dir = TempDir::new();
    let ep = dir.ep();
    // std's listener doesn't unlink on drop: exactly what a crash leaves.
    drop(std::os::unix::net::UnixListener::bind(ep.path()).unwrap());
    assert!(ep.path().exists());
    assert!(!is_alive(&ep).await);
    let (_l, mut server, client) = pair(&ep).await;
    client.tx.send(ClientMsg::Detach).await.unwrap();
    assert_eq!(server.rx.recv().await, Some(ClientMsg::Detach));
}

#[tokio::test]
async fn refuses_to_remove_a_non_socket() {
    let dir = TempDir::new();
    let ep = dir.ep();
    fs::write(ep.path(), b"precious").unwrap();
    assert!(Listener::bind(&ep).await.is_err());
    assert_eq!(fs::read(ep.path()).unwrap(), b"precious");
}

#[tokio::test]
async fn second_bind_on_live_endpoint_errors() {
    let dir = TempDir::new();
    let ep = dir.ep();
    let _l = Listener::bind(&ep).await.unwrap();
    let err = Listener::bind(&ep).await.err().unwrap().to_string();
    assert!(err.contains("already running"), "{err}");
    // And the failed attempt must not have removed the live socket.
    assert!(is_alive(&ep).await);
}

#[tokio::test]
async fn is_alive_tracks_the_listener() {
    let dir = TempDir::new();
    let ep = dir.ep();
    assert!(!is_alive(&ep).await);
    let l = Listener::bind(&ep).await.unwrap();
    assert!(is_alive(&ep).await);
    drop(l);
    assert!(!ep.path().exists());
    assert!(!is_alive(&ep).await);
    // Lock released with the listener: a new server can start.
    let _l = Listener::bind(&ep).await.unwrap();
}

#[tokio::test]
async fn connect_or_spawn_uses_a_running_server() {
    let dir = TempDir::new();
    let ep = dir.ep();
    let mut l = Listener::bind(&ep).await.unwrap();
    let (client, server) = tokio::join!(crate::connect_or_spawn(&ep, &["__never"]), l.accept());
    let (client, spawned) = client.unwrap();
    assert!(spawned.is_none());
    client.tx.send(ClientMsg::Detach).await.unwrap();
    assert_eq!(server.unwrap().rx.recv().await, Some(ClientMsg::Detach));
}
