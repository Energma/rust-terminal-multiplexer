//! Built-in keybinding presets (PLAN §5.5). The user file is merged on top.
//!
//! `L` = the leader key. Tables shared by every preset except `none`:
//!
//! | Mode    | Keys                                                                 |
//! |---------|----------------------------------------------------------------------|
//! | resize  | arrows/hjkl resize 2, Shift+arrows/HJKL resize 5, Esc/Enter/q exit    |
//! | select  | arrows/hjkl move, v start, V line, b/w word, Enter/y/^C copy+exit,     |
//! |         | / find, n/N next/prev match, PgUp/PgDn page, g/G top/bottom, Esc/q exit |
//! | locked  | L exit-mode                                                           |
//!
//! **windows** (default, leader Ctrl+Space). Normal: F10 menu, F1 help, Alt+1–9 tab,
//! Alt+]/[ next/prev tab, Alt+Shift+H/J/K/L focus. Leader: `t` new tab, `x` close tab,
//! `w` close pane, `p` palette, `n`/`b` next/prev tab, `1`–`9` tab, `|` (and `\`) split
//! right, `-` split down, arrows focus, `r` resize mode, `z` zoom, `[` select mode,
//! `]` paste, `/` find, `d` detach, `l` lock, `s` workspace switcher, `N` new
//! workspace, `(`/`)` prev/next workspace, `,` rename tab, `.` rename workspace,
//! `space` next layout, `R` reload config, `?` help. Pressing L twice sends L itself
//! to the pane (handled by the engine).
//! Direct layer (`keymap(true)`): ^Shift+P palette, ^Shift+T new tab, ^Tab/^Shift+Tab
//! next/prev tab, Alt+Shift+Plus split right, Alt+Shift+Minus (and Alt+_) split down,
//! Alt+arrows focus, Alt+Shift+arrows resize 2, ^Shift+W close pane, ^Shift+M select
//! mode, ^Shift+F find, ^Shift+C copy, ^Shift+V paste.
//!
//! **vim**: windows' normal and direct layers; the leader table swaps in `h/j/k/l`
//! focus, `s` split down, `v` split right, `q` close pane, and moves lock to `L`
//! (Shift+L) and the workspace switcher to `S` because `l`/`s` are taken.
//!
//! **tmux** (leader Ctrl+B, no direct layer). Normal: F10 menu. Leader: `c` new tab,
//! `n`/`p` next/prev tab, `1`–`9` tab, `0` tab 10, `%` split right, `"` split down,
//! `x` close pane, `&` close tab, `z` zoom, arrows focus, `o` focus right (tmux's
//! "next pane", approximated), ^arrows resize 1, Alt+arrows resize 5, `[` select,
//! `]` paste, `d` detach, `,` rename tab, `$` rename workspace, `s` switcher,
//! `(`/`)` prev/next workspace, `:` palette, `?` help, `space` next layout.
//!
//! **none**: only escape hatches, so a fresh config can't lock the user out: F10
//! menu, L `?` help, Esc exits select/resize mode, L unlocks locked mode.

use crate::action::{Action, Dir};
use crate::keys::{KeyChord, parse_sequence};

pub const PRESETS: &[&str] = &["windows", "tmux", "vim", "none"];

/// One preset binding: (mode, key sequence, action).
pub(crate) type PresetBinding = (String, Vec<KeyChord>, Action);

/// The leader key a preset uses unless `[leader] key` overrides it.
pub fn preset_leader(preset: &str) -> KeyChord {
    let key = if preset == "tmux" {
        "ctrl+b"
    } else {
        "ctrl+space"
    };
    KeyChord::parse(key).expect("valid preset leader")
}

/// Bindings for `preset` with the given leader; `direct` adds the Ctrl+Shift layer
/// for presets that have one. Unknown presets yield nothing (config validates names).
pub(crate) fn bindings(preset: &str, leader: KeyChord, direct: bool) -> Vec<PresetBinding> {
    let mut b = Builder::default();
    match preset {
        "windows" => {
            windows_normal(&mut b);
            windows_leader(&mut b, false);
            if direct {
                direct_layer(&mut b);
            }
            common_modes(&mut b, leader);
        }
        "vim" => {
            windows_normal(&mut b);
            windows_leader(&mut b, true);
            if direct {
                direct_layer(&mut b);
            }
            common_modes(&mut b, leader);
        }
        "tmux" => {
            b.bind("normal", "f10", Action::OpenMenu);
            tmux_leader(&mut b);
            common_modes(&mut b, leader);
        }
        "none" => {
            b.bind("normal", "f10", Action::OpenMenu);
            b.bind("leader", "?", Action::ShowHelp);
            b.bind("select", "esc", Action::ExitMode);
            b.bind("resize", "esc", Action::ExitMode);
            b.v.push(("locked".into(), vec![leader], Action::ExitMode));
        }
        _ => {}
    }
    b.v
}

#[derive(Default)]
struct Builder {
    v: Vec<PresetBinding>,
}

impl Builder {
    fn bind(&mut self, mode: &str, keys: &str, action: Action) {
        let keys = parse_sequence(keys).unwrap_or_else(|e| panic!("preset key {keys:?}: {e}"));
        self.v.push((mode.into(), keys, action));
    }
}

fn split(direction: Dir) -> Action {
    Action::SplitPane {
        direction,
        size: None,
        command: None,
    }
}

const ARROWS: [(&str, Dir); 4] = [
    ("left", Dir::Left),
    ("right", Dir::Right),
    ("up", Dir::Up),
    ("down", Dir::Down),
];
const HJKL: [(&str, Dir); 4] = [
    ("h", Dir::Left),
    ("l", Dir::Right),
    ("k", Dir::Up),
    ("j", Dir::Down),
];

fn windows_normal(b: &mut Builder) {
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

fn windows_leader(b: &mut Builder, vim: bool) {
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

fn direct_layer(b: &mut Builder) {
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

fn tmux_leader(b: &mut Builder) {
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

fn common_modes(b: &mut Builder, leader: KeyChord) {
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn presets_have_no_duplicate_keys() {
        for p in PRESETS {
            let leader = preset_leader(p);
            let mut seen = HashSet::new();
            for (mode, keys, a) in bindings(p, leader, true) {
                assert!(
                    seen.insert((mode.clone(), keys.clone())),
                    "{p}: duplicate {mode} {keys:?} ({a:?})"
                );
            }
        }
    }

    #[test]
    fn direct_layer_is_opt_in() {
        let ctrl_shift_t = parse_sequence("ctrl+shift+t").unwrap();
        let has = |direct| {
            bindings("windows", preset_leader("windows"), direct)
                .iter()
                .any(|(m, k, _)| m == "normal" && *k == ctrl_shift_t)
        };
        assert!(has(true));
        assert!(!has(false));
    }

    #[test]
    fn tmux_uses_ctrl_b() {
        assert_eq!(preset_leader("tmux"), KeyChord::parse("ctrl+b").unwrap());
        assert_eq!(
            preset_leader("windows"),
            KeyChord::parse("ctrl+space").unwrap()
        );
    }
}
