//! Parsing chords and chord sequences from config / CLI strings.

use crossterm::event::{KeyCode, KeyModifiers};

use super::KeyChord;

impl KeyChord {
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
