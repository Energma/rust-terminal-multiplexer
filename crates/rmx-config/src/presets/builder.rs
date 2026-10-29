//! Shared helpers for writing preset tables: the binding accumulator, the
//! split shorthand and the arrow / hjkl direction lists.

use super::PresetBinding;
use crate::action::{Action, Dir};
use crate::keys::parse_sequence;

#[derive(Default)]
pub(super) struct Builder {
    pub(super) v: Vec<PresetBinding>,
}

impl Builder {
    pub(super) fn bind(&mut self, mode: &str, keys: &str, action: Action) {
        let keys = parse_sequence(keys).unwrap_or_else(|e| panic!("preset key {keys:?}: {e}"));
        self.v.push((mode.into(), keys, action));
    }
}

pub(super) fn split(direction: Dir) -> Action {
    Action::SplitPane {
        direction,
        size: None,
        command: None,
    }
}

pub(super) const ARROWS: [(&str, Dir); 4] = [
    ("left", Dir::Left),
    ("right", Dir::Right),
    ("up", Dir::Up),
    ("down", Dir::Down),
];
pub(super) const HJKL: [(&str, Dir); 4] = [
    ("h", Dir::Left),
    ("l", Dir::Right),
    ("k", Dir::Up),
    ("j", Dir::Down),
];
