//! Binding, accepting on and connecting to the socket, with peer uid checks.

use std::fs::{self, File};
use std::io;
use std::os::unix::fs::{FileTypeExt, PermissionsExt};
use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Context, bail};
use rmx_proto::{ClientEnd, ServerEnd};
use tokio::net::{UnixListener, UnixStream};

use super::dir::{check_dir, ensure_dir, euid, parent};
use super::lock::{server_lock_path, try_lock};
use crate::Endpoint;
use crate::bridge::{client_end, server_end};

pub struct Listener {
    inner: UnixListener,
    path: PathBuf,
    _lock: File,
}

impl Listener {
    /// Creates the endpoint securely and errors clearly if a live server
    /// already listens there.
    pub async fn bind(ep: &Endpoint) -> anyhow::Result<Listener> {
        ensure_dir(parent(ep)?)?;
        let already = || anyhow::anyhow!("an rmx server is already running on {}", ep.display());
        let lock = try_lock(&server_lock_path(ep))?.ok_or_else(already)?;

        match fs::symlink_metadata(&ep.path) {
            Ok(meta) => {
                // Lock is free, so this should be a leftover from a crash —
                // but double-check before deleting anything.
                if is_alive(ep).await {
                    return Err(already());
                }
                if !meta.file_type().is_socket() {
                    bail!(
                        "{} exists and is not a socket; refusing to remove it",
                        ep.display()
                    );
                }
                fs::remove_file(&ep.path)
                    .with_context(|| format!("removing stale socket {}", ep.display()))?;
                tracing::info!("removed stale socket {}", ep.display());
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => return Err(e).with_context(|| format!("inspecting {}", ep.display())),
        }

        let inner =
            UnixListener::bind(&ep.path).with_context(|| format!("binding {}", ep.display()))?;
        // chmod after bind rather than a umask dance (umask is process-wide
        // and racy with other threads); the 0700 directory covers the gap.
        fs::set_permissions(&ep.path, fs::Permissions::from_mode(0o600))
            .with_context(|| format!("restricting {}", ep.display()))?;
        Ok(Listener {
            inner,
            path: ep.path.clone(),
            _lock: lock,
        })
    }

    /// Waits for the next client whose uid matches ours; others are logged
    /// and dropped.
    pub async fn accept(&mut self) -> anyhow::Result<ServerEnd> {
        loop {
            let (stream, _) = self.inner.accept().await.context("accepting a client")?;
            match stream.peer_cred() {
                Ok(cred) if cred.uid() == euid() => return Ok(server_end(stream)),
                Ok(cred) => tracing::warn!("rejected client with uid {}", cred.uid()),
                Err(e) => tracing::warn!("rejected client: cannot read its credentials: {e}"),
            }
        }
    }
}

impl Drop for Listener {
    fn drop(&mut self) {
        // Safe to remove unconditionally: we still hold the server lock, so
        // nobody else can have bound this path since.
        let _ = fs::remove_file(&self.path);
    }
}

pub(crate) async fn connect(ep: &Endpoint) -> anyhow::Result<ClientEnd> {
    // Don't talk to a socket in a directory someone else could control.
    check_dir(parent(ep)?)?;
    let stream = UnixStream::connect(&ep.path)
        .await
        .with_context(|| format!("connecting to {}", ep.display()))?;
    let cred = stream.peer_cred().context("reading server credentials")?;
    if cred.uid() != euid() {
        bail!(
            "server on {} runs as uid {}, not us; refusing to connect",
            ep.display(),
            cred.uid()
        );
    }
    Ok(client_end(stream))
}

pub(crate) async fn is_alive(ep: &Endpoint) -> bool {
    // A stale socket refuses immediately; the timeout only guards against a
    // wedged server whose backlog is full.
    matches!(
        tokio::time::timeout(Duration::from_secs(1), UnixStream::connect(&ep.path)).await,
        Ok(Ok(_))
    )
}
