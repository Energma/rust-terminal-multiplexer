use super::*;
use crate::keys::parse_sequence;
use std::collections::HashSet;

#[test]
fn presets_have_no_duplicate_keys() {
    for p in PRESETS {
        let leader = preset_leader(p);
        let mut seen = HashSet::new();
        for (mode, keys, a) in bindings(p, leader, true) {
            assert!(
                seen.insert((mode.clone(), keys.clone())),
                "{p}: duplicate {mode} {keys:?} ({a:?})"
            );
        }
    }
}

#[test]
fn direct_layer_is_opt_in() {
    let ctrl_shift_t = parse_sequence("ctrl+shift+t").unwrap();
    let has = |direct| {
        bindings("windows", preset_leader("windows"), direct)
            .iter()
            .any(|(m, k, _)| m == "normal" && *k == ctrl_shift_t)
    };
    assert!(has(true));
    assert!(!has(false));
}

#[test]
fn tmux_uses_ctrl_b() {
    assert_eq!(preset_leader("tmux"), KeyChord::parse("ctrl+b").unwrap());
    assert_eq!(
        preset_leader("windows"),
        KeyChord::parse("ctrl+space").unwrap()
    );
}
