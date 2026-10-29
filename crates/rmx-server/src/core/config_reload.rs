//! Reloading the config file and telling clients how it went.

use rmx_config::Config;
use rmx_config::keymap::KeyEngine;

use super::client::end_select;
use super::{ClientId, Core};

impl Core {
    pub(super) fn reload_config(&mut self, requested_by: Option<ClientId>) {
        match Config::load(&self.opts.config_path) {
            Ok((config, warnings)) => {
                self.config = config;
                for c in self.clients.values_mut() {
                    let direct = self.config.direct_enabled(c.enhanced_keys);
                    c.keymap = self.config.keymap(direct);
                    end_select(c, &mut self.panes);
                    c.engine = KeyEngine::new();
                    c.dirty = true;
                }
                let msg = match warnings.first() {
                    Some(w) => (
                        format!("Settings reloaded · {} warning(s): {w}", warnings.len()),
                        true,
                    ),
                    None => ("Settings reloaded".to_owned(), false),
                };
                let ids: Vec<ClientId> = self.clients.keys().copied().collect();
                for id in ids {
                    self.toast(id, msg.0.clone(), msg.1);
                }
                tracing::info!("config reloaded");
            }
            Err(err) => {
                // Keep running with the old config; show where the problem
                // is, with just the file name so the message fits.
                let full = self.opts.config_path.display().to_string();
                let name = self
                    .opts
                    .config_path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                let first = err
                    .to_string()
                    .lines()
                    .next()
                    .unwrap_or_default()
                    .replace(&full, &name);
                tracing::warn!("config reload failed: {err}");
                let ids: Vec<ClientId> = match requested_by {
                    Some(id) => vec![id],
                    None => self.clients.keys().copied().collect(),
                };
                for id in ids {
                    self.toast(id, format!("Settings not applied: {first}"), true);
                }
            }
        }
    }
}
