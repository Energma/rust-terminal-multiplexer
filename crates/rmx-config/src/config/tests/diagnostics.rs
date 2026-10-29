use super::*;
use crate::action::Action;
use crate::keys::parse_sequence;

#[test]
fn unknown_action_error_has_location_and_suggestion() {
    let src = "[keys.leader]\n\"|\" = \"split-pain\"\n";
    let e = err(src);
    assert_eq!((e.line, e.col), (2, 7));
    assert_eq!(e.message, "unknown action \"split-pain\"");
    assert_eq!(e.help.as_deref(), Some("did you mean \"split-pane\"?"));
    let shown = e.to_string();
    assert!(
        shown.starts_with(
            "config.toml:2:7: unknown action \"split-pain\"\n  help: did you mean \"split-pane\"?"
        ),
        "{shown}"
    );
    assert!(
        shown.contains("2 | \"|\" = \"split-pain\"\n  |       ^^^^^^^^^^^^"),
        "{shown}"
    );
}

#[test]
fn error_locations() {
    let e = err("preset = \"windows\"\nfoo = 1\n");
    assert_eq!((e.line, e.col), (2, 1));
    assert!(e.message.contains("unknown setting `foo`"));

    let e = err("[genral]\nmouse = true\n");
    assert_eq!((e.line, e.col), (1, 2));
    assert_eq!(e.help.as_deref(), Some("did you mean `general`?"));

    let e = err("[general]\nscrollbak = 5\n");
    assert_eq!((e.line, e.col), (2, 1));
    assert_eq!(e.help.as_deref(), Some("did you mean `scrollback`?"));

    let e = err("[general]\nmouse = 3\n");
    assert_eq!((e.line, e.col), (2, 9));
    assert!(e.message.contains("true or false"));

    let e = err("preset = \"tmx\"");
    assert_eq!(e.help.as_deref(), Some("did you mean \"tmux\"?"));

    let e = err("[keys.normal]\n\"ctrl+bogus\" = \"new-tab\"\n");
    assert_eq!((e.line, e.col), (2, 1));
    assert!(e.message.contains("unknown key \"bogus\""));

    let e =
        err("[keys.leader]\nr = { action = \"enter-mode\", mode = \"resize\", stiky = true }\n");
    assert_eq!((e.line, e.col), (2, 47));
    assert_eq!(e.help.as_deref(), Some("did you mean `sticky`?"));

    let e = err("[theme]\naccent = \"#zzzzzz\"\n");
    assert_eq!((e.line, e.col), (2, 10));

    let e = err("[leader]\nkey = \"ctrl+a b\"\n");
    assert!(e.message.contains("single key"));

    // TOML syntax errors keep their location too.
    let e = err("[general]\nmouse = \n");
    assert_eq!(e.line, 2);

    let e = err("[[commands]]\nname = \"x\"\n");
    assert!(e.message.contains("missing `action`"));
}

#[test]
fn duplicate_after_normalisation_warns() {
    let w =
        warns("[keys.normal]\n\"Shift+Alt+T\" = \"new-tab\"\n\"alt+shift+t\" = \"close-tab\"\n");
    assert_eq!(w.len(), 1, "{w:?}");
    assert!(w[0].contains("duplicate key"));
    let (c, ws) =
        parse("[keys.normal]\n\"Shift+Alt+T\" = \"new-tab\"\n\"alt+shift+t\" = \"close-tab\"\n");
    assert_eq!(ws[0].line, 3);
    // Later entry wins.
    let km = c.keymap(false);
    let b = km
        .bindings("normal")
        .iter()
        .find(|b| b.keys == parse_sequence("alt+shift+t").unwrap())
        .unwrap();
    assert_eq!(b.action, Action::CloseTab);
}

#[test]
fn prefix_conflict_warns() {
    let w = warns("[keys.normal]\n\"ctrl+a\" = \"new-tab\"\n\"ctrl+a x\" = \"quit\"\n");
    assert!(w.iter().any(|m| m.contains("also starts")), "{w:?}");
    // Against a preset binding too.
    let w = warns("[keys.normal]\n\"f10 x\" = \"quit\"\n");
    assert!(w.iter().any(|m| m.contains("also starts")), "{w:?}");
    // Leader sequences fold into the leader table and don't conflict.
    assert!(warns("[keys.normal]\n\"ctrl+space q\" = \"quit\"\n").is_empty());
}

#[test]
fn leader_in_normal_warns() {
    let w = warns("[keys.normal]\n\"ctrl+space\" = \"new-tab\"\n");
    assert!(w.iter().any(|m| m.contains("is the leader key")), "{w:?}");
    // Fine in other modes.
    assert!(warns("[keys.select]\n\"ctrl+space\" = \"exit-mode\"\n").is_empty());
}

#[test]
fn ctrl_shift_letter_warns() {
    let w = warns("[keys.normal]\n\"ctrl+shift+a\" = \"new-tab\"\n");
    assert_eq!(w.len(), 1, "{w:?}");
    assert!(w[0].contains("ctrl+a"), "{w:?}");
}

#[test]
fn windows_terminal_keys_warn_for_user_bindings_only() {
    for k in [
        "ctrl+tab",
        "alt+left",
        "alt+shift+down",
        "ctrl+alt+3",
        "ctrl+shift+v",
    ] {
        let w = warns(&format!("[keys.normal]\n\"{k}\" = \"new-tab\"\n"));
        assert!(
            w.iter().any(|m| m.contains("Windows Terminal")),
            "{k}: {w:?}"
        );
    }
    // Unbinding is not a use; presets never warn (covered by every_preset_is_clean).
    assert!(warns("[keys.normal]\n\"ctrl+tab\" = \"none\"\n").is_empty());
}

#[test]
fn mode_name_typo_warns_but_user_modes_are_fine() {
    let w = warns("[keys.resze]\nq = \"exit-mode\"\n");
    assert!(w[0].contains("unknown mode"), "{w:?}");
    assert!(warns("[keys.git]\ns = \"quit\"\n").is_empty());
}

#[test]
fn warning_display_has_location() {
    let (_, w) = parse("[keys.normal]\n\"ctrl+shift+a\" = \"new-tab\"\n");
    let s = w[0].to_string();
    assert!(s.starts_with("config.toml:2:1: warning: "), "{s}");
}
