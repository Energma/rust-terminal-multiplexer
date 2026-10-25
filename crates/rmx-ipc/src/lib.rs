//! The real transport between `rmx` clients and the detached server
//! (docs/PLAN.md §4.1, §5.9): it bridges a local stream onto the same
//! [`ClientEnd`]/[`ServerEnd`] channels as [`rmx_proto::in_memory`], so the
//! server core and client never know which transport they run on.
//!
//! # Backend choice
//!
//! We use tokio's own `UnixListener`/`UnixStream` and
//! `tokio::net::windows::named_pipe` rather than `interprocess`. interprocess
//! 2.4 does expose security descriptors, `FILE_FLAG_FIRST_PIPE_INSTANCE` and
//! `PIPE_REJECT_REMOTE_CLIENTS`, but every check §5.9 needs — peer uid,
//! socket mode, stale-socket handling, the server's owner SID — has to reach
//! the raw OS object anyway, and tokio gives us that directly (`peer_cred`,
//! raw pipe handles, `create_with_security_attributes_raw`) with one fewer
//! layer whose defaults we'd have to audit.

use std::time::Duration;

use anyhow::Context;
use rmx_proto::codec::{Decoder, encode};
use rmx_proto::{ClientEnd, ServerEnd};
use serde::Serialize;
use serde::de::DeserializeOwned;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::sync::{mpsc, oneshot};
use tokio::time::Instant;

#[cfg(unix)]
mod unix;
#[cfg(unix)]
use unix as sys;
#[cfg(windows)]
mod windows;
#[cfg(windows)]
use windows as sys;

pub use sys::Listener;
#[cfg(unix)]
#[doc(hidden)]
pub use unix::endpoint_at;
#[cfg(windows)]
#[doc(hidden)]
pub use windows::endpoint_at;

/// Same capacities as `rmx_proto::in_memory`: input may queue up, but frames
/// to a slow client must apply back-pressure rather than pile up.
const CLIENT_TO_SERVER_CAP: usize = 256;
const SERVER_TO_CLIENT_CAP: usize = 8;
const READ_BUF: usize = 64 * 1024;
/// How long `connect_or_spawn` waits for a freshly spawned server.
const SPAWN_WAIT: Duration = Duration::from_secs(3);

/// Where a server listens: a socket path on Unix, a pipe name on Windows.
#[derive(Debug, Clone)]
pub struct Endpoint {
    #[cfg(unix)]
    path: std::path::PathBuf,
    #[cfg(windows)]
    pipe: String,
}

impl Endpoint {
    pub fn display(&self) -> String {
        #[cfg(unix)]
        return self.path.display().to_string();
        #[cfg(windows)]
        return self.pipe.clone();
    }

    #[cfg(unix)]
    pub fn path(&self) -> &std::path::Path {
        &self.path
    }
}

/// Where the server for `name` ("default" unless the user passes -L) listens.
///
/// Unix: `$XDG_RUNTIME_DIR/rmx/<name>.sock`, else `$TMPDIR/rmx-<uid>/<name>.sock`,
/// else `/tmp/rmx-<uid>/<name>.sock`. Windows: `\\.\pipe\rmx-<SID>-<name>`.
pub fn endpoint(name: &str) -> anyhow::Result<Endpoint> {
    validate_name(name)?;
    sys::endpoint(name)
}

/// `name` ends up in a path or pipe name, so only allow characters that can't
/// escape the directory or collide with our own suffixes.
fn validate_name(name: &str) -> anyhow::Result<()> {
    let ok = !name.is_empty()
        && name.len() <= 64
        && !name.starts_with('.')
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'));
    anyhow::ensure!(
        ok,
        "invalid server name {name:?}: use 1-64 letters, digits, '-', '_' or '.', not starting with '.'"
    );
    Ok(())
}

/// Connects to a running server.
pub async fn connect(ep: &Endpoint) -> anyhow::Result<ClientEnd> {
    sys::connect(ep).await
}

/// Is a server answering on `ep`?
pub async fn is_alive(ep: &Endpoint) -> bool {
    sys::is_alive(ep).await
}

/// Result of [`spawn_detached`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpawnInfo {
    pub pid: u32,
    /// False when Windows refused `CREATE_BREAKAWAY_FROM_JOB`: the server
    /// then dies with the job (e.g. closing that terminal), so warn the user.
    pub breakaway: bool,
}

/// Starts `current_exe() <args...>` fully detached from the calling terminal
/// so it outlives it.
pub fn spawn_detached(args: &[&str]) -> anyhow::Result<SpawnInfo> {
    let exe = std::env::current_exe().context("locating the rmx executable")?;
    sys::spawn_detached(&exe, args)
}

/// Connects, or if no server is running spawns one with `server_args` and
/// waits up to ~3 s for it. A lock (Unix) keeps simultaneous clients from
/// starting two servers; on Windows `FILE_FLAG_FIRST_PIPE_INSTANCE` makes the
/// loser's bind fail, so a plain retry is enough there.
pub async fn connect_or_spawn(
    ep: &Endpoint,
    server_args: &[&str],
) -> anyhow::Result<(ClientEnd, Option<SpawnInfo>)> {
    match connect(ep).await {
        Ok(c) => return Ok((c, None)),
        // Something answers but we refused it (wrong owner, ...): spawning
        // would just fail to bind, so surface the real reason.
        Err(e) if is_alive(ep).await => return Err(e),
        Err(_) => {}
    }
    let _lock = sys::spawn_lock(ep).await?;
    // Another client may have spawned it while we waited for the lock.
    if let Ok(c) = connect(ep).await {
        return Ok((c, None));
    }
    let info = spawn_detached(server_args)?;
    let deadline = Instant::now() + SPAWN_WAIT;
    let mut delay = Duration::from_millis(10);
    loop {
        match connect(ep).await {
            Ok(c) => return Ok((c, Some(info))),
            Err(e) if Instant::now() >= deadline => {
                return Err(e.context(format!(
                    "started server (pid {}) but it did not listen on {} within {}s",
                    info.pid,
                    ep.display(),
                    SPAWN_WAIT.as_secs()
                )));
            }
            Err(_) => {
                tokio::time::sleep(delay).await;
                delay = (delay * 2).min(Duration::from_millis(200));
            }
        }
    }
}

