//! The `windows` preset (the default) and its `vim` variant: normal-mode keys,
//! the leader table (with vim's overrides) and the Ctrl+Shift / Alt direct layer.

use super::builder::{ARROWS, Builder, HJKL, split};
use crate::action::{Action, Dir};

pub(super) fn windows_normal(b: &mut Builder) {
    b.bind("normal", "f10", Action::OpenMenu);
    b.bind("normal", "f1", Action::ShowHelp);
    for i in 1..=9 {
        b.bind("normal", &format!("alt+{i}"), Action::GoToTab { index: i });
    }
    b.bind("normal", "alt+]", Action::NextTab);
    b.bind("normal", "alt+[", Action::PrevTab);
    for (k, direction) in HJKL {
        b.bind(
            "normal",
            &format!("alt+shift+{k}"),
            Action::MoveFocus { direction },
        );
    }
}

pub(super) fn windows_leader(b: &mut Builder, vim: bool) {
    use Action::*;
    let l = |b: &mut Builder, k: &str, a: Action| b.bind("leader", k, a);
    l(b, "t", NewTab);
    l(b, "x", CloseTab);
    l(b, "w", ClosePane);
    l(b, "p", CommandPalette);
    l(b, "n", NextTab);
    l(b, "b", PrevTab);
    for i in 1..=9 {
        l(b, &i.to_string(), GoToTab { index: i });
    }
    l(b, "|", split(Dir::Right));
    l(b, "\\", split(Dir::Right));
    l(b, "-", split(Dir::Down));
    for (k, direction) in ARROWS {
        l(b, k, MoveFocus { direction });
    }
    l(
        b,
        "r",
        EnterMode {
            mode: "resize".into(),
            sticky: true,
        },
    );
    l(b, "z", TogglePaneZoom);
    l(b, "[", SelectMode);
    l(b, "]", Paste);
    l(b, "/", Find);
    l(b, "d", Detach);
    l(b, "e", OpenEditor);
    l(b, "shift+n", NewWorkspace);
    l(b, "(", PrevWorkspace);
    l(b, ")", NextWorkspace);
    l(b, ",", RenameTab);
    l(b, ".", RenameWorkspace);
    l(
        b,
        "space",
        ApplyLayout {
            name: "next".into(),
        },
    );
    l(b, "shift+r", ReloadConfig);
    l(b, "?", ShowHelp);
    if vim {
        for (k, direction) in HJKL {
            l(b, k, MoveFocus { direction });
        }
        l(b, "s", split(Dir::Down));
        l(b, "v", split(Dir::Right));
        l(b, "q", ClosePane);
        l(b, "shift+l", Lock);
        l(b, "shift+s", WorkspaceSwitcher);
    } else {
        l(b, "l", Lock);
        l(b, "s", WorkspaceSwitcher);
    }
}

pub(super) fn direct_layer(b: &mut Builder) {
    use Action::*;
    let n = |b: &mut Builder, k: &str, a: Action| b.bind("normal", k, a);
    n(b, "ctrl+shift+p", CommandPalette);
    n(b, "ctrl+shift+t", NewTab);
    n(b, "ctrl+tab", NextTab);
    n(b, "ctrl+shift+tab", PrevTab);
    n(b, "alt+shift+plus", split(Dir::Right));
    n(b, "alt+shift+minus", split(Dir::Down));
    // Shift+Minus is `_` on most layouts, and some terminals report it that way.
    n(b, "alt+_", split(Dir::Down));
    for (k, direction) in ARROWS {
        n(b, &format!("alt+{k}"), MoveFocus { direction });
        n(
            b,
            &format!("alt+shift+{k}"),
            ResizePane {
                direction,
                amount: 2,
            },
        );
    }
    n(b, "ctrl+shift+w", ClosePane);
    n(b, "ctrl+shift+m", SelectMode);
    n(b, "ctrl+shift+f", Find);
    n(b, "ctrl+shift+c", Copy);
    n(b, "ctrl+shift+v", Paste);
}
