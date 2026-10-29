//! Moving the border next to a pane, the way an arrow-key resize expects.

use ratatui::layout::Rect;

use super::geometry::split_rects;
use super::{Axis, Layout, Node, PaneId, Side};

impl Layout {
    /// Moves the border nearest `pane` on `side` by `cells` towards `side`:
    /// the pane grows if it has a neighbour that way, otherwise it shrinks
    /// from the opposite edge (the Windows Terminal behaviour).
    pub fn resize(&mut self, pane: PaneId, side: Side, cells: u16, area: Rect) -> bool {
        let path = match path_to(&self.root, pane) {
            Some(p) => p,
            None => return false,
        };
        // Try the innermost split on this axis that has a child beyond the
        // pane in the requested direction; else move the edge behind it.
        for grow in [true, false] {
            let mut node = &self.root;
            let mut node_area = area;
            let mut candidate: Option<(Vec<usize>, Rect)> = None;
            let mut prefix = Vec::new();
            for &idx in &path {
                let Node::Split { axis, children } = node else {
                    break;
                };
                let rects = split_rects(*axis, children, node_area);
                let has_neighbor = if side.forward() == grow {
                    idx + 1 < children.len()
                } else {
                    idx > 0
                };
                if *axis == side.axis() && has_neighbor {
                    candidate = Some((prefix.clone(), node_area));
                }
                prefix.push(idx);
                node_area = rects[idx];
                node = &children[idx].0;
            }
            if let Some((split_path, split_area)) = candidate {
                let child_idx = path[split_path.len()];
                let split = node_at_mut(&mut self.root, &split_path);
                let Node::Split { axis, children } = split else {
                    unreachable!()
                };
                // The border between `a` and `b` moves; `a` is before it.
                let (a, b) = if side.forward() == grow {
                    (child_idx, child_idx + 1)
                } else {
                    (child_idx - 1, child_idx)
                };
                let delta = if side.forward() {
                    i32::from(cells)
                } else {
                    -i32::from(cells)
                };
                move_border(*axis, children, split_area, a, b, delta);
                return true;
            }
        }
        false
    }
}

fn path_to(node: &Node, pane: PaneId) -> Option<Vec<usize>> {
    match node {
        Node::Leaf(p) => (*p == pane).then(Vec::new),
        Node::Split { children, .. } => children.iter().enumerate().find_map(|(i, (c, _))| {
            path_to(c, pane).map(|mut rest| {
                rest.insert(0, i);
                rest
            })
        }),
    }
}

fn node_at_mut<'a>(mut node: &'a mut Node, path: &[usize]) -> &'a mut Node {
    for &i in path {
        let Node::Split { children, .. } = node else {
            break;
        };
        node = &mut children[i].0;
    }
    node
}

/// Moves the border between children `a` and `a + 1` by `delta` cells.
fn move_border(
    axis: Axis,
    children: &mut [(Node, f32)],
    area: Rect,
    a: usize,
    b: usize,
    delta: i32,
) {
    let rects = split_rects(axis, children, area);
    let len = |r: Rect| match axis {
        Axis::Horizontal => i32::from(r.width),
        Axis::Vertical => i32::from(r.height),
    };
    let (la, lb) = (len(rects[a]), len(rects[b]));
    let delta = delta.clamp(1 - la, lb - 1);
    let pair_weight = children[a].1 + children[b].1;
    let pair_len = (la + lb) as f32;
    children[a].1 = pair_weight * (la + delta) as f32 / pair_len;
    children[b].1 = pair_weight * (lb - delta) as f32 / pair_len;
}
