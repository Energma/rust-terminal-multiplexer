use super::*;
use ratatui::layout::Rect;

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
