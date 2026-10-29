//! Registry rows for tab and pane actions.

use super::{ActionInfo, Category as C, info};

pub(super) const ROWS: &[ActionInfo] = &[
    info(
        "new-tab",
        "New tab",
        C::Tab,
        "Open a new tab with the default shell",
    ),
    info(
        "close-tab",
        "Close tab",
        C::Tab,
        "Close the current tab and all its panes",
    ),
    info(
        "rename-tab",
        "Rename tab",
        C::Tab,
        "Give the current tab a name",
    ),
    info(
        "next-tab",
        "Next tab",
        C::Tab,
        "Switch to the tab on the right",
    ),
    info(
        "prev-tab",
        "Previous tab",
        C::Tab,
        "Switch to the tab on the left",
    ),
    info(
        "go-to-tab",
        "Go to tab",
        C::Tab,
        "Switch to tab number `index` (1-based)",
    ),
    info(
        "move-tab",
        "Move tab",
        C::Tab,
        "Move the current tab `delta` places",
    ),
    info(
        "split-pane",
        "Split pane",
        C::Pane,
        "Split the focused pane in `direction`",
    ),
    info(
        "close-pane",
        "Close pane",
        C::Pane,
        "Close the focused pane",
    ),
    info(
        "move-focus",
        "Move focus",
        C::Pane,
        "Focus the neighbouring pane",
    ),
    info(
        "resize-pane",
        "Resize pane",
        C::Pane,
        "Move the focused pane's border",
    ),
    info(
        "toggle-pane-zoom",
        "Zoom pane",
        C::Pane,
        "Toggle the focused pane filling the tab",
    ),
    info(
        "swap-pane",
        "Swap pane",
        C::Pane,
        "Swap the focused pane with its neighbour",
    ),
    info(
        "apply-layout",
        "Apply layout",
        C::Pane,
        "Rearrange the tab's panes",
    ),
    info(
        "toggle-sync-input",
        "Synchronize input",
        C::Pane,
        "Type into every pane of the tab at once",
    ),
];
