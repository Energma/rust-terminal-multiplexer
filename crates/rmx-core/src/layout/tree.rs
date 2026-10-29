//! Building and editing the tree: construction, sanitising untrusted input,
//! splitting, removal and the normalisation that keeps it flat.

use super::{Layout, Node, PaneId, Side};

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
