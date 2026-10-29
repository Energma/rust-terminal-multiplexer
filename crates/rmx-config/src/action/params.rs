//! Small argument value types shared by several actions: directions, `run`
//! placement and the built-in layout names.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Dir {
    Left,
    Right,
    Up,
    Down,
}

impl Dir {
    pub const ALL: [Dir; 4] = [Dir::Left, Dir::Right, Dir::Up, Dir::Down];

    pub fn as_str(self) -> &'static str {
        match self {
            Dir::Left => "left",
            Dir::Right => "right",
            Dir::Up => "up",
            Dir::Down => "down",
        }
    }
}

/// Where a `run` command's process goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RunIn {
    NewTab,
    SplitRight,
    SplitDown,
    Background,
}

impl RunIn {
    pub(super) const NAMES: [&'static str; 4] =
        ["new-tab", "split-right", "split-down", "background"];

    pub fn as_str(self) -> &'static str {
        match self {
            RunIn::NewTab => "new-tab",
            RunIn::SplitRight => "split-right",
            RunIn::SplitDown => "split-down",
            RunIn::Background => "background",
        }
    }
}

/// Built-in layout presets, plus `"next"` which cycles through them.
pub const LAYOUTS: &[&str] = &[
    "even-horizontal",
    "even-vertical",
    "main-left",
    "main-top",
    "tiled",
];
