//! Unix domain sockets in a private per-user directory.
//!
//! Security rests on the directory: it must be ours, 0700 and not a symlink,
//! so nobody else can create, replace or connect to the socket. The socket is
//! additionally 0600 and every peer's uid is checked, in case the directory
//! check is ever bypassed (e.g. by a careless chmod). We never use the Linux
//! abstract namespace, which has no permissions at all.

mod dir;
mod listener;
mod lock;
mod spawn;
#[cfg(test)]
mod tests;

pub(crate) use dir::endpoint;
pub use dir::endpoint_at;
pub use listener::Listener;
pub(crate) use listener::{connect, is_alive};
pub(crate) use lock::spawn_lock;
pub(crate) use spawn::spawn_detached;
