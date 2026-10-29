//! Building the merged key tables (preset + user) and reading them back.

use std::collections::BTreeMap;
use std::time::Duration;

use super::{Binding, Keymap, MODES, Source};
use crate::action::Action;
use crate::keys::KeyChord;

impl Keymap {
    /// Merges `user` over `preset`: a user entry replaces any binding with the same
    /// keys in that mode, and `None` (`"none"` in TOML) just removes it.
    ///
    /// Normal-mode sequences that start with the leader key (`"ctrl+space z"`) are
    /// folded into the leader table (`"z"`), since the engine consumes the leader
    /// key by entering leader mode.
    pub(crate) fn build(
        leader: KeyChord,
        timeout_ms: u64,
        preset: impl IntoIterator<Item = (String, Vec<KeyChord>, Action)>,
        user: impl IntoIterator<Item = (String, Vec<KeyChord>, Option<Action>)>,
    ) -> Self {
        let mut tables: BTreeMap<String, Vec<Binding>> = BTreeMap::new();
        let preset = preset
            .into_iter()
            .map(|(m, k, a)| (m, k, Some(a), Source::Preset));
        let user = user.into_iter().map(|(m, k, a)| (m, k, a, Source::User));
        for (mode, keys, action, source) in preset.chain(user) {
            let (mode, keys) = fold_leader(mode, keys, leader);
            let table = tables.entry(mode).or_default();
            table.retain(|b| b.keys != keys);
            if let Some(action) = action {
                table.push(Binding {
                    keys,
                    action,
                    source,
                });
            }
        }
        Self {
            tables,
            leader,
            timeout_ms,
        }
    }

    pub fn bindings(&self, mode: &str) -> &[Binding] {
        self.tables.get(mode).map_or(&[], Vec::as_slice)
    }

    /// Built-in modes first, then user-defined ones.
    pub fn modes(&self) -> Vec<&str> {
        let mut v: Vec<&str> = MODES.to_vec();
        v.extend(
            self.tables
                .keys()
                .map(String::as_str)
                .filter(|m| !MODES.contains(m)),
        );
        v
    }

    pub fn leader(&self) -> KeyChord {
        self.leader
    }

    /// `None` when chords and leader mode never time out.
    pub fn timeout(&self) -> Option<Duration> {
        (self.timeout_ms > 0).then(|| Duration::from_millis(self.timeout_ms))
    }
}

pub(crate) fn fold_leader(
    mode: String,
    keys: Vec<KeyChord>,
    leader: KeyChord,
) -> (String, Vec<KeyChord>) {
    if mode == "normal" && keys.len() > 1 && keys[0] == leader {
        ("leader".into(), keys[1..].to_vec())
    } else {
        (mode, keys)
    }
}
