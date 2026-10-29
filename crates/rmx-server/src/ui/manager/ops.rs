//! Manager keys that change sessions: new workspace or tab, split, rename and
//! close. Each becomes a [`Cmd`] (often via a prompt or confirmation).

use std::path::Path;

use crossterm::event::{KeyCode, KeyEvent};

use super::{Manager, Row, tab_id};
use crate::state::WorkspaceId;
use crate::ui::{Cmd, Ctx, Outcome, PromptThen};

fn ws_name(ctx: &Ctx, w: WorkspaceId) -> String {
    ctx.workspaces
        .iter()
        .find(|x| x.id == w)
        .map(|x| x.name.clone())
        .unwrap_or_default()
}

impl Manager {
    pub(super) fn operate(&mut self, key: &KeyEvent, row: Row, ctx: &Ctx) -> Outcome {
        match key.code {
            KeyCode::Char('n') => {
                return Outcome::cmd(
                    false,
                    Cmd::Prompt {
                        title: "New workspace name".into(),
                        initial: ctx
                            .launch_cwd
                            .and_then(Path::file_name)
                            .map(|n| n.to_string_lossy().into_owned())
                            .unwrap_or_default(),
                        then: PromptThen::NewWorkspace {
                            cwd: ctx.launch_cwd.map(Path::to_path_buf),
                        },
                    },
                );
            }
            KeyCode::Char('t') => {
                let Some(w) = row.workspace() else {
                    return Outcome::STAY;
                };
                self.expanded_ws.insert(w);
                return Outcome::cmd(false, Cmd::NewTab { workspace: w });
            }
            KeyCode::Char('|' | 'v') => {
                if let Some((workspace, tab)) = Self::tab_for(row, ctx) {
                    return Outcome::cmd(
                        false,
                        Cmd::Split {
                            workspace,
                            tab,
                            right: true,
                        },
                    );
                }
            }
            KeyCode::Char('-' | 's') => {
                if let Some((workspace, tab)) = Self::tab_for(row, ctx) {
                    return Outcome::cmd(
                        false,
                        Cmd::Split {
                            workspace,
                            tab,
                            right: false,
                        },
                    );
                }
            }
            KeyCode::Char('r') | KeyCode::F(2) => {
                let (title, initial, then) = match row {
                    Row::NewHere => return Outcome::STAY,
                    Row::Workspace(w) => (
                        "Rename workspace".to_owned(),
                        ws_name(ctx, w),
                        PromptThen::RenameWorkspace { workspace: w },
                    ),
                    Row::Tab(w, t) | Row::Pane(w, t, _) => {
                        let ws = ctx.workspaces.iter().find(|x| x.id == w);
                        let name = ws
                            .map(|ws| crate::tab_title(&ws.tabs[t], ctx.panes))
                            .unwrap_or_default();
                        (
                            "Rename tab".to_owned(),
                            name,
                            PromptThen::RenameTab {
                                workspace: w,
                                tab: tab_id(ctx, w, t).unwrap_or_default(),
                            },
                        )
                    }
                };
                return Outcome::cmd(
                    false,
                    Cmd::Prompt {
                        title,
                        initial,
                        then,
                    },
                );
            }
            KeyCode::Char('x') | KeyCode::Delete => {
                let (message, then) = match row {
                    Row::NewHere => return Outcome::STAY,
                    Row::Workspace(w) => (
                        format!("Close workspace \"{}\" and all its tabs?", ws_name(ctx, w)),
                        Cmd::CloseWorkspace { workspace: w },
                    ),
                    Row::Tab(w, t) | Row::Pane(w, t, _) => (
                        format!("Close tab {} of \"{}\"?", t + 1, ws_name(ctx, w)),
                        Cmd::CloseTab {
                            workspace: w,
                            tab: tab_id(ctx, w, t).unwrap_or_default(),
                        },
                    ),
                };
                return Outcome::cmd(
                    false,
                    Cmd::Confirm {
                        message,
                        then: Box::new(then),
                    },
                );
            }
            _ => {}
        }
        Outcome::STAY
    }
}
