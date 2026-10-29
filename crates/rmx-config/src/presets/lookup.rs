//! Name-based lookup: a preset's leader key and its assembled binding list
//! (including the minimal `none` preset, which is defined inline here).

use super::PresetBinding;
use super::builder::Builder;
use super::common::common_modes;
use super::tmux::tmux_leader;
use super::windows::{direct_layer, windows_leader, windows_normal};
use crate::action::Action;
use crate::keys::KeyChord;

/// The leader key a preset uses unless `[leader] key` overrides it.
pub fn preset_leader(preset: &str) -> KeyChord {
    let key = if preset == "tmux" {
        "ctrl+b"
    } else {
        "ctrl+space"
    };
    KeyChord::parse(key).expect("valid preset leader")
}

/// Bindings for `preset` with the given leader; `direct` adds the Ctrl+Shift layer
/// for presets that have one. Unknown presets yield nothing (config validates names).
pub(crate) fn bindings(preset: &str, leader: KeyChord, direct: bool) -> Vec<PresetBinding> {
    let mut b = Builder::default();
    match preset {
        "windows" => {
            windows_normal(&mut b);
            windows_leader(&mut b, false);
            if direct {
                direct_layer(&mut b);
            }
            common_modes(&mut b, leader);
        }
        "vim" => {
            windows_normal(&mut b);
            windows_leader(&mut b, true);
            if direct {
                direct_layer(&mut b);
            }
            common_modes(&mut b, leader);
        }
        "tmux" => {
            b.bind("normal", "f10", Action::OpenMenu);
            tmux_leader(&mut b);
            common_modes(&mut b, leader);
        }
        "none" => {
            b.bind("normal", "f10", Action::OpenMenu);
            b.bind("leader", "?", Action::ShowHelp);
            b.bind("select", "esc", Action::ExitMode);
            b.bind("resize", "esc", Action::ExitMode);
            b.v.push(("locked".into(), vec![leader], Action::ExitMode));
        }
        _ => {}
    }
    b.v
}
