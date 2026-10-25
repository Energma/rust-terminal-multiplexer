//! The pane layout of one tab: a tree of splits, modelled on tmux's
//! `layout_cell` (layout.c).
//!
//! Sizes are stored as relative weights rather than cells so a layout keeps
//! its proportions across terminal resizes and serialises independently of
//! any client's size. Children of a split are separated by one-cell borders.

use ratatui::layout::Rect;
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

impl Layout {
    pub fn new(pane: PaneId) -> Self {
        Self {
            root: Node::Leaf(pane),
        }
    }

    pub fn root(&self) -> &Node {
        &self.root
    }

    pub fn from_root(root: Node) -> Self {
        Self { root }
    }

    /// Builds a layout from an untrusted tree (e.g. a session file): drops
    /// leaves `keep` rejects and empty splits, and collapses what's left.
    /// `None` if no pane survives.
    pub fn sanitized(root: Node, keep: impl Fn(PaneId) -> bool + Copy) -> Option<Self> {
        fn prune(node: Node, keep: impl Fn(PaneId) -> bool + Copy) -> Option<Node> {
            match node {
                Node::Leaf(p) => keep(p).then_some(Node::Leaf(p)),
                Node::Split { axis, children } => {
                    let children: Vec<_> = children
                        .into_iter()
                        .filter_map(|(c, w)| {
                            prune(c, keep)
                                .map(|c| (c, if w.is_finite() && w > 0.0 { w } else { 1.0 }))
                        })
                        .collect();
                    (!children.is_empty()).then_some(Node::Split { axis, children })
                }
            }
        }
        let mut root = prune(root, keep)?;
        normalize(&mut root);
        Some(Self { root })
    }

    /// Panes in reading order (left-to-right, top-to-bottom per split).
    pub fn panes(&self) -> Vec<PaneId> {
        let mut out = Vec::new();
        collect(&self.root, &mut out);
        out
    }

    pub fn contains(&self, pane: PaneId) -> bool {
        self.panes().contains(&pane)
    }

    /// Places `new` next to `target` on `side`, giving it `percent` of
    /// `target`'s space. Returns false if `target` isn't in this layout.
    pub fn split(&mut self, target: PaneId, new: PaneId, side: Side, percent: u16) -> bool {
        let share = f32::from(percent.clamp(1, 99)) / 100.0;
        split_node(&mut self.root, target, new, side, share)
    }

    /// Removes `pane`; its space goes to its siblings. Returns false if the
    /// pane wasn't found or is the last one (a tab always has a pane).
    pub fn remove(&mut self, pane: PaneId) -> bool {
        if matches!(self.root, Node::Leaf(_)) {
            return false;
        }
        let removed = remove_node(&mut self.root, pane);
        if removed {
            normalize(&mut self.root);
        }
        removed
    }

    /// Pane rectangles and the borders between them for `area`.
    pub fn compute(&self, area: Rect) -> (Vec<(PaneId, Rect)>, Vec<Separator>) {
        let mut panes = Vec::new();
        let mut seps = Vec::new();
        compute_node(&self.root, area, &mut panes, &mut seps);
        (panes, seps)
    }

    pub fn rect_of(&self, pane: PaneId, area: Rect) -> Option<Rect> {
        self.compute(area)
            .0
            .into_iter()
            .find(|(p, _)| *p == pane)
            .map(|(_, r)| r)
    }

