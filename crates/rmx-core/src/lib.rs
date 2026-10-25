//! Pure logic shared by the rust-multiplexer server and client.
//!
//! Nothing in this crate touches a PTY, socket or real terminal, so all of it
//! can be unit-tested on every OS.

pub mod emulator;
pub mod encode;
pub mod layout;
