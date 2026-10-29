//! Locating and loading the config file, and deriving runtime settings
//! (effective keymap, direct-key layer) from a loaded [`Config`].

use std::path::{Path, PathBuf};

use super::{Config, ConfigError, DirectKeys, Warning};
use crate::keymap::Keymap;
use crate::presets;

impl Config {
    /// `RMX_CONFIG` > Windows `%APPDATA%\rmx` > macOS `$XDG_CONFIG_HOME/rmx` or
    /// `~/.config/rmx` if it exists, else `~/Library/Application Support/rmx` >
    /// Linux `$XDG_CONFIG_HOME/rmx`. Always ends in `config.toml` (except the env override).
    pub fn default_path() -> PathBuf {
        if let Some(p) = std::env::var_os("RMX_CONFIG").filter(|p| !p.is_empty()) {
            return PathBuf::from(p);
        }
        if cfg!(target_os = "macos") {
            // Many CLI users keep dotfiles in ~/.config even on macOS; honour that first.
            let xdg = std::env::var_os("XDG_CONFIG_HOME")
                .filter(|p| !p.is_empty())
                .map(PathBuf::from)
                .or_else(|| dirs::home_dir().map(|h| h.join(".config")));
            if let Some(dir) = xdg.map(|x| x.join("rmx")).filter(|d| d.is_dir()) {
                return dir.join("config.toml");
            }
        }
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("rmx")
            .join("config.toml")
    }

    /// A missing file means defaults with no warnings.
    pub fn load(path: &Path) -> Result<(Config, Vec<Warning>), ConfigError> {
        let origin = path.display().to_string();
        match std::fs::read_to_string(path) {
            Ok(src) => Self::parse(&src, &origin),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok((Config::default(), vec![])),
            Err(e) => Err(ConfigError {
                origin,
                line: 0,
                col: 0,
                message: format!("cannot read config: {e}"),
                help: None,
                snippet: None,
            }),
        }
    }

    /// Effective keymap: preset bindings with user bindings merged over them (user
    /// wins; `"none"` unbinds). `direct` includes the Ctrl+Shift layer (PLAN §5.5).
    pub fn keymap(&self, direct: bool) -> Keymap {
        Keymap::build(
            self.leader.key,
            self.leader.timeout_ms,
            presets::bindings(&self.preset, self.leader.key, direct),
            self.user_keys
                .iter()
                .map(|b| (b.mode.clone(), b.keys.clone(), b.action.clone())),
        )
    }

    pub fn direct_enabled(&self, terminal_supports_enhanced_keys: bool) -> bool {
        match self.direct_keys {
            DirectKeys::Auto => cfg!(windows) || terminal_supports_enhanced_keys,
            DirectKeys::On => true,
            DirectKeys::Off => false,
        }
    }
}