fn server_end<S>(stream: S) -> ServerEnd
where
    S: AsyncRead + AsyncWrite + Send + 'static,
{
    let (tx, rx) = bridge(stream, CLIENT_TO_SERVER_CAP, SERVER_TO_CLIENT_CAP);
    ServerEnd { tx, rx }
}

fn client_end<S>(stream: S) -> ClientEnd
where
    S: AsyncRead + AsyncWrite + Send + 'static,
{
    let (tx, rx) = bridge(stream, SERVER_TO_CLIENT_CAP, CLIENT_TO_SERVER_CAP);
    ClientEnd { tx, rx }
}

/// Spawns a reader and a writer task that pump `stream` to and from channels.
///
/// The two directions close independently (a client may drop its sender and
/// still read the server's goodbye), except that a decode error tears down
/// both: after garbage we can't trust anything else on that stream.
fn bridge<S, In, Out>(
    stream: S,
    in_cap: usize,
    out_cap: usize,
) -> (mpsc::Sender<Out>, mpsc::Receiver<In>)
where
    S: AsyncRead + AsyncWrite + Send + 'static,
    In: DeserializeOwned + Send + 'static,
    Out: Serialize + Send + 'static,
{
    let (mut rd, mut wr) = tokio::io::split(stream);
    let (in_tx, in_rx) = mpsc::channel::<In>(in_cap);
    let (out_tx, mut out_rx) = mpsc::channel::<Out>(out_cap);
    let (kill_tx, kill_rx) = oneshot::channel::<()>();

    tokio::spawn(async move {
        let mut kill_tx = Some(kill_tx);
        let mut buf = vec![0u8; READ_BUF];
        let mut dec = Decoder::default();
        'conn: loop {
            let n = match rd.read(&mut buf).await {
                Ok(0) => break,
                Ok(n) => n,
                Err(e) => {
                    tracing::debug!("ipc read failed: {e}");
                    break;
                }
            };
            dec.push(&buf[..n]);
            loop {
                match dec.try_next::<In>() {
                    Ok(Some(msg)) => {
                        if in_tx.send(msg).await.is_err() {
                            break 'conn;
                        }
                    }
                    Ok(None) => break,
                    Err(e) => {
                        tracing::warn!("closing ipc connection: {e}");
                        if let Some(k) = kill_tx.take() {
                            let _ = k.send(());
                        }
                        break 'conn;
                    }
                }
            }
        }
    });

    tokio::spawn(async move {
        // Only an explicit kill stops the writer; the reader merely finishing
        // (peer half-closed) must not.
        let kill = async {
            if kill_rx.await.is_err() {
                std::future::pending::<()>().await;
            }
        };
        tokio::pin!(kill);
        loop {
            let msg = tokio::select! {
                _ = &mut kill => break,
                msg = out_rx.recv() => match msg {
                    Some(m) => m,
                    None => break,
                },
            };
            let bytes = match encode(&msg) {
                Ok(b) => b,
                Err(e) => {
                    // Skipping one frame would desync the peer's state, so
                    // treat it like any other broken connection.
                    tracing::error!("cannot encode outgoing message, closing connection: {e}");
                    break;
                }
            };
            let res = async {
                wr.write_all(&bytes).await?;
                wr.flush().await
            }
            .await;
            if let Err(e) = res {
                tracing::debug!("ipc write failed: {e}");
                break;
            }
        }
        let _ = wr.shutdown().await;
    });

    (out_tx, in_rx)
}

/// Where the long-lived server runs from. Not the first client's directory:
/// that would keep it from being deleted (Windows) or unmounted (Unix) for as
/// long as the server lives.
pub(crate) fn neutral_dir() -> std::path::PathBuf {
    std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
        .map(std::path::PathBuf::from)
        .filter(|p| p.is_dir())
        .unwrap_or_else(std::env::temp_dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_names_that_could_escape_the_directory() {
        for bad in ["", "../x", "a/b", ".hidden", "a b", &"x".repeat(65)] {
            assert!(validate_name(bad).is_err(), "{bad:?}");
        }
        for good in ["default", "work-2", "a.b_c"] {
            validate_name(good).unwrap();
        }
    }

    #[tokio::test]
    async fn bridge_closes_both_directions_on_garbage() {
        let (a, mut b) = tokio::io::duplex(1024);
        let (tx, mut rx) = bridge::<_, rmx_proto::ClientMsg, rmx_proto::ServerMsg>(a, 8, 8);
        // A valid length prefix followed by a body that isn't a ClientMsg.
        b.write_all(&[1, 0, 0, 0, 0xff]).await.unwrap();
        assert!(rx.recv().await.is_none());
        // The writer was killed too, so sends start failing once it's gone.
        tokio::time::timeout(Duration::from_secs(2), tx.closed())
            .await
            .unwrap();
    }
}
