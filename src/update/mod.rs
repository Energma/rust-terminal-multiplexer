//! `rmx update`: replace this binary with the latest GitHub release.
//!
//! Downloads go through `curl` and archives through `tar`, which Linux,
//! macOS and Windows 10+ all ship, so rmx needs no HTTP or TLS crate and
//! stays a small binary. The archive's SHA-256 is checked against the
//! `.sha256` file published next to it before anything is replaced.

mod command;
mod install;
mod release;

#[cfg(test)]
mod tests;

pub use command::run;

const REPO: &str = "Energma/rust-terminal-multiplexer";
