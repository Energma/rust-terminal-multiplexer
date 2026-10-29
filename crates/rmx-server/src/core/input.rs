//! Keyboard and paste input, and the overlay/toast plumbing it feeds.

use std::time::Instant;

use crossterm::event::KeyEvent;
use rmx_config::keymap::KeyOutcome;
use rmx_core::encode::{encode_key, encode_paste};
use rmx_core::layout::PaneId;

use super::client::{end_select, make_ctx};
use super::{ClientId, Core};
use crate::ui::{self, Ctx, Effect, Outcome, Overlay, PromptThen};

impl Core {
    pub(super) fn on_key(&mut self, id: ClientId, key: KeyEvent) {
        let Some(client) = self.clients.get_mut(&id) else {
            return;
        };
        client.dirty = true;
        if !client.overlays.is_empty() {
            let ctx = make_ctx(
                &self.config,
                &client.keymap,
                &self.workspaces,
                &self.panes,
                &self.attached,
                client.workspace,
                client.launch_cwd.as_deref(),
                &self.opts,
            );
            let outcome = client.overlays.last_mut().unwrap().handle_key(&key, &ctx);
            self.apply_outcome(id, outcome);
            return;
        }
        let outcome = client.engine.feed(&client.keymap, &key, Instant::now());
        client.hints = None;
        match outcome {
            KeyOutcome::Action(action) => self.dispatch(id, action),
            KeyOutcome::Pending { hints } => {
                if self.config.leader.show_hints {
                    client.hints = Some(hints);
                }
            }
            KeyOutcome::PassThrough => self.key_to_pane(id, &key),
            KeyOutcome::Ignored => {}
        }
        self.sync_select(id);
    }

    /// Ends select state when the key engine left select mode (Esc etc.).
    pub(super) fn sync_select(&mut self, id: ClientId) {
        let Some(c) = self.clients.get_mut(&id) else {
            return;
        };
        // Also when focus moved elsewhere (tab switch, click, close): select
        // state belongs to the pane it started in.
        let active = self
            .workspaces
            .iter()
            .find(|w| w.id == c.workspace)
            .map(|w| w.active().active_pane);
        let elsewhere = c.select.as_ref().is_some_and(|s| Some(s.pane) != active);
        if c.select.is_some() && (c.engine.mode() != "select" || elsewhere) {
            end_select(c, &mut self.panes);
        }
    }

    fn key_to_pane(&mut self, id: ClientId, key: &KeyEvent) {
        let Some(c) = self.clients.get(&id) else {
            return;
        };
        let Some(ws) = self.workspace(c.workspace) else {
            return;
        };
        let tab = ws.active();
        let targets = if tab.sync_input {
            tab.layout.panes()
        } else {
            vec![tab.active_pane]
        };
        for p in targets {
            if let Some(pane) = self.panes.get_mut(&p)
                && let Some(bytes) = encode_key(key, pane.emulator.application_cursor())
            {
                // Typing snaps a scrolled-back view to the live screen.
                pane.emulator.set_view_offset(0);
                pane.write(bytes);
            }
        }
    }

    pub(super) fn paste_text(&mut self, id: ClientId, text: &str) {
        if text.is_empty() {
            return;
        }
        if let Some(c) = self.clients.get_mut(&id)
            && let Some(top) = c.overlays.last_mut()
        {
            // Only into a text field; never replayed as keys, where a pasted
            // "y" would answer a confirmation.
            top.paste(text);
            c.dirty = true;
            return;
        }
        if let Some(c) = self.clients.get_mut(&id) {
            c.engine.exit_mode();
        }
        self.sync_select(id);
        if let Some(pane) = self.client_pane(id).and_then(|p| self.panes.get_mut(&p)) {
            pane.emulator.set_view_offset(0);
            let bytes = encode_paste(text, pane.emulator.bracketed_paste());
            pane.write(bytes);
        }
    }

    pub(super) fn client_pane(&self, id: ClientId) -> Option<PaneId> {
        self.clients
            .get(&id)
            .and_then(|c| self.active_pane_of(c.workspace))
    }

    pub(super) fn apply_outcome(&mut self, id: ClientId, outcome: Outcome) {
        if let Some(c) = self.clients.get_mut(&id) {
            c.dirty = true;
            if outcome.pop {
                c.overlays.pop();
            }
        }
        match outcome.effect {
            Some(Effect::Action(action)) => self.dispatch(id, action),
            Some(Effect::Cmd(cmd)) => self.run_cmd(id, cmd),
            None => {}
        }
    }

    pub(super) fn push_overlay(&mut self, id: ClientId, make: impl FnOnce(&Ctx) -> Overlay) {
        let Some(c) = self.clients.get_mut(&id) else {
            return;
        };
        let ctx = make_ctx(
            &self.config,
            &c.keymap,
            &self.workspaces,
            &self.panes,
            &self.attached,
            c.workspace,
            c.launch_cwd.as_deref(),
            &self.opts,
        );
        let overlay = make(&ctx);
        c.overlays.push(overlay);
        c.dirty = true;
    }

    pub(super) fn toast(&mut self, id: ClientId, text: impl Into<String>, error: bool) {
        if let Some(c) = self.clients.get_mut(&id) {
            c.toast = Some((text.into(), Instant::now(), error));
            c.dirty = true;
        }
    }

    pub(super) fn push_prompt(
        &mut self,
        id: ClientId,
        title: &str,
        initial: String,
        then: PromptThen,
    ) {
        let title = title.to_owned();
        self.push_overlay(id, move |_| {
            Overlay::Prompt(ui::dialog::Prompt::new(title, &initial, then))
        });
    }
}
