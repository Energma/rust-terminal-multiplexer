//! Hand-rolled TOML parsing of actions (not serde) so errors can name the
//! exact argument and suggest fixes.

mod args;
mod build;

use serde_spanned::Spanned;
use toml::de::{DeTable, DeValue};

use super::Action;
use crate::diag::Diag;
use args::Args;
use build::build;

impl Action {
    /// Parses an action from a CLI/API string: a bare id (`new-tab`) or a TOML
    /// inline table (`{ action = "split-pane", direction = "down" }`).
    pub fn parse(src: &str) -> Result<Action, String> {
        let src = src.trim();
        let fmt = |d: Diag| match d.help {
            Some(h) => format!("{} ({h})", d.message),
            None => d.message,
        };
        let value = if src.starts_with('{') {
            DeValue::parse(src).map_err(|e| e.message().to_string())?
        } else {
            Spanned::new(0..src.len(), DeValue::String(src.into()))
        };
        match from_value(&value).map_err(fmt)? {
            Some(a) => Ok(a),
            None => Err("\"none\" only makes sense in a key binding".into()),
        }
    }
}

type Value<'i> = Spanned<DeValue<'i>>;

/// Parses a binding/command action value. `Ok(None)` is the `"none"` unbind marker.
pub(crate) fn from_value(v: &Value<'_>) -> Result<Option<Action>, Diag> {
    match v.get_ref() {
        DeValue::String(id) => {
            if id == "none" {
                return Ok(None);
            }
            let mut args = Args::new(id, v.span(), None);
            build(id, v.span(), &mut args).map(Some)
        }
        DeValue::Table(t) => {
            let (id, id_span) = match find(t, "action") {
                Some(a) => match a.get_ref() {
                    DeValue::String(s) => (s.as_ref(), a.span()),
                    other => {
                        return Err(Diag::new(
                            a.span(),
                            format!("`action` must be a string, found {}", other.type_str()),
                        ));
                    }
                },
                None if find(t, "mode").is_some() => ("enter-mode", v.span()),
                None => {
                    return Err(Diag::new(v.span(), "missing `action` in action table")
                        .help("write { action = \"split-pane\", direction = \"right\" }"));
                }
            };
            if id == "none" {
                return Err(Diag::new(id_span, "\"none\" takes no arguments")
                    .help("write \"none\" as a plain string to unbind a key"));
            }
            let mut args = Args::new(id, v.span(), Some(t));
            let action = build(id, id_span, &mut args)?;
            args.finish()?;
            Ok(Some(action))
        }
        other => Err(Diag::new(
            v.span(),
            format!(
                "an action must be a string or an inline table, found {}",
                other.type_str()
            ),
        )
        .help("e.g. \"new-tab\" or { action = \"go-to-tab\", index = 1 }")),
    }
}

pub(super) fn find<'a, 'i>(t: &'a DeTable<'i>, name: &str) -> Option<&'a Value<'i>> {
    t.iter().find(|(k, _)| k.get_ref() == name).map(|(_, v)| v)
}
