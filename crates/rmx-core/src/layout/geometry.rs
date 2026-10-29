//! Turning the tree into screen rectangles and borders, and finding a
//! pane's geometric neighbour.

use ratatui::layout::Rect;

use super::{Axis, Layout, Node, PaneId, Separator, Side};

impl Layout {
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
}

/// Splits `area` among `children` by weight, leaving one cell per border.
pub(super) fn split_rects(axis: Axis, children: &[(Node, f32)], area: Rect) -> Vec<Rect> {
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

fn overlap(a0: u16, a1: u16, b0: u16, b1: u16) -> u16 {
    a1.min(b1).saturating_sub(a0.max(b0))
}
