//! Typed, validated access to an action table's arguments.

mod choice;

use std::ops::Range;

use toml::de::{DeTable, DeValue};

use super::{Value, find};
use crate::diag::Diag;

/// Argument reader that records which names the action asked for, so leftovers
/// can be reported as unknown with a suggestion from the real argument list.
pub(super) struct Args<'a, 'i> {
    id: String,
    span: Range<usize>,
    table: Option<&'a DeTable<'i>>,
    known: Vec<&'static str>,
}

impl<'a, 'i> Args<'a, 'i> {
    pub(super) fn new(id: &str, span: Range<usize>, table: Option<&'a DeTable<'i>>) -> Self {
        Self {
            id: id.to_string(),
            span,
            table,
            known: Vec::new(),
        }
    }

    fn get(&mut self, name: &'static str) -> Option<&'a Value<'i>> {
        self.known.push(name);
        self.table.and_then(|t| find(t, name))
    }

    fn missing(&self, name: &str, example: &str) -> Diag {
        Diag::new(
            self.span.clone(),
            format!("action \"{}\" needs argument `{name}`", self.id),
        )
        .help(format!(
            "write {{ action = \"{}\", {name} = {example} }}",
            self.id
        ))
    }

    fn type_err(name: &str, want: &str, v: &Value<'_>) -> Diag {
        Diag::new(
            v.span(),
            format!("`{name}` must be {want}, found {}", v.get_ref().type_str()),
        )
    }

    pub(super) fn string(
        &mut self,
        name: &'static str,
    ) -> Result<Option<(String, Range<usize>)>, Diag> {
        match self.get(name) {
            None => Ok(None),
            Some(v) => match v.get_ref() {
                DeValue::String(s) if s.trim().is_empty() => {
                    Err(Diag::new(v.span(), format!("`{name}` must not be empty")))
                }
                DeValue::String(s) => Ok(Some((s.to_string(), v.span()))),
                _ => Err(Self::type_err(name, "a string", v)),
            },
        }
    }

    pub(super) fn req_string(&mut self, name: &'static str, example: &str) -> Result<String, Diag> {
        self.string(name)?
            .map(|(s, _)| s)
            .ok_or_else(|| self.missing(name, example))
    }

    pub(super) fn int(
        &mut self,
        name: &'static str,
        min: i64,
        max: i64,
    ) -> Result<Option<i64>, Diag> {
        let Some(v) = self.get(name) else {
            return Ok(None);
        };
        let DeValue::Integer(i) = v.get_ref() else {
            return Err(Self::type_err(name, "an integer", v));
        };
        match i64::from_str_radix(i.as_str(), i.radix()) {
            Ok(n) if (min..=max).contains(&n) => Ok(Some(n)),
            _ => Err(Diag::new(
                v.span(),
                format!("`{name}` must be between {min} and {max}"),
            )),
        }
    }

    pub(super) fn req_int(
        &mut self,
        name: &'static str,
        min: i64,
        max: i64,
        ex: &str,
    ) -> Result<i64, Diag> {
        self.int(name, min, max)?
            .ok_or_else(|| self.missing(name, ex))
    }

    pub(super) fn bool(&mut self, name: &'static str) -> Result<Option<bool>, Diag> {
        match self.get(name) {
            None => Ok(None),
            Some(v) => match v.get_ref() {
                DeValue::Boolean(b) => Ok(Some(*b)),
                _ => Err(Self::type_err(name, "true or false", v)),
            },
        }
    }

    pub(super) fn finish(&self) -> Result<(), Diag> {
        let Some(t) = self.table else {
            return Ok(());
        };
        for (k, _) in t.iter() {
            let k_str: &str = k.get_ref();
            if k_str == "action" || self.known.contains(&k_str) {
                continue;
            }
            let d = Diag::new(
                k.span(),
                format!("unknown argument `{k_str}` for action \"{}\"", self.id),
            );
            return Err(if self.known.is_empty() {
                d.help(format!("\"{}\" takes no arguments", self.id))
            } else {
                match crate::diag::did_you_mean(k_str, self.known.iter().copied()) {
                    Some(s) => d.help(format!("did you mean `{s}`?")),
                    None => d.help(format!("valid arguments: {}", self.known.join(", "))),
                }
            });
        }
        Ok(())
    }
}
