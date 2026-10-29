//! Registry rows for scrolling and the UI overlays.

use super::{ActionInfo, Category as C, info};

pub(super) const ROWS: &[ActionInfo] = &[
    info(
        "scroll-up",
        "Scroll up",
        C::View,
        "Scroll the pane up `lines` lines",
    ),
    info(
        "scroll-down",
        "Scroll down",
        C::View,
        "Scroll the pane down `lines` lines",
    ),
    info(
        "scroll-page-up",
        "Scroll page up",
        C::View,
        "Scroll up one screen",
    ),
    info(
        "scroll-page-down",
        "Scroll page down",
        C::View,
        "Scroll down one screen",
    ),
    info(
        "scroll-to-top",
        "Scroll to top",
        C::View,
        "Jump to the oldest scrollback line",
    ),
    info(
        "scroll-to-bottom",
        "Scroll to bottom",
        C::View,
        "Jump back to the live screen",
    ),
    info(
        "command-palette",
        "Command palette",
        C::View,
        "Search and run any action",
    ),
    info("open-menu", "Open menu", C::View, "Open the menu bar"),
    info("show-help", "Show help", C::Help, "List the key bindings"),
];
