//! Effective key tables and the per-client key engine that walks them.
//!
//! The engine is a small state machine (mode, sticky flag, chord buffer) with
//! no I/O and an injected clock, so timeouts are testable without sleeping.

mod engine;
mod hints;
mod table;
#[cfg(test)]
mod tests;

use std::collections::BTreeMap;

use crate::action::Action;
use crate::keys::KeyChord;

pub use engine::{KeyEngine, KeyOutcome};
pub(crate) use table::fold_leader;

/// Built-in modes. Users may define extra ones under `[keys.<name>]`.
pub const MODES: &[&str] = &["normal", "leader", "select", "resize", "locked"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    Preset,
    User,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Binding {
    pub keys: Vec<KeyChord>,
    pub action: Action,
    pub source: Source,
}

#[derive(Debug, Clone)]
pub struct Keymap {
    tables: BTreeMap<String, Vec<Binding>>,
    leader: KeyChord,
    timeout_ms: u64,
}
