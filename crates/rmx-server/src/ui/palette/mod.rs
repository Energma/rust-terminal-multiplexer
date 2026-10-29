//! Command palette (Ctrl+Shift+P style): fuzzy search over every action,
//! user `[[commands]]` and workspaces, each shown with its key binding so the
//! palette doubles as the way people discover shortcuts.

mod input;
mod render;

use ratatui::layout::Rect;
use rmx_config::action::{Action, palette_actions};

use super::{Cmd, Ctx, Outcome, TextInput, centered, fuzzy_score};
use crate::state::WorkspaceId;

enum Entry {
    Action {
        label: String,
        detail: String,
        action: Action,
    },
    Workspace {
        id: WorkspaceId,
        name: String,
    },
}

impl Entry {
    fn text(&self) -> String {
        match self {
            Entry::Action { label, detail, .. } => format!("{detail}: {label}"),
            Entry::Workspace { name, .. } => format!("Workspace: {name}"),
        }
    }
}

#[derive(Default)]
pub struct Palette {
    input: TextInput,
    selected: usize,
}

impl Palette {
    fn entries(&self, ctx: &Ctx) -> Vec<Entry> {
        let mut all: Vec<Entry> = palette_actions()
            .into_iter()
            .map(|action| Entry::Action {
                label: action.label(),
                detail: format!("{:?}", action.category()),
                action,
            })
            .collect();
        all.extend(ctx.commands.iter().map(|c| Entry::Action {
            label: c.name.clone(),
            detail: "Custom".into(),
            action: c.action.clone(),
        }));
        all.extend(ctx.workspaces.iter().map(|w| Entry::Workspace {
            id: w.id,
            name: w.name.clone(),
        }));

        let mut scored: Vec<(i32, usize, Entry)> = all
            .into_iter()
            .enumerate()
            .filter_map(|(i, e)| fuzzy_score(&self.input.text, &e.text()).map(|s| (s, i, e)))
            .collect();
        scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
        scored.into_iter().map(|(_, _, e)| e).collect()
    }

    fn choose(&self, ctx: &Ctx, index: usize) -> Outcome {
        match self.entries(ctx).into_iter().nth(index) {
            Some(Entry::Action { action, .. }) => Outcome::run(action),
            Some(Entry::Workspace { id, .. }) => Outcome::cmd(
                true,
                Cmd::Open {
                    workspace: id,
                    tab: None,
                    pane: None,
                },
            ),
            None => Outcome::STAY,
        }
    }

    fn rect(screen: Rect) -> Rect {
        let r = centered(screen, 72, 20);
        Rect {
            y: screen.y + 2,
            ..r
        }
        .intersection(screen)
    }
}
