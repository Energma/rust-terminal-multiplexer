//! Mode and pending-chord state of a [`KeyEngine`]: queries and transitions.

use std::time::Instant;

use super::KeyEngine;
use crate::keymap::Keymap;

impl KeyEngine {
    pub fn new() -> Self {
        Self {
            mode: "normal".into(),
            sticky: false,
            pending: Vec::new(),
            since: None,
        }
    }

    pub fn mode(&self) -> &str {
        &self.mode
    }

    pub fn is_sticky(&self) -> bool {
        self.sticky
    }

    /// Switches mode and clears any pending chord. Non-sticky modes return to
    /// normal after one action, an unbound key, or the timeout.
    pub fn set_mode(&mut self, mode: &str, sticky: bool) {
        self.enter(mode, sticky, Instant::now());
    }

    pub fn exit_mode(&mut self) {
        self.mode = "normal".into();
        self.sticky = false;
        self.pending.clear();
        self.since = None;
    }

    /// True while a chord is half-typed or a one-shot mode (like leader) waits for its key.
    pub fn is_pending(&self) -> bool {
        !self.pending.is_empty() || self.one_shot()
    }

    /// Hints for what can follow right now (for a which-key popup).
    pub fn pending_hints(&self, keymap: &Keymap) -> Vec<(String, String)> {
        keymap.hints(&self.mode, &self.pending)
    }

    pub(super) fn one_shot(&self) -> bool {
        !self.sticky && self.mode != "normal"
    }

    pub(super) fn enter(&mut self, mode: &str, sticky: bool, now: Instant) {
        if mode == "normal" {
            self.exit_mode();
            return;
        }
        self.mode = mode.into();
        self.sticky = sticky;
        self.pending.clear();
        self.since = Some(now);
    }
}
