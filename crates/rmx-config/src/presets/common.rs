//! Resize, select and locked mode tables shared by every preset except `none`.

use super::builder::{ARROWS, Builder, HJKL};
use crate::action::{Action, Dir};
use crate::keys::KeyChord;

pub(super) fn common_modes(b: &mut Builder, leader: KeyChord) {
    use Action::*;
    for (keys, step) in [(ARROWS, 2), (HJKL, 2)] {
        for (k, direction) in keys {
            b.bind(
                "resize",
                k,
                ResizePane {
                    direction,
                    amount: step,
                },
            );
            b.bind(
                "resize",
                &format!("shift+{k}"),
                ResizePane {
                    direction,
                    amount: 5,
                },
            );
        }
    }
    for k in ["esc", "enter", "q"] {
        b.bind("resize", k, ExitMode);
    }

    for (k, direction) in ARROWS.into_iter().chain(HJKL) {
        b.bind("select", k, SelectMove { direction });
    }
    let s = |b: &mut Builder, k: &str, a: Action| b.bind("select", k, a);
    // Windows text-editing conventions alongside the vi-style keys.
    for (k, direction) in [
        ("shift+left", Dir::Left),
        ("shift+right", Dir::Right),
        ("shift+up", Dir::Up),
        ("shift+down", Dir::Down),
    ] {
        b.bind("select", k, SelectExtend { direction });
    }
    for k in ["home", "0"] {
        b.bind("select", k, SelectLineStart);
    }
    for k in ["end", "$"] {
        b.bind("select", k, SelectLineEnd);
    }
    s(b, "v", SelectStart);
    s(b, "shift+v", SelectLine);
    s(b, "b", SelectWordLeft);
    s(b, "w", SelectWordRight);
    s(b, "enter", CopyAndExit);
    s(b, "y", CopyAndExit);
    s(b, "ctrl+c", CopyAndExit);
    s(b, "/", Find);
    s(b, "n", FindNext);
    s(b, "shift+n", FindPrev);
    s(b, "pgup", ScrollPageUp);
    s(b, "pgdn", ScrollPageDown);
    s(b, "g", ScrollToTop);
    s(b, "shift+g", ScrollToBottom);
    s(b, "esc", ExitMode);
    s(b, "q", ExitMode);

    b.v.push(("locked".into(), vec![leader], ExitMode));
}
