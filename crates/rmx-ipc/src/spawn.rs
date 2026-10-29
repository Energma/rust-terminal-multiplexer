//! Starting a detached server and connecting to it once it listens.

use std::time::Duration;

use anyhow::Context;
use rmx_proto::ClientEnd;
use tokio::time::Instant;

use crate::{Endpoint, connect, is_alive, sys};

/// How long `connect_or_spawn` waits for a freshly spawned server.
const SPAWN_WAIT: Duration = Duration::from_secs(3);

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
