//! The per-client key engine: a small state machine (mode, sticky flag, chord
//! buffer) with no I/O and an injected clock, so timeouts are testable without
//! sleeping.

mod feed;
mod state;

use std::time::Instant;

use crate::action::Action;
use crate::keys::KeyChord;

#[derive(Debug, Clone, PartialEq)]
pub enum KeyOutcome {
    Action(Action),
    /// Waiting for more keys; hints = (key display, action label) of possible continuations.
    Pending {
        hints: Vec<(String, String)>,
    },
    /// Not a binding: send the key to the focused pane.
    PassThrough,
    /// Swallowed (e.g. unbound key in leader/select/resize mode).
    Ignored,
}

/// Per-client key state. Mode changes requested by `enter-mode`, `exit-mode`,
/// `lock` and `select-mode` are applied by [`feed`](Self::feed) itself before the
/// action is returned, so the next key is already read in the right mode even if
/// the caller forgets to call `set_mode`; calling it again is harmless.
#[derive(Debug, Clone)]
pub struct KeyEngine {
    mode: String,
    sticky: bool,
    pending: Vec<KeyChord>,
    /// Start of the current timeout window: a pending chord or a non-sticky mode.
    since: Option<Instant>,
}

impl Default for KeyEngine {
    fn default() -> Self {
        Self::new()
    }
}
