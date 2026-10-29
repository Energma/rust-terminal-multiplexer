use super::*;
use crossterm::event::{KeyEvent, KeyEventKind, KeyEventState};

fn k(s: &str) -> KeyChord {
    KeyChord::parse(s).unwrap()
}

fn ev(code: KeyCode, mods: KeyModifiers) -> KeyChord {
    KeyChord::from_event(&KeyEvent {
        code,
        modifiers: mods,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    })
}

#[test]
fn parses_modifiers_in_any_order_and_case() {
    assert_eq!(k("Shift+Ctrl+T"), k("ctrl+shift+t"));
    assert_eq!(
        k("ctrl+shift+t"),
        KeyChord {
            code: KeyCode::Char('t'),
            mods: KeyModifiers::CONTROL | KeyModifiers::SHIFT
        }
    );
    assert_eq!(k("Alt+Left").code, KeyCode::Left);
    assert_eq!(k("Ctrl+T"), k("ctrl+t"));
    assert_eq!(k("f10").code, KeyCode::F(10));
}

#[test]
fn parses_named_and_symbol_keys() {
    assert_eq!(k("|").code, KeyCode::Char('|'));
    assert_eq!(k("space").code, KeyCode::Char(' '));
    assert_eq!(
        k("ctrl+space"),
        ev(KeyCode::Char(' '), KeyModifiers::CONTROL)
    );
    assert_eq!(k("esc").code, KeyCode::Esc);
    assert_eq!(k("enter").code, KeyCode::Enter);
    assert_eq!(k("pgup").code, KeyCode::PageUp);
    assert_eq!(k("plus"), k("+"));
    assert_eq!(k("ctrl++"), k("ctrl+plus"));
    assert_eq!(k("minus"), k("-"));
    assert_eq!(k("alt+shift+-"), k("alt+minus"));
}

#[test]
fn rejects_bad_keys() {
    assert!(KeyChord::parse("").is_err());
    assert!(KeyChord::parse("ctrl+").is_err());
    assert!(KeyChord::parse("hyperr+a").is_err());
    assert!(KeyChord::parse("f99").is_err());
    assert!(KeyChord::parse("nosuchkey").is_err());
    assert!(KeyChord::parse("ctrl++a").is_err());
}

#[test]
fn display_round_trips() {
    for s in [
        "ctrl+alt+shift+super+x",
        "ctrl+space",
        "plus",
        "alt+plus",
        "|",
        "f10",
        "pgdn",
        "shift+tab",
        "alt+left",
        "-",
        "ctrl+shift+t",
    ] {
        let c = k(s);
        assert_eq!(c.to_string(), s, "canonical form of {s}");
        assert_eq!(k(&c.to_string()), c);
    }
    assert_eq!(k("Shift+Ctrl+T").to_string(), "ctrl+shift+t");
}

#[test]
fn display_short_forms() {
    assert_eq!(k("ctrl+space").display_short(), "^Space");
    assert_eq!(k("alt+1").display_short(), "Alt+1");
    assert_eq!(k("f10").display_short(), "F10");
    assert_eq!(k("ctrl+shift+t").display_short(), "^Shift+T");
    assert_eq!(k("pgup").display_short(), "PgUp");
    assert_eq!(k("shift+left").display_short(), "Shift+Left");
    assert_eq!(k("|").display_short(), "|");
}

#[test]
fn shift_letter_events_normalise_the_same() {
    let want = k("shift+a");
    assert_eq!(ev(KeyCode::Char('A'), KeyModifiers::NONE), want);
    assert_eq!(ev(KeyCode::Char('A'), KeyModifiers::SHIFT), want);
    assert_eq!(ev(KeyCode::Char('a'), KeyModifiers::SHIFT), want);
    assert_ne!(ev(KeyCode::Char('a'), KeyModifiers::NONE), want);
    assert_eq!(k("shift+A"), want);
}

#[test]
fn shift_is_dropped_for_symbols() {
    assert_eq!(ev(KeyCode::Char('|'), KeyModifiers::SHIFT), k("|"));
    assert_eq!(ev(KeyCode::Char('?'), KeyModifiers::SHIFT), k("?"));
    assert_eq!(ev(KeyCode::Char('!'), KeyModifiers::SHIFT), k("shift+!"));
    assert_eq!(
        ev(KeyCode::Char('+'), KeyModifiers::ALT | KeyModifiers::SHIFT),
        k("alt+shift+plus")
    );
}

#[test]
fn special_events_normalise() {
    assert_eq!(ev(KeyCode::BackTab, KeyModifiers::SHIFT), k("shift+tab"));
    assert_eq!(ev(KeyCode::BackTab, KeyModifiers::NONE), k("shift+tab"));
    assert_eq!(
        ev(
            KeyCode::BackTab,
            KeyModifiers::CONTROL | KeyModifiers::SHIFT
        ),
        k("ctrl+shift+tab")
    );
    assert_eq!(ev(KeyCode::Null, KeyModifiers::NONE), k("ctrl+space"));
    assert_eq!(
        ev(KeyCode::Char('@'), KeyModifiers::CONTROL),
        k("ctrl+space")
    );
    assert_eq!(
        ev(
            KeyCode::Char('x'),
            KeyModifiers::CONTROL | KeyModifiers::HYPER
        ),
        k("ctrl+x")
    );
    // Shift on non-char keys is meaningful and kept.
    assert_ne!(ev(KeyCode::Left, KeyModifiers::SHIFT), k("left"));
}

#[test]
fn sequences() {
    assert_eq!(
        parse_sequence("ctrl+space  t").unwrap(),
        vec![k("ctrl+space"), k("t")]
    );
    assert!(parse_sequence("   ").is_err());
    assert!(parse_sequence("ctrl+space bogus").is_err());
    assert_eq!(
        display_sequence(&parse_sequence("ctrl+space |").unwrap()),
        "^Space |"
    );
}

#[test]
fn caps_lock_does_not_turn_ctrl_letter_into_ctrl_shift() {
    let caps = KeyChord::from_event(&KeyEvent::new(KeyCode::Char('C'), KeyModifiers::CONTROL));
    assert_eq!(caps, KeyChord::parse("ctrl+c").unwrap());
    let real = KeyChord::from_event(&KeyEvent::new(
        KeyCode::Char('C'),
        KeyModifiers::CONTROL | KeyModifiers::SHIFT,
    ));
    assert_eq!(real, KeyChord::parse("ctrl+shift+c").unwrap());
}
