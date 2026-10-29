//! `flock`-based locks: one held by the live server, one by a client
//! that is spawning it.

use std::fs::{File, OpenOptions};
use std::io;
use std::os::fd::AsRawFd;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, bail};
use tokio::time::Instant;

use super::dir::{ensure_dir, parent};
use crate::Endpoint;

/// Opens (creating 0600) a lock file and tries to take an exclusive `flock`.
/// Returns `Ok(None)` if someone else holds it. The lock lives as long as the
/// returned `File`; std opens with `O_CLOEXEC`, so pane shells never inherit it.
pub(super) fn try_lock(path: &Path) -> anyhow::Result<Option<File>> {
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)
        .with_context(|| format!("opening lock file {}", path.display()))?;
    // SAFETY: the fd is valid for the duration of the call.
    if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } == 0 {
        return Ok(Some(file));
    }
    let err = io::Error::last_os_error();
    if err.kind() == io::ErrorKind::WouldBlock {
        Ok(None)
    } else {
        Err(err).with_context(|| format!("locking {}", path.display()))
    }
}

/// Held by the server for its whole life. It makes "is a server already
/// running?" race-free, so removing a stale socket can never delete the
/// fresh one of a server that is starting at the same moment.
pub(super) fn server_lock_path(ep: &Endpoint) -> PathBuf {
    ep.path.with_extension("lock")
}

/// Held by a client between "nobody answers" and "the server it spawned
/// answers". Separate from the server lock, which the spawned server needs
/// while this one is still held.
/// A distinct extension (not `spawn.lock`) so the spawn lock of server `x`
/// can never be the server lock of a server named `x.spawn`.
pub(super) fn spawn_lock_path(ep: &Endpoint) -> PathBuf {
    ep.path.with_extension("spawnlock")
}

pub(crate) async fn spawn_lock(ep: &Endpoint) -> anyhow::Result<File> {
    ensure_dir(parent(ep)?)?;
    let path = spawn_lock_path(ep);
    // Generous: the holder may itself be waiting ~3 s for its server.
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(f) = try_lock(&path)? {
            return Ok(f);
        }
        if Instant::now() >= deadline {
            bail!("timed out waiting for {}", path.display());
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}
