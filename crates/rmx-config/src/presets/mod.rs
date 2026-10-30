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
//! `]` paste, `/` find, `d` detach, `e` editor in the pane's folder, `l` lock, `s` workspace switcher, `N` new
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

mod builder;
mod common;
mod lookup;
#[cfg(test)]
mod tests;
mod tmux;
mod windows;

use crate::action::Action;
use crate::keys::KeyChord;

pub(crate) use lookup::bindings;
pub use lookup::preset_leader;

pub const PRESETS: &[&str] = &["windows", "tmux", "vim", "none"];

/// One preset binding: (mode, key sequence, action).
pub(crate) type PresetBinding = (String, Vec<KeyChord>, Action);
