//! What the menu bar contains: every menu and its entries.

use rmx_config::action::{Action, Dir};

use super::{Item, Menu};

pub(super) fn split(direction: Dir) -> Action {
    Action::SplitPane {
        direction,
        size: None,
        command: None,
    }
}

fn layout(name: &str) -> Action {
    Action::ApplyLayout { name: name.into() }
}

/// The menu bar. Labels use Windows Terminal / VS Code vocabulary.
pub fn menus() -> Vec<Menu> {
    use Item::{Entry, Separator};
    vec![
        Menu {
            title: "File",
            items: vec![
                Entry("New Tab", Action::NewTab),
                Entry("New Workspace", Action::NewWorkspace),
                Separator,
                Entry("Close Pane", Action::ClosePane),
                Entry("Close Tab", Action::CloseTab),
                Separator,
                Entry("Leave (keep running)", Action::Detach),
                Entry("Quit rmx", Action::Quit),
            ],
        },
        Menu {
            title: "Edit",
            items: vec![
                Entry("Select / Scroll Mode", Action::SelectMode),
                Entry("Copy", Action::Copy),
                Entry("Paste", Action::Paste),
                Entry("Find…", Action::Find),
                Separator,
                Entry("Settings (config.toml)", Action::OpenConfig),
                Entry("Reload Settings", Action::ReloadConfig),
            ],
        },
        Menu {
            title: "View",
            items: vec![
                Entry("Command Palette…", Action::CommandPalette),
                Entry("Session Manager…", Action::WorkspaceSwitcher),
                Separator,
                Entry("Zoom Pane", Action::TogglePaneZoom),
                Entry("Type in All Panes", Action::ToggleSyncInput),
                Separator,
                Entry("Layout: Side by Side", layout("even-horizontal")),
                Entry("Layout: Stacked", layout("even-vertical")),
                Entry("Layout: Main Left", layout("main-left")),
                Entry("Layout: Main Top", layout("main-top")),
                Entry("Layout: Grid", layout("tiled")),
            ],
        },
        Menu {
            title: "Tab",
            items: vec![
                Entry("New Tab", Action::NewTab),
                Entry("Rename Tab…", Action::RenameTab),
                Separator,
                Entry("Next Tab", Action::NextTab),
                Entry("Previous Tab", Action::PrevTab),
                Entry("Move Tab Left", Action::MoveTab { delta: -1 }),
                Entry("Move Tab Right", Action::MoveTab { delta: 1 }),
                Separator,
                Entry("Close Tab", Action::CloseTab),
            ],
        },
        Menu {
            title: "Pane",
            items: vec![
                Entry("Split Right", split(Dir::Right)),
                Entry("Split Down", split(Dir::Down)),
                Entry("Split Left", split(Dir::Left)),
                Entry("Split Up", split(Dir::Up)),
                Separator,
                Entry(
                    "Focus Left",
                    Action::MoveFocus {
                        direction: Dir::Left,
                    },
                ),
                Entry(
                    "Focus Right",
                    Action::MoveFocus {
                        direction: Dir::Right,
                    },
                ),
                Entry("Focus Up", Action::MoveFocus { direction: Dir::Up }),
                Entry(
                    "Focus Down",
                    Action::MoveFocus {
                        direction: Dir::Down,
                    },
                ),
                Entry(
                    "Swap with Right",
                    Action::SwapPane {
                        direction: Dir::Right,
                    },
                ),
                Entry(
                    "Resize Mode",
                    Action::EnterMode {
                        mode: "resize".into(),
                        sticky: true,
                    },
                ),
                Separator,
                Entry("Close Pane", Action::ClosePane),
            ],
        },
        Menu {
            title: "Workspace",
            items: vec![
                Entry("Session Manager…", Action::WorkspaceSwitcher),
                Entry("New Workspace", Action::NewWorkspace),
                Entry("Rename Workspace…", Action::RenameWorkspace),
                Separator,
                Entry("Next Workspace", Action::NextWorkspace),
                Entry("Previous Workspace", Action::PrevWorkspace),
                Separator,
                Entry("Lock Keys (send all to pane)", Action::Lock),
                Entry("Leave (keep running)", Action::Detach),
            ],
        },
        Menu {
            title: "Help",
            items: vec![
                Entry("Keyboard Shortcuts", Action::ShowHelp),
                Entry("Command Palette…", Action::CommandPalette),
                Entry("Settings (config.toml)", Action::OpenConfig),
            ],
        },
    ]
}
