//! Messages between the rust-multiplexer client and server.
//!
//! The client sends logical input events; the server owns all state and sends
//! back composed frames as cell diffs (tmux passes the tty fd to its server
//! instead, which is impossible on Windows). See docs/PLAN.md §4.1 and §5.8.

mod channel;
pub mod codec;
mod msg;
mod wire;

pub use channel::{ClientEnd, ServerEnd, in_memory};
pub use msg::{Attach, ClientMsg, Frame, Hello, PROTOCOL_VERSION, Request, ServerMsg};
pub use wire::{WireCell, WireColor};
