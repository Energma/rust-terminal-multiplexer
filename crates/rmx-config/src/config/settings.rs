//! Plain setting groups of a [`Config`](super::Config): `[general]`, `[leader]`,
//! `direct_keys` and `[[commands]]` entries.

use crate::action::Action;
use crate::keys::KeyChord;
use crate::presets;

/// Whether the Ctrl+Shift "direct" layer is active (PLAN §5.5).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DirectKeys {
    /// On when the terminal can report Ctrl+Shift keys distinctly.
    Auto,
    On,
    Off,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct General {
    /// `None` = detect (`$SHELL`, `pwsh`, ...); `"auto"` in the file.
    pub shell: Option<String>,
    pub scrollback: usize,
    pub mouse: bool,
    pub copy_on_select: bool,
    pub restore_on_start: bool,
}

impl Default for General {
    fn default() -> Self {
        Self {
            shell: None,
            scrollback: 10_000,
            mouse: true,
            copy_on_select: true,
            restore_on_start: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LeaderConfig {
    pub key: KeyChord,
    /// 0 = leader mode and pending chords never time out.
    pub timeout_ms: u64,
    pub show_hints: bool,
}

impl LeaderConfig {
    pub(super) fn for_preset(preset: &str) -> Self {
        Self {
            key: presets::preset_leader(preset),
            timeout_ms: 2000,
            show_hints: true,
        }
    }
}

/// An extra command-palette entry from `[[commands]]`.
#[derive(Debug, Clone, PartialEq)]
pub struct CustomCommand {
    pub name: String,
    pub action: Action,
}
