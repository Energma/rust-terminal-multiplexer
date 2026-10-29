//! The single normaliser every chord goes through, whether it came from a
//! config string or a live terminal event.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::KeyChord;

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

    pub(crate) fn is_ctrl_shift_letter(&self) -> bool {
        matches!(self.code, KeyCode::Char(c) if c.is_alphabetic())
            && self
                .mods
                .contains(KeyModifiers::CONTROL | KeyModifiers::SHIFT)
    }
}
