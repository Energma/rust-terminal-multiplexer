//! Non-fatal binding warnings: leader shadowing, terminal-indistinguishable
//! chords, Windows Terminal reserved keys and prefix conflicts.

use std::ops::Range;

use super::Parser;
use crate::diag::Diag;
use crate::keymap::fold_leader;
use crate::keys::KeyChord;

/// Keys Windows Terminal binds by default, so they never reach rmx there.
const WT_KEYS: &[&str] = &[
    "ctrl+shift+t",
    "ctrl+shift+w",
    "ctrl+shift+p",
    "ctrl+shift+f",
    "ctrl+shift+c",
    "ctrl+shift+v",
    "ctrl+tab",
    "ctrl+shift+tab",
    "alt+left",
    "alt+right",
    "alt+up",
    "alt+down",
    "alt+shift+left",
    "alt+shift+right",
    "alt+shift+up",
    "alt+shift+down",
    "ctrl+alt+1",
    "ctrl+alt+2",
    "ctrl+alt+3",
    "ctrl+alt+4",
    "ctrl+alt+5",
    "ctrl+alt+6",
    "ctrl+alt+7",
    "ctrl+alt+8",
    "ctrl+alt+9",
];

impl Parser<'_> {
    pub(super) fn key_warnings(
        &mut self,
        mode: &str,
        keys: &[KeyChord],
        shown: &str,
        span: Range<usize>,
    ) {
        if mode == "normal" && keys == [self.config.leader.key] {
            self.warnings.push(
                Diag::new(
                    span.clone(),
                    format!("\"{shown}\" is the leader key; binding it in normal mode disables leader mode"),
                )
                .help("bind it under [keys.leader] instead, or change [leader] key"),
            );
        }
        for chord in keys {
            let canon = chord.to_string();
            if chord.is_ctrl_shift_letter() {
                let unshifted = KeyChord {
                    code: chord.code,
                    mods: chord.mods - crossterm::event::KeyModifiers::SHIFT,
                };
                self.warnings.push(
                    Diag::new(
                        span.clone(),
                        format!("\"{canon}\" can't be told apart from \"{unshifted}\" in most terminals"),
                    )
                    .help("it works on Windows and in terminals with the kitty keyboard protocol; set direct_keys = \"on\" if yours supports it"),
                );
            }
            if WT_KEYS.contains(&canon.as_str()) {
                self.warnings.push(
                    Diag::new(
                        span.clone(),
                        format!(
                            "Windows Terminal uses \"{canon}\" itself, so rmx won't see it there"
                        ),
                    )
                    .help("`rmx doctor` prints a settings.json snippet that frees these keys"),
                );
            }
        }
    }

    /// A key that is both a binding and the start of a longer one makes the
    /// longer one unreachable (the short one fires immediately).
    pub(super) fn prefix_warnings(&mut self) {
        let km = self.config.keymap(true);
        let leader = self.config.leader.key;
        for b in &self.config.user_keys {
            if b.action.is_none() {
                continue;
            }
            let (mode, keys) = fold_leader(b.mode.clone(), b.keys.clone(), leader);
            let conflict = km.bindings(&mode).iter().find(|o| {
                o.keys != keys && (o.keys.starts_with(&keys) || keys.starts_with(&o.keys))
            });
            if let Some(o) = conflict {
                let (short, long) = if o.keys.len() < keys.len() {
                    (&o.keys, &keys)
                } else {
                    (&keys, &o.keys)
                };
                let fmt = |k: &[KeyChord]| {
                    k.iter()
                        .map(|c| c.to_string())
                        .collect::<Vec<_>>()
                        .join(" ")
                };
                self.warnings.push(
                    Diag::new(
                        b.span.clone(),
                        format!(
                            "\"{}\" is bound and also starts \"{}\" in {mode} mode; the longer binding can never fire",
                            fmt(short),
                            fmt(long)
                        ),
                    )
                    .help("unbind one of them with \"none\""),
                );
            }
        }
    }
}
