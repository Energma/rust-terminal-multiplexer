//! The pane layout of one tab: a tree of splits, modelled on tmux's
//! `layout_cell` (layout.c).
//!
//! Sizes are stored as relative weights rather than cells so a layout keeps
//! its proportions across terminal resizes and serialises independently of
//! any client's size. Children of a split are separated by one-cell borders.

mod arrange;
mod geometry;
mod resize;
#[cfg(test)]
mod tests;
mod tree;

use serde::{Deserialize, Serialize};

pub type PaneId = u32;

/// Layout presets, as in tmux's `select-layout`.
pub const PRESETS: &[&str] = &[
    "even-horizontal",
    "even-vertical",
    "main-left",
    "main-top",
    "tiled",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Axis {
    /// Children sit side by side, separated by vertical borders.
    Horizontal,
    /// Children are stacked, separated by horizontal borders.
    Vertical,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Left,
    Right,
    Up,
    Down,
}

impl Side {
    fn axis(self) -> Axis {
        match self {
            Side::Left | Side::Right => Axis::Horizontal,
            Side::Up | Side::Down => Axis::Vertical,
        }
    }

    /// Whether this side points towards higher coordinates.
    fn forward(self) -> bool {
        matches!(self, Side::Right | Side::Down)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Node {
    Leaf(PaneId),
    Split {
        axis: Axis,
        children: Vec<(Node, f32)>,
    },
}

/// A border between two children of a split, in screen cells.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Separator {
    pub x: u16,
    pub y: u16,
    pub len: u16,
    /// A vertical line (between side-by-side panes).
    pub vertical: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Layout {
    root: Node,
}
