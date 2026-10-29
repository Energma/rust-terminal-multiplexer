//! The `rmx` command line: argument definitions and the handlers each
//! subcommand dispatches to.

mod args;
mod config;
mod dispatch;
mod doctor;
mod ipc;
mod server;
mod session;
mod workspace;

#[cfg(test)]
mod tests;

pub(crate) use args::{Cli, Command};
pub(crate) use dispatch::run;
