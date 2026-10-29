use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::*;

#[test]
fn fuzzy_prefers_word_starts_and_runs() {
    assert!(
        fuzzy_score("sr", "Split right").unwrap()
            > fuzzy_score("sr", "Close current").unwrap_or(-99)
    );
    assert!(fuzzy_score("zoom", "Toggle pane zoom").is_some());
    assert_eq!(fuzzy_score("xyz", "Split right"), None);
}

#[test]
fn text_input_edits_unicode() {
    let mut t = TextInput::new("héllo");
    t.handle(&KeyEvent::from(KeyCode::Backspace));
    assert_eq!(t.text, "héll");
    t.handle(&KeyEvent::from(KeyCode::Home));
    t.handle(&KeyEvent::from(KeyCode::Char('¡')));
    assert_eq!(t.text, "¡héll");
    t.handle(&KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL));
    assert_eq!(t.text, "");
}
