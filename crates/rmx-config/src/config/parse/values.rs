//! Typed accessors over span-carrying TOML values, each producing a located
//! [`Diag`] on mismatch.

use serde_spanned::Spanned;
use toml::de::{DeTable, DeValue};

use crate::diag::{Diag, did_you_mean};

pub(super) type Value<'i> = Spanned<DeValue<'i>>;
pub(super) type Entry<'a, 'i> = (&'a Spanned<std::borrow::Cow<'i, str>>, &'a Value<'i>);

/// Table entries in source order (DeTable itself is sorted by key), so later
/// duplicates win and diagnostics come out top to bottom.
pub(super) fn entries<'a, 'i>(t: &'a DeTable<'i>) -> Vec<Entry<'a, 'i>> {
    let mut v: Vec<_> = t.iter().collect();
    v.sort_by_key(|(k, _)| k.span().start);
    v
}

pub(super) fn as_table<'a, 'i>(v: &'a Value<'i>, what: &str) -> Result<&'a DeTable<'i>, Diag> {
    v.get_ref().as_table().ok_or_else(|| {
        Diag::new(
            v.span(),
            format!("{what} must be a table, found {}", v.get_ref().type_str()),
        )
    })
}

pub(super) fn as_str<'a>(v: &'a Value<'_>, name: &str) -> Result<&'a str, Diag> {
    v.get_ref().as_str().ok_or_else(|| {
        Diag::new(
            v.span(),
            format!(
                "`{name}` must be a string, found {}",
                v.get_ref().type_str()
            ),
        )
    })
}

pub(super) fn as_bool(v: &Value<'_>, name: &str) -> Result<bool, Diag> {
    v.get_ref().as_bool().ok_or_else(|| {
        Diag::new(
            v.span(),
            format!(
                "`{name}` must be true or false, found {}",
                v.get_ref().type_str()
            ),
        )
    })
}

pub(super) fn as_int(v: &Value<'_>, name: &str, min: i64, max: i64) -> Result<i64, Diag> {
    let Some(i) = v.get_ref().as_integer() else {
        return Err(Diag::new(
            v.span(),
            format!(
                "`{name}` must be an integer, found {}",
                v.get_ref().type_str()
            ),
        ));
    };
    match i64::from_str_radix(i.as_str(), i.radix()) {
        Ok(n) if (min..=max).contains(&n) => Ok(n),
        _ => Err(Diag::new(
            v.span(),
            format!("`{name}` must be between {min} and {max}"),
        )),
    }
}

pub(super) fn unknown_key(
    k: &Spanned<std::borrow::Cow<'_, str>>,
    section: &str,
    known: &[&str],
) -> Diag {
    let name: &str = k.get_ref();
    let msg = if section.is_empty() {
        format!("unknown setting `{name}`")
    } else {
        format!("unknown setting `{name}` in [{section}]")
    };
    let d = Diag::new(k.span(), msg);
    match did_you_mean(name, known.iter().copied()) {
        Some(s) => d.help(format!("did you mean `{s}`?")),
        None => d.help(format!("valid settings: {}", known.join(", "))),
    }
}

impl Diag {
    pub(super) fn help_if_none(self, help: String) -> Self {
        if self.help.is_some() {
            self
        } else {
            self.help(help)
        }
    }
}
