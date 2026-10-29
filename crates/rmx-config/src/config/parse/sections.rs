//! The simple sections: `[general]`, `[leader]`, `[theme]` and `[[commands]]`.

use ratatui::style::Color;
use toml::de::DeTable;

use super::Parser;
use super::values::{Value, as_bool, as_int, as_str, as_table, entries, unknown_key};
use crate::action;
use crate::config::CustomCommand;
use crate::config::theme::THEME_KEYS;
use crate::diag::Diag;
use crate::keys::parse_sequence;

const GENERAL_KEYS: &[&str] = &[
    "shell",
    "scrollback",
    "mouse",
    "copy_on_select",
    "restore_on_start",
];
const LEADER_KEYS: &[&str] = &["key", "timeout_ms", "show_hints"];

impl Parser<'_> {
    pub(super) fn general(&mut self, t: &DeTable<'_>) -> Result<(), Diag> {
        let g = &mut self.config.general;
        for (k, v) in entries(t) {
            match k.get_ref().as_ref() {
                "shell" => {
                    let s = as_str(v, "shell")?.trim();
                    g.shell = (!s.is_empty() && s != "auto").then(|| s.to_string());
                }
                "scrollback" => g.scrollback = as_int(v, "scrollback", 0, 10_000_000)? as usize,
                "mouse" => g.mouse = as_bool(v, "mouse")?,
                "copy_on_select" => g.copy_on_select = as_bool(v, "copy_on_select")?,
                "restore_on_start" => g.restore_on_start = as_bool(v, "restore_on_start")?,
                _ => return Err(unknown_key(k, "general", GENERAL_KEYS)),
            }
        }
        Ok(())
    }

    /// Returns whether `key` was given (otherwise the preset's leader applies).
    pub(super) fn leader(&mut self, t: &DeTable<'_>) -> Result<bool, Diag> {
        let mut key_set = false;
        let l = &mut self.config.leader;
        for (k, v) in entries(t) {
            match k.get_ref().as_ref() {
                "key" => {
                    let s = as_str(v, "key")?;
                    let keys = parse_sequence(s).map_err(|e| Diag::new(v.span(), e))?;
                    let [key] = keys[..] else {
                        return Err(Diag::new(v.span(), "the leader must be a single key")
                            .help("e.g. key = \"ctrl+space\""));
                    };
                    l.key = key;
                    key_set = true;
                }
                "timeout_ms" => l.timeout_ms = as_int(v, "timeout_ms", 0, 60_000)? as u64,
                "show_hints" => l.show_hints = as_bool(v, "show_hints")?,
                _ => return Err(unknown_key(k, "leader", LEADER_KEYS)),
            }
        }
        Ok(key_set)
    }

    pub(super) fn theme(&mut self, t: &DeTable<'_>) -> Result<(), Diag> {
        for (k, v) in entries(t) {
            let name: &str = k.get_ref();
            let Some(slot) = self.config.theme.slot(name) else {
                return Err(unknown_key(k, "theme", THEME_KEYS));
            };
            let s = as_str(v, name)?;
            *slot = s.parse::<Color>().map_err(|_| {
                Diag::new(v.span(), format!("invalid color \"{s}\""))
                    .help("use \"#rrggbb\", an ANSI index like \"238\", or a name like \"blue\"")
            })?;
        }
        Ok(())
    }

    pub(super) fn commands(&mut self, v: &Value<'_>) -> Result<(), Diag> {
        let Some(items) = v.get_ref().as_array() else {
            return Err(Diag::new(v.span(), "`commands` must be an array of tables")
                .help("write [[commands]] sections"));
        };
        for item in items.iter() {
            let t = as_table(item, "each command")?;
            let (mut name, mut act) = (None, None);
            for (k, v) in entries(t) {
                match k.get_ref().as_ref() {
                    "name" => name = Some(as_str(v, "name")?.to_string()),
                    "action" => {
                        act = Some(action::from_value(v)?.ok_or_else(|| {
                            Diag::new(v.span(), "a command can't have action \"none\"")
                        })?)
                    }
                    _ => return Err(unknown_key(k, "[commands]", &["name", "action"])),
                }
            }
            let missing = |what| Diag::new(item.span(), format!("command is missing `{what}`"));
            self.config.commands.push(CustomCommand {
                name: name
                    .filter(|n| !n.trim().is_empty())
                    .ok_or_else(|| missing("name"))?,
                action: act.ok_or_else(|| missing("action"))?,
            });
        }
        Ok(())
    }
}
