//! `config.toml`: loading, validation with precise locations, and the effective keymap.
//!
//! The document is walked as span-carrying TOML values rather than deserialised
//! with serde, so every error and warning can point at a line and column and
//! suggest a fix. Effective config = preset + user file, rebuilt on every load,
//! so deleting a line reverts that setting.

mod error;
mod load;
mod parse;
mod settings;
#[cfg(test)]
mod tests;
mod theme;

use std::ops::Range;

use crate::action::Action;
use crate::keys::KeyChord;

pub use error::{ConfigError, Warning};
pub use settings::{CustomCommand, DirectKeys, General, LeaderConfig};
pub use theme::Theme;

/// A commented starter config (`rmx config init`). Parses with no warnings.
pub const DEFAULT_CONFIG_TOML: &str = include_str!("../default_config.toml");

/// One `[keys.<mode>]` entry as written; `action: None` unbinds.
#[derive(Debug, Clone, PartialEq)]
struct UserBinding {
    mode: String,
    keys: Vec<KeyChord>,
    action: Option<Action>,
    span: Range<usize>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Config {
    pub preset: String,
    pub direct_keys: DirectKeys,
    pub general: General,
    pub leader: LeaderConfig,
    pub theme: Theme,
    pub commands: Vec<CustomCommand>,
    user_keys: Vec<UserBinding>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            preset: "windows".into(),
            direct_keys: DirectKeys::Auto,
            general: General::default(),
            leader: LeaderConfig::for_preset("windows"),
            theme: Theme::default(),
            commands: Vec::new(),
            user_keys: Vec::new(),
        }
    }
}
