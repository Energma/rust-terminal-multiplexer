//! Key chords: parsing from config strings, normalising terminal events, display.
//!
//! Terminals disagree about how they report the same physical key (Shift+a as
//! `'A'`, with or without SHIFT; Ctrl+Space as NUL, `' '` or `'@'`), so both config
//! strings and live events go through one normaliser. Bindings then compare with
//! plain equality.

use std::fmt;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use serde::{Deserialize, Serialize};

/// One key plus modifiers, always in normalised form (see [`KeyChord::new`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct KeyChord {
    pub code: KeyCode,
    pub mods: KeyModifiers,
}

const KEPT_MODS: KeyModifiers = KeyModifiers::SHIFT
    .union(KeyModifiers::CONTROL)
    .union(KeyModifiers::ALT)
    .union(KeyModifiers::SUPER);

impl KeyChord {
    /// Builds a normalised chord:
    /// - letters are lowercase; an uppercase letter implies SHIFT;
    /// - SHIFT is dropped for other printable chars, because which symbols need
    ///   Shift depends on the keyboard layout (`|` must match `Char('|')+SHIFT`);
    /// - BackTab becomes Shift+Tab; NUL and Ctrl+`@` become Ctrl+Space;
    /// - only Shift/Ctrl/Alt/Super survive (Hyper/Meta are too rare to bind).
    pub fn new(code: KeyCode, mods: KeyModifiers) -> Self {
        let mut mods = mods & KEPT_MODS;
        let code = match code {
            KeyCode::BackTab => {
                mods |= KeyModifiers::SHIFT;
                KeyCode::Tab
            }
            KeyCode::Null => {
                mods |= KeyModifiers::CONTROL;
                KeyCode::Char(' ')
            }
            KeyCode::Char('@') if mods.contains(KeyModifiers::CONTROL) => KeyCode::Char(' '),
            KeyCode::Char(c) if c.is_uppercase() => {
                // With Ctrl held, trust the reported Shift flag rather than the
                // letter's case: Windows reports Caps Lock + Ctrl+C as `C`, and
                // legacy terminals can't send Ctrl+Shift+letter at all.
                if !mods.contains(KeyModifiers::CONTROL) {
                    mods |= KeyModifiers::SHIFT;
                }
                KeyCode::Char(c.to_lowercase().next().unwrap_or(c))
            }
            KeyCode::Char(c) => {
                if !c.is_lowercase() {
                    mods -= KeyModifiers::SHIFT;
                }
                KeyCode::Char(c)
            }
            other => other,
        };
        Self { code, mods }
    }

    pub fn from_event(ev: &KeyEvent) -> Self {
        Self::new(ev.code, ev.modifiers)
    }

    /// Parses one chord like `ctrl+shift+t`, `Alt+Left`, `f10`, `|` or `ctrl+space`.
    /// Case-insensitive: `Ctrl+T` is Ctrl+t; Shift must be written out (`shift+t`).
    pub fn parse(s: &str) -> Result<Self, String> {
        let s = s.trim();
        if s.is_empty() {
            return Err("empty key".into());
        }
        // A trailing "+" is the plus key itself ("+", "ctrl++").
        let (mods_part, key_part) = if s == "+" {
            ("", "+")
        } else if let Some(rest) = s.strip_suffix("++") {
            (rest, "+")
        } else {
            match s.rsplit_once('+') {
                Some((m, k)) => (m, k),
                None => ("", s),
            }
        };
        if key_part.is_empty() {
            return Err(format!("missing key after modifiers in \"{s}\""));
        }
        let mut mods = KeyModifiers::NONE;
        if !mods_part.is_empty() {
            for m in mods_part.split('+') {
                mods |= match m.to_ascii_lowercase().as_str() {
                    "ctrl" | "control" => KeyModifiers::CONTROL,
                    "alt" | "option" | "meta" => KeyModifiers::ALT,
                    "shift" => KeyModifiers::SHIFT,
                    "super" | "cmd" | "win" => KeyModifiers::SUPER,
                    "" => return Err(format!("empty modifier in \"{s}\"")),
                    other => {
                        return Err(format!(
                            "unknown modifier \"{other}\" in \"{s}\" (use ctrl, alt, shift or super)"
                        ));
                    }
                };
            }
        }
        let code = parse_code(key_part).ok_or_else(|| format!("unknown key \"{key_part}\""))?;
        Ok(Self::new(code, mods))
    }

    /// Compact form for status bars and menus: `^Space`, `Alt+1`, `F10`, `^Shift+T`.
    pub fn display_short(&self) -> String {
        let mut out = String::new();
        if self.mods.contains(KeyModifiers::CONTROL) {
            out.push('^');
        }
        for (m, name) in [
            (KeyModifiers::ALT, "Alt+"),
            (KeyModifiers::SHIFT, "Shift+"),
            (KeyModifiers::SUPER, "Super+"),
        ] {
            if self.mods.contains(m) {
                out.push_str(name);
            }
        }
        match self.code {
            KeyCode::Char(' ') => out.push_str("Space"),
            KeyCode::Char(c) => out.extend(c.to_uppercase()),
            KeyCode::F(n) => out.push_str(&format!("F{n}")),
            KeyCode::PageUp => out.push_str("PgUp"),
            KeyCode::PageDown => out.push_str("PgDn"),
            code => {
                let name = code_name(code);
                let mut chars = name.chars();
                if let Some(first) = chars.next() {
                    out.extend(first.to_uppercase());
                    out.push_str(chars.as_str());
                }
            }
        }
        out
    }

