//! The rust-multiplexer client: a thin terminal front end.
//!
//! It forwards input as logical events and paints the cell diffs the server
//! sends. It holds no session state, which is what makes detach possible.
//! The one thing it owns is the system clipboard, since that belongs to the
//! machine the user sits at.

mod clipboard;
mod input;
mod render;
mod session;
mod terminal;

#[cfg(test)]
mod tests;

use rmx_proto::Attach;

pub use session::run;

/// How this client wants to join the server.
#[derive(Debug, Clone)]
pub struct Options {
    pub attach: Attach,
    pub open_manager: bool,
    pub cwd: Option<String>,
}
