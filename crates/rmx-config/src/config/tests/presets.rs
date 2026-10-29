use ratatui::style::Color;

use super::*;
use crate::action::{Action, Dir};
use crate::keymap::Source;
use crate::keys::parse_sequence;
use crate::presets::PRESETS;

#[test]
fn plan_example_parses() {
    let src = r##"
preset = "windows"
direct_keys = "auto"

[general]
shell = "auto"
scrollback = 5000
mouse = false

[leader]
key = "ctrl+a"
timeout_ms = 1500

[theme]
status_bg = "#101010"
active_border = "blue"

[keys.normal]
"f10" = "open-menu"
"alt+1" = { action = "go-to-tab", index = 1 }
"ctrl+a z" = "toggle-pane-zoom"

[keys.leader]
"|" = { action = "split-pane", direction = "right" }
"-" = { action = "split-pane", direction = "down", size = "30%" }
"g" = { action = "run", command = "lazygit", in = "new-tab" }

[keys.resize]
"left" = { action = "resize-pane", direction = "left", amount = 2 }
"esc" = "exit-mode"

[[commands]]
name = "Open htop"
action = { action = "run", command = "htop", in = "split-right" }
"##;
    let (c, w) = parse(src);
    assert!(w.is_empty(), "{w:?}");
    assert_eq!(c.general.shell, None);
    assert_eq!(c.general.scrollback, 5000);
    assert!(!c.general.mouse);
    assert_eq!(c.leader.timeout_ms, 1500);
    assert_eq!(c.theme.status_bg, Color::Rgb(0x10, 0x10, 0x10));
    assert_eq!(c.theme.active_border, Color::Blue);
    assert_eq!(c.commands.len(), 1);
    assert_eq!(c.commands[0].name, "Open htop");
    let km = c.keymap(false);
    assert_eq!(km.leader(), KeyChord::parse("ctrl+a").unwrap());
    let z = km
        .bindings("leader")
        .iter()
        .find(|b| b.keys == parse_sequence("z").unwrap())
        .unwrap();
    assert_eq!(z.source, Source::User);
}

#[test]
fn preset_merge_override_and_unbind() {
    let (c, _) = parse(
        r#"
[keys.leader]
"t" = "none"
"w" = "quit"
"#,
    );
    let km = c.keymap(false);
    let leader = km.bindings("leader");
    assert!(
        !leader
            .iter()
            .any(|b| b.keys == parse_sequence("t").unwrap())
    );
    let w = leader
        .iter()
        .find(|b| b.keys == parse_sequence("w").unwrap())
        .unwrap();
    assert_eq!((w.action.clone(), w.source), (Action::Quit, Source::User));
    assert!(leader.iter().any(|b| b.source == Source::Preset));
}

#[test]
fn tmux_preset_defaults_leader() {
    let (c, _) = parse("preset = \"tmux\"");
    assert_eq!(c.leader.key, KeyChord::parse("ctrl+b").unwrap());
    let km = c.keymap(true);
    assert!(
        km.bindings("leader")
            .iter()
            .any(|b| b.keys == parse_sequence("%").unwrap()
                && b.action
                    == Action::SplitPane {
                        direction: Dir::Right,
                        size: None,
                        command: None
                    })
    );
    // tmux has no direct layer even when requested.
    assert!(
        !km.bindings("normal")
            .iter()
            .any(|b| b.keys == parse_sequence("ctrl+shift+t").unwrap())
    );
}

#[test]
fn every_preset_is_clean() {
    for p in PRESETS {
        let (c, w) = parse(&format!("preset = \"{p}\""));
        assert!(w.is_empty(), "{p}: {w:?}");
        for direct in [false, true] {
            let km = c.keymap(direct);
            for mode in km.modes() {
                let b = km.bindings(mode);
                for (i, x) in b.iter().enumerate() {
                    for y in &b[i + 1..] {
                        assert!(
                            !x.keys.starts_with(&y.keys) && !y.keys.starts_with(&x.keys),
                            "{p}/{mode}: {:?} vs {:?}",
                            x.keys,
                            y.keys
                        );
                    }
                }
            }
            // Escape hatches exist in every preset.
            assert!(!km.bindings("locked").is_empty(), "{p}");
            assert!(km.key_hint(&Action::OpenMenu).is_some(), "{p}");
        }
    }
}
