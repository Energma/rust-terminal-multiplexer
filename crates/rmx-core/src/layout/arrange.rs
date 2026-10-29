//! Rearranging panes without changing the set: swaps and presets.

use ratatui::layout::Rect;

use super::{Axis, Layout, Node, PaneId, Side};

impl Layout {
    /// Swaps `pane` with its neighbour on `side`.
    pub fn swap(&mut self, pane: PaneId, side: Side, area: Rect) -> Option<PaneId> {
        let other = self.neighbor(pane, side, area)?;
        swap_leaves(&mut self.root, pane, other);
        Some(other)
    }

    /// Rebuilds the tree as preset `name`, keeping the pane order.
    pub fn apply_preset(&mut self, name: &str) -> bool {
        let panes = self.panes();
        let leaves = |ids: &[PaneId]| {
            ids.iter()
                .map(|&p| (Node::Leaf(p), 1.0))
                .collect::<Vec<_>>()
        };
        let split = |axis, children: Vec<(Node, f32)>| {
            if children.len() == 1 {
                children.into_iter().next().unwrap().0
            } else {
                Node::Split { axis, children }
            }
        };
        self.root = match name {
            "even-horizontal" => split(Axis::Horizontal, leaves(&panes)),
            "even-vertical" => split(Axis::Vertical, leaves(&panes)),
            "main-left" | "main-top" if panes.len() > 1 => {
                let (outer, inner) = if name == "main-left" {
                    (Axis::Horizontal, Axis::Vertical)
                } else {
                    (Axis::Vertical, Axis::Horizontal)
                };
                let rest = split(inner, leaves(&panes[1..]));
                Node::Split {
                    axis: outer,
                    children: vec![(Node::Leaf(panes[0]), 3.0), (rest, 2.0)],
                }
            }
            "main-left" | "main-top" => Node::Leaf(panes[0]),
            "tiled" => {
                let cols = (panes.len() as f32).sqrt().ceil() as usize;
                let rows = panes
                    .chunks(cols.max(1))
                    .map(|row| (split(Axis::Horizontal, leaves(row)), 1.0))
                    .collect();
                split(Axis::Vertical, rows)
            }
            _ => return false,
        };
        true
    }
}

fn swap_leaves(node: &mut Node, x: PaneId, y: PaneId) {
    match node {
        Node::Leaf(p) if *p == x => *p = y,
        Node::Leaf(p) if *p == y => *p = x,
        Node::Leaf(_) => {}
        Node::Split { children, .. } => children.iter_mut().for_each(|(c, _)| swap_leaves(c, x, y)),
    }
}