    pub(crate) fn is_ctrl_shift_letter(&self) -> bool {
        matches!(self.code, KeyCode::Char(c) if c.is_alphabetic())
            && self
                .mods
                .contains(KeyModifiers::CONTROL | KeyModifiers::SHIFT)
    }
}

/// Canonical lowercase form (`ctrl+alt+shift+super+key`); parses back to the same chord.
impl fmt::Display for KeyChord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (m, name) in [
            (KeyModifiers::CONTROL, "ctrl+"),
            (KeyModifiers::ALT, "alt+"),
            (KeyModifiers::SHIFT, "shift+"),
            (KeyModifiers::SUPER, "super+"),
        ] {
            if self.mods.contains(m) {
                f.write_str(name)?;
            }
        }
        match self.code {
            KeyCode::Char(' ') => f.write_str("space"),
            KeyCode::Char('+') => f.write_str("plus"),
            KeyCode::Char(c) => write!(f, "{c}"),
            KeyCode::F(n) => write!(f, "f{n}"),
            code => f.write_str(code_name(code)),
        }
    }
}

/// Parses a space-separated chord sequence such as `ctrl+space t`.
pub fn parse_sequence(s: &str) -> Result<Vec<KeyChord>, String> {
    let keys = s
        .split_whitespace()
        .map(KeyChord::parse)
        .collect::<Result<Vec<_>, _>>()?;
    if keys.is_empty() {
        return Err("empty key".into());
    }
    Ok(keys)
}

/// Formats a sequence for UI hints: `^Space |`.
pub fn display_sequence(keys: &[KeyChord]) -> String {
    keys.iter()
        .map(KeyChord::display_short)
        .collect::<Vec<_>>()
        .join(" ")
}

fn code_name(code: KeyCode) -> &'static str {
    match code {
        KeyCode::Backspace => "backspace",
        KeyCode::Enter => "enter",
        KeyCode::Left => "left",
        KeyCode::Right => "right",
        KeyCode::Up => "up",
        KeyCode::Down => "down",
        KeyCode::Home => "home",
        KeyCode::End => "end",
        KeyCode::PageUp => "pgup",
        KeyCode::PageDown => "pgdn",
        KeyCode::Tab | KeyCode::BackTab => "tab",
        KeyCode::Delete => "delete",
        KeyCode::Insert => "insert",
        KeyCode::Esc => "esc",
        KeyCode::CapsLock => "capslock",
        KeyCode::ScrollLock => "scrolllock",
        KeyCode::NumLock => "numlock",
        KeyCode::PrintScreen => "printscreen",
        KeyCode::Pause => "pause",
        KeyCode::Menu => "menu",
        _ => "unknown",
    }
}

fn parse_code(s: &str) -> Option<KeyCode> {
    let mut chars = s.chars();
    if let (Some(c), None) = (chars.next(), chars.next()) {
        return Some(KeyCode::Char(c.to_lowercase().next().unwrap_or(c)));
    }
    let lower = s.to_ascii_lowercase();
    if let Some(n) = lower.strip_prefix('f').and_then(|n| n.parse::<u8>().ok()) {
        return (1..=24).contains(&n).then_some(KeyCode::F(n));
    }
    Some(match lower.as_str() {
        "space" | "spc" => KeyCode::Char(' '),
        "enter" | "return" | "ret" => KeyCode::Enter,
        "esc" | "escape" => KeyCode::Esc,
        "tab" => KeyCode::Tab,
        "backtab" => KeyCode::BackTab,
        "backspace" | "bs" => KeyCode::Backspace,
        "delete" | "del" => KeyCode::Delete,
        "insert" | "ins" => KeyCode::Insert,
        "home" => KeyCode::Home,
        "end" => KeyCode::End,
        "pgup" | "pageup" => KeyCode::PageUp,
        "pgdn" | "pgdown" | "pagedown" => KeyCode::PageDown,
        "up" => KeyCode::Up,
        "down" => KeyCode::Down,
        "left" => KeyCode::Left,
        "right" => KeyCode::Right,
        "menu" => KeyCode::Menu,
        "plus" => KeyCode::Char('+'),
        "minus" => KeyCode::Char('-'),
        "comma" => KeyCode::Char(','),
        "period" | "dot" => KeyCode::Char('.'),
        "slash" => KeyCode::Char('/'),
        "backslash" => KeyCode::Char('\\'),
        "pipe" => KeyCode::Char('|'),
        "semicolon" => KeyCode::Char(';'),
        "colon" => KeyCode::Char(':'),
        "quote" => KeyCode::Char('\''),
        "dquote" => KeyCode::Char('"'),
        "backquote" | "backtick" => KeyCode::Char('`'),
        "lbracket" => KeyCode::Char('['),
        "rbracket" => KeyCode::Char(']'),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyEventKind, KeyEventState};

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
}
