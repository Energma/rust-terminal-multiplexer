use super::*;

#[test]
fn normal_passthrough_and_direct_keys() {
    let km = map(&[]);
    let mut e = KeyEngine::new();
    let t = Instant::now();
    assert_eq!(e.feed(&km, &ev("a"), t), KeyOutcome::PassThrough);
    assert_eq!(e.feed(&km, &ev("ctrl+c"), t), KeyOutcome::PassThrough);
    assert_eq!(
        e.feed(&km, &ev("alt+3"), t),
        KeyOutcome::Action(Action::GoToTab { index: 3 })
    );
    let mut release = ev("alt+3");
    release.kind = KeyEventKind::Release;
    assert_eq!(e.feed(&km, &release, t), KeyOutcome::Ignored);
}

#[test]
fn sticky_resize_mode_stays_until_exit() {
    let km = map(&[]);
    let mut e = KeyEngine::new();
    let t = Instant::now();
    e.feed(&km, &ev("ctrl+space"), t);
    assert!(matches!(
        e.feed(&km, &ev("r"), t),
        KeyOutcome::Action(Action::EnterMode { .. })
    ));
    assert_eq!(e.mode(), "resize");
    let resize_left = Action::ResizePane {
        direction: Dir::Left,
        amount: 2,
    };
    for _ in 0..3 {
        assert_eq!(
            e.feed(&km, &ev("left"), t),
            KeyOutcome::Action(resize_left.clone())
        );
    }
    assert_eq!(e.feed(&km, &ev("a"), t), KeyOutcome::Ignored);
    assert!(
        !e.tick(&km, t + Duration::from_secs(60)),
        "sticky modes don't time out"
    );
    assert_eq!(e.mode(), "resize");
    assert_eq!(
        e.feed(&km, &ev("esc"), t),
        KeyOutcome::Action(Action::ExitMode)
    );
    assert_eq!(e.mode(), "normal");
}

#[test]
fn locked_passes_everything_but_unlock() {
    let km = map(&[]);
    let mut e = KeyEngine::new();
    let t = Instant::now();
    e.feed(&km, &ev("ctrl+space"), t);
    assert_eq!(e.feed(&km, &ev("l"), t), KeyOutcome::Action(Action::Lock));
    assert_eq!(e.mode(), "locked");
    for k in ["a", "f10", "alt+1", "esc"] {
        assert_eq!(e.feed(&km, &ev(k), t), KeyOutcome::PassThrough, "{k}");
    }
    assert_eq!(
        e.feed(&km, &ev("ctrl+space"), t),
        KeyOutcome::Action(Action::ExitMode)
    );
    assert_eq!(e.mode(), "normal");
}

#[test]
fn select_mode_entered_by_feed() {
    let km = map(&[]);
    let mut e = KeyEngine::new();
    let t = Instant::now();
    e.feed(&km, &ev("ctrl+space"), t);
    assert_eq!(
        e.feed(&km, &ev("["), t),
        KeyOutcome::Action(Action::SelectMode)
    );
    assert_eq!(e.mode(), "select");
    assert_eq!(
        e.feed(&km, &ev("ctrl+c"), t),
        KeyOutcome::Action(Action::CopyAndExit)
    );
    // The server exits select mode after copying.
    e.exit_mode();
    assert_eq!(e.mode(), "normal");
}

#[test]
fn multi_chord_normal_sequence() {
    let km = map(&[("normal", "ctrl+a x", Some(Action::Quit))]);
    let mut e = KeyEngine::new();
    let t = Instant::now();
    let out = e.feed(&km, &ev("ctrl+a"), t);
    assert_eq!(
        out,
        KeyOutcome::Pending {
            hints: vec![("X".into(), "Quit".into())]
        }
    );
    assert_eq!(e.feed(&km, &ev("x"), t), KeyOutcome::Action(Action::Quit));
    // A wrong second key passes through (only that key).
    e.feed(&km, &ev("ctrl+a"), t);
    assert_eq!(e.feed(&km, &ev("y"), t), KeyOutcome::PassThrough);
    assert!(!e.is_pending());
    // And a pending prefix times out.
    e.feed(&km, &ev("ctrl+a"), t);
    assert!(e.tick(&km, t + Duration::from_secs(5)));
    assert!(!e.is_pending());
}

#[test]
fn user_mode_non_sticky_returns_after_one_action() {
    let km = map(&[
        (
            "leader",
            "g",
            Some(Action::EnterMode {
                mode: "git".into(),
                sticky: false,
            }),
        ),
        ("git", "s", Some(Action::Quit)),
    ]);
    let mut e = KeyEngine::new();
    let t = Instant::now();
    e.feed(&km, &ev("ctrl+space"), t);
    e.feed(&km, &ev("g"), t);
    assert_eq!(e.mode(), "git");
    assert_eq!(e.feed(&km, &ev("s"), t), KeyOutcome::Action(Action::Quit));
    assert_eq!(e.mode(), "normal");
}
