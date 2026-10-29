//! Pumping a byte stream to and from typed message channels.

use rmx_proto::codec::{Decoder, encode};
use rmx_proto::{ClientEnd, ServerEnd};
use serde::Serialize;
use serde::de::DeserializeOwned;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::sync::{mpsc, oneshot};

/// Same capacities as `rmx_proto::in_memory`: input may queue up, but frames
/// to a slow client must apply back-pressure rather than pile up.
const CLIENT_TO_SERVER_CAP: usize = 256;
const SERVER_TO_CLIENT_CAP: usize = 8;
const READ_BUF: usize = 64 * 1024;

pub(crate) fn server_end<S>(stream: S) -> ServerEnd
where
    S: AsyncRead + AsyncWrite + Send + 'static,
{
    let (tx, rx) = bridge(stream, CLIENT_TO_SERVER_CAP, SERVER_TO_CLIENT_CAP);
    ServerEnd { tx, rx }
}

pub(crate) fn client_end<S>(stream: S) -> ClientEnd
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
pub(super) fn bridge<S, In, Out>(
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
