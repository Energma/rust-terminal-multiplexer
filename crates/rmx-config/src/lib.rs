//! Configuration and key handling for rust-multiplexer.
//!
//! Pure logic (no terminal or socket I/O beyond reading the config file): the
//! server owns one [`Config`]/[`Keymap`] and one [`KeyEngine`] per client.

pub mod action;
pub mod config;
mod diag;
pub mod keymap;
pub mod keys;
pub mod presets;

pub use action::{
    ACTIONS, Action, ActionInfo, Category, Dir, LAYOUTS, RunIn, action_info, all_action_ids,
    palette_actions,
};
pub use config::{
    Config, ConfigError, CustomCommand, DEFAULT_CONFIG_TOML, DirectKeys, General, LeaderConfig,
    Theme, Warning,
};
pub use keymap::{Binding, KeyEngine, KeyOutcome, Keymap, MODES, Source};
pub use keys::{KeyChord, display_sequence, parse_sequence};
pub use presets::{PRESETS, preset_leader};
