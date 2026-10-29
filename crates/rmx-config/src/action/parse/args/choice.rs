//! Arguments restricted to a fixed set of values: enumerated choices and
//! split percentages.

use toml::de::DeValue;

use super::Args;
use crate::action::{Dir, RunIn};
use crate::diag::Diag;

impl Args<'_, '_> {
    fn choice<T: Copy>(
        &mut self,
        name: &'static str,
        options: &[(&'static str, T)],
    ) -> Result<Option<T>, Diag> {
        let Some(v) = self.get(name) else {
            return Ok(None);
        };
        let names = options.iter().map(|(n, _)| *n);
        let list = names.clone().collect::<Vec<_>>().join(", ");
        let DeValue::String(s) = v.get_ref() else {
            return Err(Self::type_err(name, &format!("one of {list}"), v));
        };
        match options.iter().find(|(n, _)| *n == s.as_ref()) {
            Some((_, t)) => Ok(Some(*t)),
            None => {
                let d = Diag::new(
                    v.span(),
                    format!("invalid `{name}` \"{s}\" (expected {list})"),
                );
                Err(d.suggest(s, names))
            }
        }
    }

    pub(in crate::action::parse) fn dir(
        &mut self,
        name: &'static str,
    ) -> Result<Option<Dir>, Diag> {
        self.choice(name, &Dir::ALL.map(|d| (d.as_str(), d)))
    }

    pub(in crate::action::parse) fn req_dir(&mut self, name: &'static str) -> Result<Dir, Diag> {
        self.dir(name)?
            .ok_or_else(|| self.missing(name, "\"left\""))
    }

    pub(in crate::action::parse) fn run_in(
        &mut self,
        name: &'static str,
    ) -> Result<Option<RunIn>, Diag> {
        let opts = [
            RunIn::NewTab,
            RunIn::SplitRight,
            RunIn::SplitDown,
            RunIn::Background,
        ];
        debug_assert!(opts.iter().map(|o| o.as_str()).eq(RunIn::NAMES));
        self.choice(name, &opts.map(|o| (o.as_str(), o)))
    }

    /// Split size: `30` or `"30%"`, 1..=99.
    pub(in crate::action::parse) fn size(
        &mut self,
        name: &'static str,
    ) -> Result<Option<u16>, Diag> {
        let Some(v) = self.get(name) else {
            return Ok(None);
        };
        let n = match v.get_ref() {
            DeValue::Integer(i) => i64::from_str_radix(i.as_str(), i.radix()).ok(),
            DeValue::String(s) => s
                .trim()
                .strip_suffix('%')
                .unwrap_or(s.trim())
                .trim()
                .parse::<i64>()
                .ok(),
            _ => return Err(Self::type_err(name, "a percentage like 30 or \"30%\"", v)),
        };
        match n {
            Some(n @ 1..=99) => Ok(Some(n as u16)),
            _ => Err(Diag::new(
                v.span(),
                format!("`{name}` must be a percentage from 1 to 99"),
            )),
        }
    }
}
