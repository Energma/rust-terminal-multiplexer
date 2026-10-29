//! Windows named pipes, locked down per docs/PLAN.md §5.9.
//!
//! A default named pipe is readable by Everyone, so every instance gets an
//! explicit protected DACL (SYSTEM + current user only) and `O:` pinned to our
//! SID. The first instance uses `FILE_FLAG_FIRST_PIPE_INSTANCE`, so a squatter
//! can't sit on the name and pose as our server; and clients check the pipe's
//! owner SID, which only our own user can set, before trusting it. Remote
//! (SMB) clients are rejected, and tokio's client default of
//! `SECURITY_IDENTIFICATION` QoS keeps a server from impersonating us.

mod listener;
mod security;
mod sid;
mod spawn;
#[cfg(test)]
mod tests;

pub use listener::Listener;
pub(crate) use listener::{connect, is_alive};
pub(crate) use sid::endpoint;
pub use sid::endpoint_at;
pub(crate) use spawn::{spawn_detached, spawn_lock};
