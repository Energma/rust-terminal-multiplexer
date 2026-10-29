//! Rendering chords: canonical config form (`Display`) and compact UI hints.

use std::fmt;

use crossterm::event::{KeyCode, KeyModifiers};

use super::KeyChord;

impl KeyChord {
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
