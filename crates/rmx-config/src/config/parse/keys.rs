//! The `[keys.<mode>]` tables: user bindings, unknown-mode and duplicate-key
//! warnings.

use std::collections::HashMap;
use std::ops::Range;

use toml::de::DeTable;

use super::Parser;
use super::values::{as_table, entries};
use crate::action;
use crate::config::UserBinding;
use crate::diag::{Diag, did_you_mean, line_col};
use crate::keymap::{MODES, fold_leader};
use crate::keys::{KeyChord, parse_sequence};

impl Parser<'_> {
    pub(super) fn keys(&mut self, t: &DeTable<'_>) -> Result<(), Diag> {
        let leader = self.config.leader.key;
        // (mode, keys) after normalisation and leader folding -> first occurrence.
        let mut seen: HashMap<(String, Vec<KeyChord>), Range<usize>> = HashMap::new();
        for (mode_key, mode_val) in entries(t) {
            let mode: &str = mode_key.get_ref();
            let table = as_table(mode_val, &format!("[keys.{mode}]"))?;
            if !MODES.contains(&mode)
                && let Some(s) = did_you_mean(mode, MODES.iter().copied())
            {
                self.warnings.push(
                    Diag::new(
                        mode_key.span(),
                        format!("unknown mode \"{mode}\" (treated as a new user mode)"),
                    )
                    .help(format!("did you mean \"{s}\"?")),
                );
            }
            for (k, v) in entries(table) {
                let keys = parse_sequence(k.get_ref()).map_err(|e| Diag::new(k.span(), e))?;
                let action = action::from_value(v)?;
                let folded = fold_leader(mode.to_string(), keys.clone(), leader);
                let shown = k.get_ref().to_string();
                if let Some(first) = seen.get(&folded) {
                    let what = if folded.0 == mode {
                        format!("[keys.{mode}]")
                    } else {
                        "the leader table".to_string()
                    };
                    self.warnings.push(
                        Diag::new(
                            k.span(),
                            format!("duplicate key \"{shown}\" in {what}; this one wins"),
                        )
                        .help(format!(
                            "it normalises to the same key as line {}",
                            line_col(self.src, first.start).0
                        )),
                    );
                } else {
                    seen.insert(folded, k.span());
                }
                if action.is_some() {
                    self.key_warnings(mode, &keys, &shown, k.span());
                }
                self.config.user_keys.push(UserBinding {
                    mode: mode.to_string(),
                    keys,
                    action,
                    span: k.span(),
                });
            }
        }
        self.prefix_warnings();
        Ok(())
    }
}
