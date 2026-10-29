//! The real transport between `rmx` clients and the detached server
//! (docs/PLAN.md §4.1, §5.9): it bridges a local stream onto the same
//! [`ClientEnd`]/[`ServerEnd`](rmx_proto::ServerEnd) channels as
//! [`rmx_proto::in_memory`], so the server core and client never know which
//! transport they run on.
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

use rmx_proto::ClientEnd;

mod bridge;
mod endpoint;
mod spawn;
#[cfg(test)]
mod tests;
#[cfg(unix)]
mod unix;
#[cfg(unix)]
use unix as sys;
#[cfg(windows)]
mod windows;
#[cfg(windows)]
use windows as sys;

pub use endpoint::{Endpoint, endpoint};
pub use spawn::{SpawnInfo, connect_or_spawn, spawn_detached};
pub use sys::Listener;
#[cfg(unix)]
#[doc(hidden)]
pub use unix::endpoint_at;
#[cfg(windows)]
#[doc(hidden)]
pub use windows::endpoint_at;

/// Connects to a running server.
pub async fn connect(ep: &Endpoint) -> anyhow::Result<ClientEnd> {
    sys::connect(ep).await
}

/// Is a server answering on `ep`?
pub async fn is_alive(ep: &Endpoint) -> bool {
    sys::is_alive(ep).await
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
