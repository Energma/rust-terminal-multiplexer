use std::path::Path;

use super::*;
use crate::keys::KeyChord;

mod diagnostics;
mod presets;

fn parse(src: &str) -> (Config, Vec<Warning>) {
    Config::parse(src, "config.toml").unwrap_or_else(|e| panic!("{e}"))
}

fn err(src: &str) -> ConfigError {
    Config::parse(src, "config.toml").unwrap_err()
}

fn warns(src: &str) -> Vec<String> {
    parse(src).1.into_iter().map(|w| w.message).collect()
}

#[test]
fn empty_is_default() {
    let (c, w) = parse("");
    assert_eq!(c, Config::default());
    assert!(w.is_empty());
    assert_eq!(c.leader.key, KeyChord::parse("ctrl+space").unwrap());
}

#[test]
fn default_config_parses_cleanly() {
    let (c, w) = parse(DEFAULT_CONFIG_TOML);
    assert!(w.is_empty(), "{w:?}");
    assert_eq!(c.preset, "windows");
}

#[test]
fn direct_keys_setting() {
    let (c, _) = parse("direct_keys = \"off\"");
    assert!(!c.direct_enabled(true));
    let (c, _) = parse("direct_keys = \"on\"");
    assert!(c.direct_enabled(false));
    let (c, _) = parse("");
    assert!(c.direct_enabled(true));
    assert_eq!(c.direct_enabled(false), cfg!(windows));
    assert!(err("direct_keys = \"atuo\"").help.unwrap().contains("auto"));
}

#[test]
fn load_missing_file_is_default() {
    let (c, w) = Config::load(Path::new("/definitely/not/here/config.toml")).unwrap();
    assert_eq!(c, Config::default());
    assert!(w.is_empty());
}

#[test]
fn default_path_ends_in_config_toml() {
    // RMX_CONFIG may be set in the environment; only check the shape otherwise.
    if std::env::var_os("RMX_CONFIG").is_none() {
        let p = Config::default_path();
        assert!(p.ends_with("rmx/config.toml"), "{}", p.display());
    }
}
