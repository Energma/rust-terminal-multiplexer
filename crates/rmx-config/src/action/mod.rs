//! The action registry: every command rmx can perform, whatever triggered it.
//!
//! Keys, menus, the palette, the CLI and the API all speak [`Action`], so a new
//! feature is one variant plus one [`ActionInfo`] row. TOML parsing is hand-rolled
//! (not serde) so errors can name the exact argument and suggest fixes.

mod describe;
mod palette;
mod params;
mod parse;
mod registry;
#[cfg(test)]
mod tests;

use serde::{Deserialize, Serialize};

pub use palette::palette_actions;
pub use params::{Dir, LAYOUTS, RunIn};
pub(crate) use parse::from_value;
pub use registry::{ACTIONS, ActionInfo, Category, action_info, all_action_ids};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Action {
    // tabs
    NewTab,
    CloseTab,
    RenameTab,
    NextTab,
    PrevTab,
    /// 1-based, like the numbers on the tab bar.
    GoToTab {
        index: usize,
    },
    MoveTab {
        delta: i32,
    },
    // panes
    SplitPane {
        direction: Dir,
        /// Percent of the split pane given to the new one, 1..=99.
        size: Option<u16>,
        command: Option<String>,
    },
    ClosePane,
    MoveFocus {
        direction: Dir,
    },
    ResizePane {
        direction: Dir,
        amount: u16,
    },
    TogglePaneZoom,
    SwapPane {
        direction: Dir,
    },
    ApplyLayout {
        name: String,
    },
    ToggleSyncInput,
    // select mode / clipboard / scrolling
    SelectMode,
    Find,
    FindNext,
    FindPrev,
    Copy,
    Paste,
    CopyAndExit,
    SelectMove {
        direction: Dir,
    },
    /// Shift+arrow style: moves the cursor, starting a selection if needed.
    SelectExtend {
        direction: Dir,
    },
    SelectLineStart,
    SelectLineEnd,
    SelectStart,
    SelectLine,
    SelectWordLeft,
    SelectWordRight,
    ScrollUp {
        lines: u16,
    },
    ScrollDown {
        lines: u16,
    },
    ScrollPageUp,
    ScrollPageDown,
    ScrollToTop,
    ScrollToBottom,
    // ui
    CommandPalette,
    OpenMenu,
    ShowHelp,
    WorkspaceSwitcher,
    // workspaces / session
    NewWorkspace,
    RenameWorkspace,
    NextWorkspace,
    PrevWorkspace,
    Detach,
    Quit,
    ReloadConfig,
    OpenConfig,
    // modes
    EnterMode {
        mode: String,
        sticky: bool,
    },
    ExitMode,
    Lock,
    // misc
    Run {
        command: String,
        r#in: RunIn,
    },
    SendKeys {
        keys: String,
    },
}
