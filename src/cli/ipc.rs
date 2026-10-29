//! Talking to the background server: one-shot requests and the event stream.

use anyhow::{Context, Result, bail};
use rmx_proto::{ClientMsg, Request, ServerMsg};

use super::server::server_args;

/// Sends one request to a server, starting one if needed.
pub(super) async fn request(
    server: &str,
    config: &std::path::Path,
    req: Request,
) -> Result<String> {
    let ep = rmx_ipc::endpoint(server)?;
    let args = server_args(server, config)?;
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let (conn, _) = rmx_ipc::connect_or_spawn(&ep, &args).await?;
    roundtrip(conn, req).await
}

/// Sends one request to a running server; errors helpfully if there is none.
pub(super) async fn request_existing(server: &str, req: Request) -> Result<String> {
    let ep = rmx_ipc::endpoint(server)?;
    if !rmx_ipc::is_alive(&ep).await {
        bail!("no rmx server is running (start one with `rmx`)");
    }
    roundtrip(rmx_ipc::connect(&ep).await?, req).await
}

async fn roundtrip(mut conn: rmx_proto::ClientEnd, req: Request) -> Result<String> {
    conn.tx.send(ClientMsg::request(req)).await?;
    loop {
        match conn.rx.recv().await {
            Some(ServerMsg::Response { ok: true, body }) => return Ok(body),
            Some(ServerMsg::Response { ok: false, body }) => bail!("{body}"),
            Some(ServerMsg::Exit { reason }) => bail!("{reason}"),
            Some(_) => continue,
            None => bail!(
                "the server closed the connection; it may be shutting down, or be a different \
                 rmx version (run `rmx stop`, then retry)"
            ),
        }
    }
}

pub(super) fn simple(runtime: &tokio::runtime::Runtime, server: &str, req: Request) -> Result<i32> {
    runtime.block_on(request_existing(server, req))?;
    Ok(0)
}

pub(super) async fn events(server: &str) -> Result<()> {
    let ep = rmx_ipc::endpoint(server)?;
    let mut conn = rmx_ipc::connect(&ep)
        .await
        .context("no rmx server is running")?;
    conn.tx.send(ClientMsg::request(Request::Subscribe)).await?;
    while let Some(msg) = conn.rx.recv().await {
        if let ServerMsg::Event(line) = msg {
            println!("{line}");
        }
    }
    Ok(())
}
