use super::*;

fn enc(code: KeyCode, mods: KeyModifiers) -> Vec<u8> {
    encode_key(&KeyEvent::new(code, mods), false).unwrap()
}

#[test]
fn plain_and_unicode_chars() {
    assert_eq!(enc(KeyCode::Char('a'), KeyModifiers::NONE), b"a");
    assert_eq!(enc(KeyCode::Char('A'), KeyModifiers::SHIFT), b"A");
    assert_eq!(enc(KeyCode::Char('é'), KeyModifiers::NONE), "é".as_bytes());
}

#[test]
fn ctrl_and_alt_chars() {
    assert_eq!(enc(KeyCode::Char('c'), KeyModifiers::CONTROL), [0x03]);
    // Ctrl+Shift+C is indistinguishable from Ctrl+C in legacy encoding.
    assert_eq!(
        enc(
            KeyCode::Char('C'),
            KeyModifiers::CONTROL | KeyModifiers::SHIFT
        ),
        [0x03]
    );
    assert_eq!(enc(KeyCode::Char(' '), KeyModifiers::CONTROL), [0x00]);
    assert_eq!(enc(KeyCode::Char('x'), KeyModifiers::ALT), b"\x1bx");
    assert_eq!(
        enc(
            KeyCode::Char('x'),
            KeyModifiers::ALT | KeyModifiers::CONTROL
        ),
        [0x1b, 0x18]
    );
}

#[test]
fn shift_letters_and_ctrl_slash() {
    assert_eq!(
        enc(KeyCode::Char('a'), KeyModifiers::ALT | KeyModifiers::SHIFT),
        b"\x1bA"
    );
    assert_eq!(enc(KeyCode::Char('/'), KeyModifiers::CONTROL), [0x1f]);
}

#[test]
fn special_keys() {
    assert_eq!(enc(KeyCode::Enter, KeyModifiers::NONE), b"\r");
    assert_eq!(enc(KeyCode::Backspace, KeyModifiers::NONE), [0x7f]);
    assert_eq!(enc(KeyCode::BackTab, KeyModifiers::SHIFT), b"\x1b[Z");
    assert_eq!(enc(KeyCode::Delete, KeyModifiers::NONE), b"\x1b[3~");
    assert_eq!(enc(KeyCode::PageUp, KeyModifiers::CONTROL), b"\x1b[5;5~");
    assert_eq!(enc(KeyCode::F(1), KeyModifiers::NONE), b"\x1bOP");
    assert_eq!(enc(KeyCode::F(5), KeyModifiers::NONE), b"\x1b[15~");
    assert_eq!(enc(KeyCode::F(12), KeyModifiers::SHIFT), b"\x1b[24;2~");
}

#[test]
fn arrows_respect_application_cursor_and_modifiers() {
    let up = KeyEvent::new(KeyCode::Up, KeyModifiers::NONE);
    assert_eq!(encode_key(&up, false).unwrap(), b"\x1b[A");
    assert_eq!(encode_key(&up, true).unwrap(), b"\x1bOA");
    let ctrl_left = KeyEvent::new(KeyCode::Left, KeyModifiers::CONTROL);
    assert_eq!(encode_key(&ctrl_left, true).unwrap(), b"\x1b[1;5D");
}

#[test]
fn paste_brackets_only_when_requested() {
    assert_eq!(encode_paste("a\nb", false), b"a\rb");
    assert_eq!(encode_paste("a\r\nb", true), b"\x1b[200~a\rb\x1b[201~");
}
