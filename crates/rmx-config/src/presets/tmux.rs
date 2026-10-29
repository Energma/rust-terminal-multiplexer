//! The `tmux` preset's leader table (leader Ctrl+B, no direct layer).

use super::builder::{ARROWS, Builder, split};
use crate::action::{Action, Dir};

pub(super) fn tmux_leader(b: &mut Builder) {
    use Action::*;
    let l = |b: &mut Builder, k: &str, a: Action| b.bind("leader", k, a);
    l(b, "c", NewTab);
    l(b, "n", NextTab);
    l(b, "p", PrevTab);
    for i in 1..=9 {
        l(b, &i.to_string(), GoToTab { index: i });
    }
    l(b, "0", GoToTab { index: 10 });
    l(b, "%", split(Dir::Right));
    l(b, "\"", split(Dir::Down));
    l(b, "x", ClosePane);
    l(b, "&", CloseTab);
    l(b, "z", TogglePaneZoom);
    for (k, direction) in ARROWS {
        l(b, k, MoveFocus { direction });
        l(
            b,
            &format!("ctrl+{k}"),
            ResizePane {
                direction,
                amount: 1,
            },
        );
        l(
            b,
            &format!("alt+{k}"),
            ResizePane {
                direction,
                amount: 5,
            },
        );
    }
    l(
        b,
        "o",
        MoveFocus {
            direction: Dir::Right,
        },
    );
    l(b, "[", SelectMode);
    l(b, "]", Paste);
    l(b, "d", Detach);
    l(b, ",", RenameTab);
    l(b, "$", RenameWorkspace);
    l(b, "s", WorkspaceSwitcher);
    l(b, "(", PrevWorkspace);
    l(b, ")", NextWorkspace);
    l(b, ":", CommandPalette);
    l(b, "?", ShowHelp);
    l(
        b,
        "space",
        ApplyLayout {
            name: "next".into(),
        },
    );
}
