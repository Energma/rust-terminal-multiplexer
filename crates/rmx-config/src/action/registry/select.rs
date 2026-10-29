//! Registry rows for select mode, search and the clipboard.

use super::{ActionInfo, Category as C, info};

pub(super) const ROWS: &[ActionInfo] = &[
    info(
        "select-mode",
        "Select mode",
        C::Edit,
        "Scroll back and select text with the keyboard",
    ),
    info("find", "Find", C::Edit, "Search the scrollback"),
    info(
        "find-next",
        "Find next",
        C::Edit,
        "Jump to the next search match",
    ),
    info(
        "find-prev",
        "Find previous",
        C::Edit,
        "Jump to the previous search match",
    ),
    info(
        "copy",
        "Copy",
        C::Edit,
        "Copy the selection to the clipboard",
    ),
    info(
        "paste",
        "Paste",
        C::Edit,
        "Paste the clipboard into the focused pane",
    ),
    info(
        "copy-and-exit",
        "Copy and exit",
        C::Edit,
        "Copy the selection and leave select mode",
    ),
    info(
        "select-move",
        "Move cursor",
        C::Edit,
        "Move the select-mode cursor",
    ),
    info(
        "select-extend",
        "Extend selection",
        C::Edit,
        "Move the cursor, starting a selection if there is none (Shift+arrows)",
    ),
    info(
        "select-line-start",
        "Start of line",
        C::Edit,
        "Move the cursor to the start of the line",
    ),
    info(
        "select-line-end",
        "End of line",
        C::Edit,
        "Move the cursor to the end of the line",
    ),
    info(
        "select-start",
        "Start selection",
        C::Edit,
        "Start or clear a selection at the cursor",
    ),
    info("select-line", "Select line", C::Edit, "Select whole lines"),
    info(
        "select-word-left",
        "Previous word",
        C::Edit,
        "Move the cursor one word left",
    ),
    info(
        "select-word-right",
        "Next word",
        C::Edit,
        "Move the cursor one word right",
    ),
];
