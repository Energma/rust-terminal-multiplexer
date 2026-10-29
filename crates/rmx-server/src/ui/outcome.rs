//! What overlays hand back to the core: whether to close, and which action
//! or overlay-only command to run.

use rmx_config::action::Action;
use rmx_core::layout::PaneId;

use crate::state::{TabId, WorkspaceId};

/// What an overlay asks the core to do after handling input.
pub struct Outcome {
    /// Remove this overlay from the stack.
    pub pop: bool,
    pub effect: Option<Effect>,
}

impl Outcome {
    pub const STAY: Self = Self {
        pop: false,
        effect: None,
    };
    pub const CLOSE: Self = Self {
        pop: true,
        effect: None,
    };

    pub fn run(action: Action) -> Self {
        Self {
            pop: true,
            effect: Some(Effect::Action(action)),
        }
    }

    pub fn cmd(pop: bool, cmd: Cmd) -> Self {
        Self {
            pop,
            effect: Some(Effect::Cmd(cmd)),
        }
    }
}

pub enum Effect {
    Action(Action),
    Cmd(Cmd),
}

/// Core operations that only overlays trigger.
#[derive(Debug, Clone)]
pub enum Cmd {
    RenameTab {
        workspace: WorkspaceId,
        tab: TabId,
        name: String,
    },
    RenameWorkspace {
        workspace: WorkspaceId,
        name: String,
    },
    NewWorkspace {
        /// `None` names it after its folder.
        name: Option<String>,
        /// `None` uses the current pane's folder.
        cwd: Option<std::path::PathBuf>,
    },
    Find(String),
    Open {
        workspace: WorkspaceId,
        tab: Option<TabId>,
        pane: Option<PaneId>,
    },
    NewTab {
        workspace: WorkspaceId,
    },
    Split {
        workspace: WorkspaceId,
        tab: TabId,
        right: bool,
    },
    CloseWorkspace {
        workspace: WorkspaceId,
    },
    CloseTab {
        workspace: WorkspaceId,
        tab: TabId,
    },
    /// Ask before closing; carries the follow-up command.
    Confirm {
        message: String,
        then: Box<Cmd>,
    },
    Prompt {
        title: String,
        initial: String,
        then: PromptThen,
    },
    QuitServer,
}

/// What a prompt's text is used for.
#[derive(Debug, Clone)]
pub enum PromptThen {
    RenameTab { workspace: WorkspaceId, tab: TabId },
    RenameWorkspace { workspace: WorkspaceId },
    NewWorkspace { cwd: Option<std::path::PathBuf> },
    Find,
}
