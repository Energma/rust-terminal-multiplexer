use super::clipboard::base64;
use super::input::{cap_len, normalize_key};

#[test]
fn caps_paste_on_a_char_boundary() {
    // "ab€" is 5 bytes; caps that land inside a "€" back off to its start.
    assert_eq!(cap_len("ab€".repeat(4), 6), "ab€a");
    assert_eq!(cap_len("ab€".repeat(4), 8), "ab€ab");
}

#[test]
fn drops_key_releases() {
    use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
    let mut k = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE);
    assert!(normalize_key(k).is_some());
    k.kind = KeyEventKind::Release;
    assert!(normalize_key(k).is_none());
}

#[test]
fn base64_matches_rfc4648_vectors() {
    for (input, want) in [
        ("", ""),
        ("f", "Zg=="),
        ("fo", "Zm8="),
        ("foo", "Zm9v"),
        ("foobar", "Zm9vYmFy"),
    ] {
        assert_eq!(base64(input.as_bytes()), want);
    }
}
