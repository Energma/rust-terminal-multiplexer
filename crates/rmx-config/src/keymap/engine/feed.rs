//! Resolving key events against a [`Keymap`], and timeouts.

use std::time::Instant;

use crossterm::event::{KeyEvent, KeyEventKind};

use super::{KeyEngine, KeyOutcome};
use crate::action::Action;
use crate::keymap::Keymap;
use crate::keys::KeyChord;

impl KeyEngine {
    /// Resolves one key event. In normal mode an unmatched key passes through; when
    /// a multi-chord prefix was pending, only the *last* key passes through (the
    /// buffered prefix keys are dropped), which keeps the engine stateless toward
    /// the pane at the cost of losing a rarely-meant prefix key.
    ///
    /// Pressing the leader key while in leader mode passes it through, so nested
    /// multiplexers and programs that use it still receive it (tmux `send-prefix`).
    pub fn feed(&mut self, keymap: &Keymap, key: &KeyEvent, now: Instant) -> KeyOutcome {
        if key.kind == KeyEventKind::Release {
            return KeyOutcome::Ignored;
        }
        self.tick(keymap, now);
        let chord = KeyChord::from_event(key);

        if self.mode == "normal"
            && self.pending.is_empty()
            && chord == keymap.leader()
            && !keymap.bindings("normal").iter().any(|b| b.keys == [chord])
        {
            self.enter("leader", false, now);
            return KeyOutcome::Pending {
                hints: keymap.hints("leader", &[]),
            };
        }

        let was_pending = !self.pending.is_empty();
        let mut seq = std::mem::take(&mut self.pending);
        seq.push(chord);
        let table = keymap.bindings(&self.mode);

        if let Some(b) = table.iter().find(|b| b.keys == seq) {
            let action = b.action.clone();
            self.after_action(&action, now);
            return KeyOutcome::Action(action);
        }
        if table
            .iter()
            .any(|b| b.keys.len() > seq.len() && b.keys.starts_with(&seq))
        {
            let hints = keymap.hints(&self.mode, &seq);
            self.pending = seq;
            self.since = Some(now);
            return KeyOutcome::Pending { hints };
        }

        match self.mode.as_str() {
            "normal" | "locked" => {
                if self.mode == "normal" {
                    self.since = None;
                }
                KeyOutcome::PassThrough
            }
            "leader" if !was_pending && chord == keymap.leader() => {
                self.exit_mode();
                KeyOutcome::PassThrough
            }
            _ => {
                if self.sticky {
                    self.since = None;
                } else {
                    self.exit_mode();
                }
                KeyOutcome::Ignored
            }
        }
    }

    fn after_action(&mut self, action: &Action, now: Instant) {
        match action {
            Action::EnterMode { mode, sticky } => self.enter(mode, *sticky, now),
            Action::ExitMode => self.exit_mode(),
            Action::Lock => self.enter("locked", true, now),
            Action::SelectMode => self.enter("select", true, now),
            _ if self.one_shot() => self.exit_mode(),
            _ => self.since = None,
        }
    }

    /// Call periodically; returns true if a pending chord or a one-shot mode timed
    /// out and was cancelled.
    pub fn tick(&mut self, keymap: &Keymap, now: Instant) -> bool {
        let (Some(timeout), Some(since)) = (keymap.timeout(), self.since) else {
            return false;
        };
        if !self.is_pending() || now.duration_since(since) < timeout {
            return false;
        }
        self.pending.clear();
        self.since = None;
        if self.one_shot() {
            self.exit_mode();
        }
        true
    }
}
