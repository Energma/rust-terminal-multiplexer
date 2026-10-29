//! Overlays drawn on top of the panes: menus, the command palette, prompts,
//! dialogs, help and the session manager.
//!
//! They are composed on the server like everything else, so every client
//! (and every OS) gets the same UI. Overlays form a stack per client: the top
//! one gets input, so e.g. a rename prompt can sit on the session manager.
//! Overlays never mutate core state; they return an [`Effect`] for the core.

pub mod dialog;
mod draw;
mod fuzzy;
pub mod help;
pub mod manager;
pub mod menu;
mod outcome;
mod overlay;
pub mod palette;
mod text_input;

#[cfg(test)]
mod tests;

use std::collections::HashMap;
use std::path::Path;

use rmx_config::Theme;
use rmx_config::keymap::Keymap;
use rmx_core::layout::PaneId;

use crate::pane::Pane;
use crate::state::{Workspace, WorkspaceId};

pub use draw::{centered, contains, list_scroll, panel, put};
pub use fuzzy::fuzzy_score;
pub use outcome::{Cmd, Effect, Outcome, PromptThen};
pub use overlay::Overlay;
pub use text_input::TextInput;

/// Read-only view of the core that overlays render and navigate against.
pub struct Ctx<'a> {
    pub theme: &'a Theme,
    pub keymap: &'a Keymap,
    pub workspaces: &'a [Workspace],
    pub panes: &'a HashMap<PaneId, Pane>,
    /// Interactive clients per workspace (including this one).
    pub attached: &'a HashMap<WorkspaceId, usize>,
    /// Where this client ran `rmx`, if known.
    pub launch_cwd: Option<&'a Path>,
    pub current: WorkspaceId,
    pub config_path: &'a Path,
    pub commands: &'a [rmx_config::CustomCommand],
}
