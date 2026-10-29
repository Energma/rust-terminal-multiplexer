//! Registry rows for workspaces, the session, key modes and misc commands.

use super::{ActionInfo, Category as C, info};

pub(super) const ROWS: &[ActionInfo] = &[
    info(
        "workspace-switcher",
        "Switch workspace",
        C::Workspace,
        "Pick a workspace, tab or pane",
    ),
    info(
        "new-workspace",
        "New workspace",
        C::Workspace,
        "Create a new workspace",
    ),
    info(
        "rename-workspace",
        "Rename workspace",
        C::Workspace,
        "Give the current workspace a name",
    ),
    info(
        "next-workspace",
        "Next workspace",
        C::Workspace,
        "Switch to the next workspace",
    ),
    info(
        "prev-workspace",
        "Previous workspace",
        C::Workspace,
        "Switch to the previous workspace",
    ),
    info(
        "detach",
        "Leave (keep running)",
        C::File,
        "Detach; everything keeps running in the background",
    ),
    info(
        "quit",
        "Quit",
        C::File,
        "Close every pane and stop the background service",
    ),
    info(
        "reload-config",
        "Reload config",
        C::File,
        "Re-read config.toml",
    ),
    info(
        "open-config",
        "Open config",
        C::File,
        "Edit config.toml in a new tab",
    ),
    info(
        "enter-mode",
        "Enter mode",
        C::View,
        "Switch to another key table",
    ),
    info("exit-mode", "Exit mode", C::View, "Return to normal mode"),
    info(
        "lock",
        "Lock keys",
        C::View,
        "Send every key to the pane until unlocked",
    ),
    info(
        "run",
        "Run command",
        C::File,
        "Run `command` in a new tab, split or the background",
    ),
    info(
        "send-keys",
        "Send keys",
        C::Edit,
        "Type `keys` into the focused pane",
    ),
];
