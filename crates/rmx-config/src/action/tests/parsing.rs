use super::*;

#[test]
fn string_form() {
    assert_eq!(ok("\"new-tab\""), Action::NewTab);
    assert_eq!(parse("\"none\"").unwrap(), None);
    assert_eq!(
        ok("\"split-pane\""),
        Action::SplitPane {
            direction: Dir::Right,
            size: None,
            command: None
        }
    );
    assert_eq!(ok("\"scroll-up\""), Action::ScrollUp { lines: 3 });
}

#[test]
fn table_form_with_defaults() {
    assert_eq!(
        ok(r#"{ action = "split-pane", direction = "down", size = "30%" }"#),
        Action::SplitPane {
            direction: Dir::Down,
            size: Some(30),
            command: None
        }
    );
    assert_eq!(
        ok(r#"{ action = "split-pane", size = 25, command = "htop" }"#),
        Action::SplitPane {
            direction: Dir::Right,
            size: Some(25),
            command: Some("htop".into())
        }
    );
    assert_eq!(
        ok(r#"{ action = "resize-pane", direction = "left" }"#),
        Action::ResizePane {
            direction: Dir::Left,
            amount: 2
        }
    );
    assert_eq!(
        ok(r#"{ action = "go-to-tab", index = 3 }"#),
        Action::GoToTab { index: 3 }
    );
    assert_eq!(
        ok(r#"{ action = "run", command = "lazygit", in = "split-down" }"#),
        Action::Run {
            command: "lazygit".into(),
            r#in: RunIn::SplitDown
        }
    );
    assert_eq!(
        ok(r#"{ action = "enter-mode", mode = "resize" }"#),
        Action::EnterMode {
            mode: "resize".into(),
            sticky: false
        }
    );
}

#[test]
fn mode_shorthand() {
    assert_eq!(
        ok(r#"{ mode = "resize", sticky = true }"#),
        Action::EnterMode {
            mode: "resize".into(),
            sticky: true
        }
    );
}

#[test]
fn unknown_action_suggests() {
    let d = err("\"split-pain\"");
    assert_eq!(d.message, "unknown action \"split-pain\"");
    assert_eq!(d.help.as_deref(), Some("did you mean \"split-pane\"?"));
    assert_eq!(d.span, 0..12);
    let d = err(r#"{ action = "new-tba" }"#);
    assert_eq!(d.help.as_deref(), Some("did you mean \"new-tab\"?"));
    assert_eq!(d.span.start, 11);
}

#[test]
fn argument_errors() {
    let d = err("\"go-to-tab\"");
    assert!(
        d.message.contains("needs argument `index`"),
        "{}",
        d.message
    );
    assert!(d.help.unwrap().contains("index = 1"));

    let d = err(r#"{ action = "move-focus" }"#);
    assert!(d.message.contains("`direction`"));

    let d = err(r#"{ action = "resize-pane", direction = "left", amout = 3 }"#);
    assert!(d.message.contains("unknown argument `amout`"));
    assert_eq!(d.help.as_deref(), Some("did you mean `amount`?"));
    assert_eq!(
        &r#"{ action = "resize-pane", direction = "left", amout = 3 }"#[d.span],
        "amout"
    );

    let d = err(r#"{ action = "new-tab", foo = 1 }"#);
    assert_eq!(d.help.as_deref(), Some("\"new-tab\" takes no arguments"));

    let d = err(r#"{ action = "go-to-tab", index = "3" }"#);
    assert!(
        d.message.contains("must be an integer, found string"),
        "{}",
        d.message
    );

    let d = err(r#"{ action = "go-to-tab", index = 0 }"#);
    assert!(d.message.contains("between 1"));

    let d = err(r#"{ action = "move-focus", direction = "lft" }"#);
    assert_eq!(d.help.as_deref(), Some("did you mean \"left\"?"));

    let d = err(r#"{ action = "split-pane", size = 100 }"#);
    assert!(d.message.contains("1 to 99"));

    let d = err(r#"{ action = "enter-mode", mode = "resize", sticky = "yes" }"#);
    assert!(d.message.contains("true or false"));

    assert!(err("3").message.contains("string or an inline table"));
    assert!(
        err(r#"{ direction = "left" }"#)
            .message
            .contains("missing `action`")
    );
}

#[test]
fn public_parse() {
    assert_eq!(Action::parse("new-tab").unwrap(), Action::NewTab);
    assert_eq!(
        Action::parse(r#"{ action = "move-focus", direction = "up" }"#).unwrap(),
        Action::MoveFocus { direction: Dir::Up }
    );
    assert!(
        Action::parse("split-pain")
            .unwrap_err()
            .contains("split-pane")
    );
    assert!(Action::parse("none").is_err());
}