    /// The pane adjacent to `pane` on `side`, preferring the one that overlaps
    /// it most (so focus moves where the eye expects).
    pub fn neighbor(&self, pane: PaneId, side: Side, area: Rect) -> Option<PaneId> {
        let (rects, _) = self.compute(area);
        let (_, r) = *rects.iter().find(|(p, _)| *p == pane)?;
        rects
            .iter()
            .filter(|(p, _)| *p != pane)
            .filter_map(|&(p, c)| {
                let (adjacent, overlap) = match side {
                    Side::Right => (
                        c.x == r.right() + 1,
                        overlap(r.y, r.bottom(), c.y, c.bottom()),
                    ),
                    Side::Left => (
                        c.right() + 1 == r.x,
                        overlap(r.y, r.bottom(), c.y, c.bottom()),
                    ),
                    Side::Down => (
                        c.y == r.bottom() + 1,
                        overlap(r.x, r.right(), c.x, c.right()),
                    ),
                    Side::Up => (
                        c.bottom() + 1 == r.y,
                        overlap(r.x, r.right(), c.x, c.right()),
                    ),
                };
                (adjacent && overlap > 0).then_some((p, overlap, (c.y, c.x)))
            })
            // Ties go to the top/left-most pane so focus moves predictably.
            .max_by_key(|&(_, o, pos)| (o, std::cmp::Reverse(pos)))
            .map(|(p, _, _)| p)
    }

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

fn collect(node: &Node, out: &mut Vec<PaneId>) {
    match node {
        Node::Leaf(p) => out.push(*p),
        Node::Split { children, .. } => children.iter().for_each(|(c, _)| collect(c, out)),
    }
}

fn split_node(node: &mut Node, target: PaneId, new: PaneId, side: Side, share: f32) -> bool {
    match node {
        Node::Leaf(p) if *p == target => {
            let old = (Node::Leaf(target), 1.0 - share);
            let fresh = (Node::Leaf(new), share);
            let children = if side.forward() {
                vec![old, fresh]
            } else {
                vec![fresh, old]
            };
            *node = Node::Split {
                axis: side.axis(),
                children,
            };
            true
        }
        Node::Leaf(_) => false,
        Node::Split { axis, children } => {
            // Same-axis split of a direct child: insert a sibling rather than
            // nesting, so repeated splits stay flat like tmux's.
            if *axis == side.axis()
                && let Some(i) = children.iter().position(|(c, _)| *c == Node::Leaf(target))
            {
                let weight = children[i].1;
                children[i].1 = weight * (1.0 - share);
                let at = if side.forward() { i + 1 } else { i };
                children.insert(at, (Node::Leaf(new), weight * share));
                return true;
            }
            children
                .iter_mut()
                .any(|(c, _)| split_node(c, target, new, side, share))
        }
    }
}

fn remove_node(node: &mut Node, pane: PaneId) -> bool {
    let Node::Split { children, .. } = node else {
        return false;
    };
    if let Some(i) = children.iter().position(|(c, _)| *c == Node::Leaf(pane)) {
        children.remove(i);
        return true;
    }
    children.iter_mut().any(|(c, _)| remove_node(c, pane))
}

/// Collapses single-child splits and merges same-axis nesting.
fn normalize(node: &mut Node) {
    if let Node::Split { children, .. } = node {
        children.iter_mut().for_each(|(c, _)| normalize(c));
    }
    let Node::Split { axis, children } = node else {
        return;
    };
    if children.len() == 1 {
        *node = children.pop().unwrap().0;
        return;
    }
    let axis = *axis;
    let mut flat = Vec::with_capacity(children.len());
    for (child, weight) in children.drain(..) {
        match child {
            Node::Split {
                axis: a,
                children: grand,
            } if a == axis => {
                let total: f32 = grand.iter().map(|(_, w)| w).sum();
                flat.extend(grand.into_iter().map(|(g, w)| (g, weight * w / total)));
            }
            other => flat.push((other, weight)),
        }
    }
    *children = flat;
}

/// Splits `area` among `children` by weight, leaving one cell per border.
fn split_rects(axis: Axis, children: &[(Node, f32)], area: Rect) -> Vec<Rect> {
    let n = (children.len() as u16).max(1);
    let total_len = match axis {
        Axis::Horizontal => area.width,
        Axis::Vertical => area.height,
    };
    let usable = total_len.saturating_sub(n - 1);
    let total_weight: f32 = children.iter().map(|(_, w)| w.max(0.0001)).sum();
    let mut sizes: Vec<u16> = children
        .iter()
        .map(|(_, w)| ((w.max(0.0001) / total_weight) * f32::from(usable)).floor() as u16)
        .map(|s| s.max(1))
        .collect();
    // Hand rounding leftovers to the last child keeps the layout stable.
    let used: u16 = sizes.iter().sum();
    if let Some(last) = sizes.last_mut() {
        *last = (*last + usable.saturating_sub(used)).max(1);
    }
    let mut pos = match axis {
        Axis::Horizontal => area.x,
        Axis::Vertical => area.y,
    };
    let end = pos + total_len;
    sizes
        .into_iter()
        .map(|size| {
            // When the area is too small for every child, the ones that don't
            // fit get an empty rect at the edge instead of spilling outside.
            let start = pos.min(end);
            let size = size.min(end - start);
            let r = match axis {
                Axis::Horizontal => Rect::new(start, area.y, size, area.height),
                Axis::Vertical => Rect::new(area.x, start, area.width, size),
            };
            pos = pos.saturating_add(size + 1);
            r
        })
        .collect()
}

fn compute_node(
    node: &Node,
    area: Rect,
    panes: &mut Vec<(PaneId, Rect)>,
    seps: &mut Vec<Separator>,
) {
    match node {
        Node::Leaf(p) => panes.push((*p, area)),
        Node::Split { axis, children } => {
            let rects = split_rects(*axis, children, area);
            for (i, ((child, _), rect)) in children.iter().zip(&rects).enumerate() {
                let sep_inside = match axis {
                    Axis::Horizontal => rect.right() < area.right(),
                    Axis::Vertical => rect.bottom() < area.bottom(),
                };
                if i + 1 < rects.len() && sep_inside && !rect.is_empty() {
                    seps.push(match axis {
                        Axis::Horizontal => Separator {
                            x: rect.right(),
                            y: area.y,
                            len: area.height,
                            vertical: true,
                        },
                        Axis::Vertical => Separator {
                            x: area.x,
                            y: rect.bottom(),
                            len: area.width,
                            vertical: false,
                        },
                    });
                }
                compute_node(child, *rect, panes, seps);
            }
        }
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

fn swap_leaves(node: &mut Node, x: PaneId, y: PaneId) {
    match node {
        Node::Leaf(p) if *p == x => *p = y,
        Node::Leaf(p) if *p == y => *p = x,
        Node::Leaf(_) => {}
        Node::Split { children, .. } => children.iter_mut().for_each(|(c, _)| swap_leaves(c, x, y)),
    }
}

fn overlap(a0: u16, a1: u16, b0: u16, b1: u16) -> u16 {
    a1.min(b1).saturating_sub(a0.max(b0))
}

#[cfg(test)]
mod tests {
    use super::*;

    const AREA: Rect = Rect {
        x: 0,
        y: 0,
        width: 81,
        height: 25,
    };

    fn rects(l: &Layout) -> Vec<(PaneId, Rect)> {
        l.compute(AREA).0
    }

    #[test]
    fn split_right_halves_the_width_with_a_border() {
        let mut l = Layout::new(1);
        assert!(l.split(1, 2, Side::Right, 50));
        assert_eq!(
            rects(&l),
            vec![(1, Rect::new(0, 0, 40, 25)), (2, Rect::new(41, 0, 40, 25))]
        );
        let (_, seps) = l.compute(AREA);
        assert_eq!(
            seps,
            vec![Separator {
                x: 40,
                y: 0,
                len: 25,
                vertical: true
            }]
        );
    }

    #[test]
    fn split_left_and_up_put_the_new_pane_first() {
        let mut l = Layout::new(1);
        l.split(1, 2, Side::Left, 50);
        assert_eq!(l.panes(), vec![2, 1]);
        l.split(1, 3, Side::Up, 50);
        assert_eq!(l.panes(), vec![2, 3, 1]);
    }

    #[test]
    fn same_axis_splits_stay_flat() {
        let mut l = Layout::new(1);
        l.split(1, 2, Side::Right, 50);
        l.split(2, 3, Side::Right, 50);
        let Node::Split { children, .. } = l.root() else {
            panic!()
        };
        assert_eq!(children.len(), 3);
    }

    #[test]
    fn rects_tile_the_area_exactly() {
        let mut l = Layout::new(1);
        l.split(1, 2, Side::Right, 30);
        l.split(2, 3, Side::Down, 50);
        l.split(1, 4, Side::Down, 70);
        let (panes, seps) = l.compute(AREA);
        let pane_cells: u32 = panes.iter().map(|(_, r)| r.area()).sum();
        let sep_cells: u32 = seps.iter().map(|s| u32::from(s.len)).sum();
        // Every cell is either a pane or a border; borders don't overlap here.
        assert_eq!(pane_cells + sep_cells, AREA.area());
    }

    #[test]
    fn remove_gives_space_back_and_collapses() {
        let mut l = Layout::new(1);
        l.split(1, 2, Side::Right, 50);
        l.split(2, 3, Side::Down, 50);
        assert!(l.remove(3));
        assert_eq!(rects(&l)[1], (2, Rect::new(41, 0, 40, 25)));
        assert!(l.remove(2));
        assert_eq!(l.root(), &Node::Leaf(1));
        assert!(!l.remove(1), "the last pane stays");
    }

    #[test]
    fn neighbor_follows_geometry() {
        let mut l = Layout::new(1);
        l.split(1, 2, Side::Right, 50);
        l.split(2, 3, Side::Down, 50);
        assert_eq!(l.neighbor(1, Side::Right, AREA), Some(2));
        assert_eq!(l.neighbor(3, Side::Left, AREA), Some(1));
        assert_eq!(l.neighbor(2, Side::Down, AREA), Some(3));
        assert_eq!(l.neighbor(3, Side::Up, AREA), Some(2));
        assert_eq!(l.neighbor(1, Side::Left, AREA), None);
    }

    #[test]
    fn resize_moves_the_border_in_the_arrow_direction() {
        let mut l = Layout::new(1);
        l.split(1, 2, Side::Right, 50);
        assert!(l.resize(1, Side::Right, 5, AREA));
        assert_eq!(rects(&l)[0].1.width, 45);
        // Pane 2 has no right neighbour: its left border moves right instead.
        assert!(l.resize(2, Side::Right, 5, AREA));
        assert_eq!(rects(&l)[0].1.width, 50);
        assert!(l.resize(2, Side::Left, 10, AREA));
        assert_eq!(rects(&l)[0].1.width, 40);
        assert!(!l.resize(1, Side::Up, 1, AREA), "no vertical split to move");
    }

    #[test]
    fn resize_never_collapses_a_pane() {
        let mut l = Layout::new(1);
        l.split(1, 2, Side::Right, 50);
        l.resize(1, Side::Right, 500, AREA);
        assert_eq!(rects(&l)[1].1.width, 1);
    }

    #[test]
    fn tiny_areas_never_produce_rects_outside_the_area() {
        let mut l = Layout::new(1);
        l.split(1, 2, Side::Down, 50);
        l.split(2, 3, Side::Down, 50);
        l.split(3, 4, Side::Right, 50);
        for (w, h) in [(40, 2), (3, 1), (1, 1), (0, 0), (40, 4)] {
            let area = Rect::new(0, 1, w, h);
            let (panes, seps) = l.compute(area);
            for (_, r) in panes {
                assert!(
                    r.is_empty()
                        || area.contains(r.as_position())
                            && r.right() <= area.right()
                            && r.bottom() <= area.bottom(),
                    "{r:?} outside {area:?}"
                );
            }
            for s in seps {
                assert!(area.contains((s.x, s.y).into()), "{s:?} outside {area:?}");
            }
        }
    }

    #[test]
    fn sanitized_drops_unknown_leaves_and_empty_splits() {
        let root = Node::Split {
            axis: Axis::Horizontal,
            children: vec![
                (Node::Leaf(1), 1.0),
                (
                    Node::Split {
                        axis: Axis::Vertical,
                        children: vec![],
                    },
                    1.0,
                ),
                (Node::Leaf(99), f32::NAN),
            ],
        };
        let l = Layout::sanitized(root, |p| p != 99).unwrap();
        assert_eq!(l.root(), &Node::Leaf(1));
        assert!(Layout::sanitized(Node::Leaf(5), |_| false).is_none());
    }

    #[test]
    fn swap_exchanges_positions() {
        let mut l = Layout::new(1);
        l.split(1, 2, Side::Right, 50);
        assert_eq!(l.swap(1, Side::Right, AREA), Some(2));
        assert_eq!(l.panes(), vec![2, 1]);
    }

    #[test]
    fn presets_keep_all_panes() {
        let mut l = Layout::new(1);
        for p in 2..=5 {
            l.split(p - 1, p, Side::Right, 50);
        }
        for name in PRESETS {
            assert!(l.apply_preset(name));
            assert_eq!(l.panes(), vec![1, 2, 3, 4, 5], "{name}");
            assert_eq!(l.compute(AREA).0.len(), 5, "{name}");
        }
        assert!(!l.apply_preset("nope"));
    }

    #[test]
    fn tiled_makes_a_grid() {
        let mut l = Layout::new(1);
        for p in 2..=4 {
            l.split(p - 1, p, Side::Right, 50);
        }
        l.apply_preset("tiled");
        let r = rects(&l);
        assert_eq!(r[0].1.y, r[1].1.y);
        assert!(r[2].1.y > r[0].1.y);
    }

    #[test]
    fn serde_round_trip() {
        let mut l = Layout::new(1);
        l.split(1, 2, Side::Down, 40);
        let json = serde_json::to_string(&l).unwrap();
        assert_eq!(serde_json::from_str::<Layout>(&json).unwrap(), l);
    }
}
