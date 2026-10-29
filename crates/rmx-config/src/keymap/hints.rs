//! Human-readable key hints for menus, status bars and the which-key popup.

use super::Keymap;
use crate::action::Action;
use crate::keys::{KeyChord, display_sequence};

impl Keymap {
    /// Shortest binding for `action` reachable from normal mode, for UI hints:
    /// `"F10"`, `"^Space |"`. Ties prefer direct (normal-mode) keys over leader ones.
    pub fn key_hint(&self, action: &Action) -> Option<String> {
        let direct = self
            .bindings("normal")
            .iter()
            .filter(|b| b.action == *action)
            .map(|b| b.keys.clone());
        let via_leader = self
            .bindings("leader")
            .iter()
            .filter(|b| b.action == *action)
            .map(|b| {
                let mut k = vec![self.leader];
                k.extend(&b.keys);
                k
            });
        // min_by_key keeps the first minimum, so normal-mode keys win ties.
        direct
            .chain(via_leader)
            .min_by_key(Vec::len)
            .map(|k| display_sequence(&k))
    }

    /// Like [`key_hint`](Self::key_hint) but for one mode's own table (e.g. the
    /// select-mode status bar).
    pub fn key_hint_in(&self, mode: &str, action: &Action) -> Option<String> {
        self.bindings(mode)
            .iter()
            .filter(|b| b.action == *action)
            .min_by_key(|b| b.keys.len())
            .map(|b| display_sequence(&b.keys))
    }

    /// Continuations of `prefix` in `mode`: (remaining keys, action label).
    pub fn hints(&self, mode: &str, prefix: &[KeyChord]) -> Vec<(String, String)> {
        self.bindings(mode)
            .iter()
            .filter(|b| b.keys.len() > prefix.len() && b.keys.starts_with(prefix))
            .map(|b| (display_sequence(&b.keys[prefix.len()..]), b.action.label()))
            .collect()
    }
}
