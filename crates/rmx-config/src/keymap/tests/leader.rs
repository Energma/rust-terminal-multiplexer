use super::*;

#[test]
fn leader_then_key_fires_and_returns_to_normal() {
    let km = map(&[]);
    let mut e = KeyEngine::new();
    let t = Instant::now();
    let out = e.feed(&km, &ev("ctrl+space"), t);
    let KeyOutcome::Pending { hints } = out else {
        panic!("{out:?}")
    };
    assert!(hints.contains(&("|".into(), "Split pane right".into())));
    assert_eq!(e.mode(), "leader");
    assert!(e.is_pending());
    // Terminals report `|` with Shift; it still matches.
    let pipe = KeyEvent::new(KeyCode::Char('|'), KeyModifiers::SHIFT);
    assert_eq!(e.feed(&km, &pipe, t), KeyOutcome::Action(split_right()));
    assert_eq!(e.mode(), "normal");
    assert!(!e.is_pending());
}

#[test]
fn leader_times_out() {
    let km = map(&[]);
    let mut e = KeyEngine::new();
    let t = Instant::now();
    e.feed(&km, &ev("ctrl+space"), t);
    assert!(!e.tick(&km, t + Duration::from_millis(1999)));
    assert!(e.tick(&km, t + Duration::from_millis(2000)));
    assert_eq!(e.mode(), "normal");
    // A key after the timeout is a plain key again.
    e.feed(&km, &ev("ctrl+space"), t);
    assert_eq!(
        e.feed(&km, &ev("t"), t + Duration::from_secs(3)),
        KeyOutcome::PassThrough
    );
}

#[test]
fn zero_timeout_never_expires() {
    let leader = KeyChord::parse("ctrl+space").unwrap();
    let km = Keymap::build(
        leader,
        0,
        crate::presets::bindings("windows", leader, false),
        [],
    );
    let mut e = KeyEngine::new();
    let t = Instant::now();
    e.feed(&km, &ev("ctrl+space"), t);
    assert!(!e.tick(&km, t + Duration::from_secs(3600)));
    assert_eq!(e.mode(), "leader");
}

#[test]
fn unbound_key_in_leader_is_ignored() {
    let km = map(&[]);
    let mut e = KeyEngine::new();
    let t = Instant::now();
    e.feed(&km, &ev("ctrl+space"), t);
    assert_eq!(e.feed(&km, &ev("f7"), t), KeyOutcome::Ignored);
    assert_eq!(e.mode(), "normal");
}

#[test]
fn leader_twice_sends_leader() {
    let km = map(&[]);
    let mut e = KeyEngine::new();
    let t = Instant::now();
    e.feed(&km, &ev("ctrl+space"), t);
    assert_eq!(e.feed(&km, &ev("ctrl+space"), t), KeyOutcome::PassThrough);
    assert_eq!(e.mode(), "normal");
}

#[test]
fn leader_sequences_in_normal_fold_into_leader_table() {
    let km = map(&[("normal", "ctrl+space q", Some(Action::Quit))]);
    assert!(km.bindings("leader").iter().any(|b| b.keys == seq("q")));
    let mut e = KeyEngine::new();
    let t = Instant::now();
    e.feed(&km, &ev("ctrl+space"), t);
    assert_eq!(e.feed(&km, &ev("q"), t), KeyOutcome::Action(Action::Quit));
}
