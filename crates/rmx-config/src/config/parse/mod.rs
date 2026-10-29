//! Walks the span-carrying TOML document into a [`Config`], collecting
//! warnings along the way.

mod keys;
mod sections;
mod values;
mod warnings;

use toml::de::{DeTable, DeValue};

use super::{Config, ConfigError, DirectKeys, Warning};
use crate::diag::Diag;
use crate::presets::{self, PRESETS};
use values::{as_str, as_table, entries, unknown_key};

const TOP_KEYS: &[&str] = &[
    "preset",
    "direct_keys",
    "general",
    "leader",
    "theme",
    "keys",
    "commands",
];

impl Config {
    pub fn parse(src: &str, origin: &str) -> Result<(Config, Vec<Warning>), ConfigError> {
        let root = DeTable::parse(src).map_err(|e| {
            let span = e.span().unwrap_or(0..0);
            ConfigError::from_diag(src, origin, Diag::new(span, e.message().trim()))
        })?;
        let mut p = Parser {
            src,
            config: Config::default(),
            warnings: Vec::new(),
        };
        p.document(root.get_ref())
            .map_err(|d| ConfigError::from_diag(src, origin, d))?;
        let warnings = p
            .warnings
            .into_iter()
            .map(|d| Warning::from_diag(src, origin, d))
            .collect();
        Ok((p.config, warnings))
    }
}

struct Parser<'s> {
    src: &'s str,
    config: Config,
    warnings: Vec<Diag>,
}

impl Parser<'_> {
    fn document(&mut self, root: &DeTable<'_>) -> Result<(), Diag> {
        let mut leader_key_set = false;
        let mut keys = None;
        for (k, v) in entries(root) {
            match k.get_ref().as_ref() {
                "preset" => {
                    let s = as_str(v, "preset")?;
                    if !PRESETS.contains(&s) {
                        return Err(Diag::new(v.span(), format!("unknown preset \"{s}\""))
                            .suggest(s, PRESETS.iter().copied())
                            .help_if_none(format!("presets: {}", PRESETS.join(", "))));
                    }
                    self.config.preset = s.into();
                }
                "direct_keys" => {
                    self.config.direct_keys = match v.get_ref() {
                        DeValue::Boolean(true) => DirectKeys::On,
                        DeValue::Boolean(false) => DirectKeys::Off,
                        _ => match as_str(v, "direct_keys")? {
                            "auto" => DirectKeys::Auto,
                            "on" => DirectKeys::On,
                            "off" => DirectKeys::Off,
                            s => {
                                return Err(Diag::new(
                                    v.span(),
                                    format!(
                                        "invalid direct_keys \"{s}\" (expected auto, on or off)"
                                    ),
                                )
                                .suggest(s, ["auto", "on", "off"]));
                            }
                        },
                    }
                }
                "general" => self.general(as_table(v, "[general]")?)?,
                "leader" => leader_key_set = self.leader(as_table(v, "[leader]")?)?,
                "theme" => self.theme(as_table(v, "[theme]")?)?,
                "keys" => keys = Some(as_table(v, "[keys]")?),
                "commands" => self.commands(v)?,
                _ => return Err(unknown_key(k, "", TOP_KEYS)),
            }
        }
        if !leader_key_set {
            self.config.leader.key = presets::preset_leader(&self.config.preset);
        }
        if let Some(keys) = keys {
            self.keys(keys)?;
        }
        Ok(())
    }
}
