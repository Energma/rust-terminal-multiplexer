use std::time::{Duration, Instant};

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};

use super::*;
use crate::action::{Action, Dir};
use crate::keys::{KeyChord, parse_sequence};

mod leader;
mod modes;

fn seq(s: &str) -> Vec<KeyChord> {
    parse_sequence(s).unwrap()
}

fn ev(s: &str) -> KeyEvent {
    let c = KeyChord::parse(s).unwrap();
    KeyEvent {
        code: c.code,
        modifiers: c.mods,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    }
}

fn map(user: &[(&str, &str, Option<Action>)]) -> Keymap {
    let leader = KeyChord::parse("ctrl+space").unwrap();
    Keymap::build(
        leader,
        2000,
        crate::presets::bindings("windows", leader, false),
        user.iter()
            .map(|(m, k, a)| (m.to_string(), seq(k), a.clone())),
    )
}

fn split_right() -> Action {
    Action::SplitPane {
        direction: Dir::Right,
        size: None,
        command: None,
    }
}

#[test]
fn user_overrides_and_unbinds() {
    let km = map(&[("leader", "t", Some(Action::Quit)), ("normal", "f10", None)]);
    let t_binding = km
        .bindings("leader")
        .iter()
        .find(|b| b.keys == seq("t"))
        .unwrap();
    assert_eq!(t_binding.action, Action::Quit);
    assert_eq!(t_binding.source, Source::User);
    assert!(!km.bindings("normal").iter().any(|b| b.keys == seq("f10")));
    assert_eq!(
        km.bindings("leader")
            .iter()
            .find(|b| b.keys == seq("p"))
            .unwrap()
            .source,
        Source::Preset
    );
}

#[test]
fn key_hints() {
    let km = map(&[]);
    assert_eq!(km.key_hint(&Action::OpenMenu).as_deref(), Some("F10"));
    assert_eq!(km.key_hint(&split_right()).as_deref(), Some("^Space |"));
    assert_eq!(
        km.key_hint(&Action::GoToTab { index: 2 }).as_deref(),
        Some("Alt+2")
    );
    assert_eq!(km.key_hint(&Action::Quit), None);
    assert_eq!(
        km.key_hint_in("select", &Action::CopyAndExit).as_deref(),
        Some("Enter")
    );
}

#[test]
fn modes_list_builtins_then_user() {
    let km = map(&[("git", "s", Some(Action::Quit))]);
    let modes = km.modes();
    assert_eq!(&modes[..5], MODES);
    assert!(modes.contains(&"git"));
}
