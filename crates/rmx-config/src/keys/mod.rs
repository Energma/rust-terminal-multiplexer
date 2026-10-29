//! Key chords: parsing from config strings, normalising terminal events, display.
//!
//! Terminals disagree about how they report the same physical key (Shift+a as
//! `'A'`, with or without SHIFT; Ctrl+Space as NUL, `' '` or `'@'`), so both config
//! strings and live events go through one normaliser. Bindings then compare with
//! plain equality.

mod display;
mod normalize;
mod parse;
#[cfg(test)]
mod tests;

use crossterm::event::{KeyCode, KeyModifiers};
use serde::{Deserialize, Serialize};

pub use display::display_sequence;
pub use parse::parse_sequence;

/// One key plus modifiers, always in normalised form (see [`KeyChord::new`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct KeyChord {
    pub code: KeyCode,
    pub mods: KeyModifiers,
}
