use super::*;
use crate::action::parse::find;
use crate::diag::Diag;
use toml::de::DeTable;

mod parsing;

fn parse(src: &str) -> Result<Option<Action>, Diag> {
    let doc = format!("x = {src}");
    let t = DeTable::parse(&doc).unwrap();
    let v = find(t.get_ref(), "x").unwrap().clone();
    from_value(&v).map_err(|mut d| {
        // Make spans relative to `src` for easy assertions.
        d.span = d.span.start - 4..d.span.end - 4;
        d
    })
}

fn ok(src: &str) -> Action {
    parse(src).unwrap().unwrap()
}

fn err(src: &str) -> Diag {
    parse(src).unwrap_err()
}

#[test]
fn registry_is_consistent() {
    let mut ids = all_action_ids().to_vec();
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(ids.len(), ACTIONS.len(), "duplicate ids");
    for a in palette_actions() {
        assert!(action_info(a.id()).is_some());
        // Round-trip through the string/table parser.
        assert!(!a.label().is_empty());
    }
    // Every arg-less id parses from a plain string and maps back to its id.
    for id in all_action_ids() {
        if let Ok(Some(a)) = parse(&format!("\"{id}\"")) {
            assert_eq!(a.id(), *id);
        }
    }
}

#[test]
fn labels_include_args() {
    assert_eq!(
        Action::SplitPane {
            direction: Dir::Right,
            size: None,
            command: None
        }
        .label(),
        "Split pane right"
    );
    assert_eq!(Action::GoToTab { index: 3 }.label(), "Go to tab 3");
    assert_eq!(
        Action::ResizePane {
            direction: Dir::Left,
            amount: 2
        }
        .label(),
        "Resize pane left by 2"
    );
    assert_eq!(Action::MoveTab { delta: -1 }.label(), "Move tab left");
    assert_eq!(Action::NewTab.label(), "New tab");
    assert_eq!(Action::NewTab.category(), Category::Tab);
}

#[test]
fn palette_has_defaults_and_no_motions() {
    let p = palette_actions();
    assert!(p.contains(&Action::GoToTab { index: 9 }));
    assert!(p.contains(&Action::ApplyLayout {
        name: "tiled".into()
    }));
    assert!(!p.iter().any(|a| matches!(a, Action::SelectMove { .. })));
    assert!(!p.contains(&Action::CommandPalette));
}
