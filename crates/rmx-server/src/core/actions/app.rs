//! App-level actions: menus and help, quit, config, key modes, locking,
//! running commands and sending keys.

use std::path::Path;

use anyhow::Result;
use rmx_config::action::{Action, Dir, RunIn};
use rmx_core::layout::PaneId;

use crate::core::helpers::{editor_command, run_background, unescape};
use crate::core::{ClientId, Core};
use crate::state::WorkspaceId;
use crate::ui::{self, Cmd, Overlay};

impl Core {
    pub(super) fn app_action(
        &mut self,
        id: ClientId,
        ws: WorkspaceId,
        tab_idx: usize,
        pane: PaneId,
        action: Action,
    ) -> Result<()> {
        match action {
            Action::CommandPalette => {
                self.push_overlay(id, |_| Overlay::Palette(Default::default()))
            }
            Action::OpenMenu => self.push_overlay(id, |_| Overlay::Menu(ui::menu::MenuBar::new(0))),
            Action::ShowHelp => self.push_overlay(id, |_| Overlay::Help(Default::default())),
            Action::Quit => self.run_cmd(
                id,
                Cmd::Confirm {
                    message: "Quit rmx and close every workspace?".into(),
                    then: Box::new(Cmd::QuitServer),
                },
            ),
            Action::ReloadConfig => self.reload_config(Some(id)),
            Action::OpenConfig => {
                let path = self.opts.config_path.clone();
                if !path.exists() {
                    if let Some(dir) = path.parent() {
                        std::fs::create_dir_all(dir)?;
                    }
                    std::fs::write(&path, rmx_config::DEFAULT_CONFIG_TOML)?;
                }
                self.new_tab(ws, Some(editor_command(&path)))?;
            }
            // `new_tab` starts in the focused pane's cwd, so "." is that folder.
            Action::OpenEditor => self.new_tab(ws, Some(editor_command(Path::new("."))))?,
            Action::EnterMode { mode, sticky } => {
                if let Some(c) = self.clients.get_mut(&id) {
                    c.engine.set_mode(&mode, sticky);
                }
            }
            Action::ExitMode => {
                if let Some(c) = self.clients.get_mut(&id) {
                    c.engine.exit_mode();
                }
            }
            Action::Lock => {
                if let Some(c) = self.clients.get_mut(&id) {
                    c.engine.set_mode("locked", true);
                }
                let leader = self
                    .clients
                    .get(&id)
                    .map(|c| c.keymap.leader().display_short())
                    .unwrap_or_default();
                self.toast(
                    id,
                    format!("Keys locked to the pane · {leader} to unlock"),
                    false,
                );
            }
            Action::Run { command, r#in } => match r#in {
                RunIn::NewTab => self.new_tab(ws, Some(command))?,
                RunIn::SplitRight => self.split(ws, tab_idx, Dir::Right, 50, Some(command))?,
                RunIn::SplitDown => self.split(ws, tab_idx, Dir::Down, 50, Some(command))?,
                RunIn::Background => run_background(&command, self.cwd_of_active(ws))?,
            },
            Action::SendKeys { keys } => {
                if let Some(p) = self.panes.get(&pane) {
                    p.write(unescape(&keys).into_bytes());
                }
            }
            _ => unreachable!("routed by dispatch"),
        }
        Ok(())
    }
}
