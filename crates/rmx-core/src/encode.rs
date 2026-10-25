//! Turns key events back into the bytes a program in a PTY expects.
//!
//! The client sends logical key events (not raw bytes) so bindings resolve the
//! same on every OS; this is where they become xterm-style input again.
//! Ctrl+Shift+X necessarily becomes Ctrl+X here: legacy encoding can't carry it.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// Encodes `key` for a pane. `application_cursor` is the pane's DECCKM state.
/// Returns `None` for keys that have no byte representation.
pub fn encode_key(key: &KeyEvent, application_cursor: bool) -> Option<Vec<u8>> {
    let mods = key.modifiers;
    let ctrl = mods.contains(KeyModifiers::CONTROL);
    let alt = mods.contains(KeyModifiers::ALT);
    // xterm modifier parameter: 1 + shift + 2*alt + 4*ctrl.
    let param =
        1 + u8::from(mods.contains(KeyModifiers::SHIFT)) + 2 * u8::from(alt) + 4 * u8::from(ctrl);

    let mut out = Vec::new();
    let esc_prefix = |out: &mut Vec<u8>| {
        if alt {
            out.push(0x1b);
        }
    };

    match key.code {
        KeyCode::Char(c) => {
            esc_prefix(&mut out);
            // Kitty-protocol terminals report Alt+Shift+a as `a` with SHIFT.
            let c = if mods.contains(KeyModifiers::SHIFT) && c.is_lowercase() {
                c.to_uppercase().next().unwrap_or(c)
            } else {
                c
            };
            match ctrl.then(|| ctrl_byte(c)).flatten() {
                Some(b) => out.push(b),
                None => out.extend_from_slice(c.encode_utf8(&mut [0; 4]).as_bytes()),
            }
        }
        KeyCode::Enter => {
            esc_prefix(&mut out);
            out.push(b'\r');
        }
        KeyCode::Backspace => {
            esc_prefix(&mut out);
            out.push(if ctrl { 0x08 } else { 0x7f });
        }
        KeyCode::Tab => {
            esc_prefix(&mut out);
            out.push(b'\t');
        }
        KeyCode::BackTab => out.extend_from_slice(b"\x1b[Z"),
        KeyCode::Esc => {
            esc_prefix(&mut out);
            out.push(0x1b);
        }
        KeyCode::Up => cursor_key(&mut out, b'A', param, application_cursor),
        KeyCode::Down => cursor_key(&mut out, b'B', param, application_cursor),
        KeyCode::Right => cursor_key(&mut out, b'C', param, application_cursor),
        KeyCode::Left => cursor_key(&mut out, b'D', param, application_cursor),
        KeyCode::Home => cursor_key(&mut out, b'H', param, application_cursor),
        KeyCode::End => cursor_key(&mut out, b'F', param, application_cursor),
        KeyCode::Insert => tilde_key(&mut out, 2, param),
        KeyCode::Delete => tilde_key(&mut out, 3, param),
        KeyCode::PageUp => tilde_key(&mut out, 5, param),
        KeyCode::PageDown => tilde_key(&mut out, 6, param),
        KeyCode::F(n @ 1..=4) => {
            let letter = b"PQRS"[usize::from(n - 1)];
            if param > 1 {
                out.extend_from_slice(format!("\x1b[1;{param}").as_bytes());
                out.push(letter);
            } else {
                out.extend_from_slice(&[0x1b, b'O', letter]);
            }
        }
        KeyCode::F(n @ 5..=12) => {
            const CODES: [u8; 8] = [15, 17, 18, 19, 20, 21, 23, 24];
            tilde_key(&mut out, CODES[usize::from(n - 5)], param);
        }
        _ => return None,
    }
    Some(out)
}

/// Wraps pasted text in bracketed-paste markers when the program asked for them.
pub fn encode_paste(text: &str, bracketed: bool) -> Vec<u8> {
    // Terminals send CR for newlines; shells treat a bare LF oddly in raw mode.
    let body = text.replace("\r\n", "\r").replace('\n', "\r");
    if bracketed {
        format!("\x1b[200~{body}\x1b[201~").into_bytes()
    } else {
        body.into_bytes()
    }
}

fn ctrl_byte(c: char) -> Option<u8> {
    match c {
        'a'..='z' | 'A'..='Z' => Some(c.to_ascii_lowercase() as u8 & 0x1f),
        ' ' | '@' | '2' => Some(0x00),
        '[' | '3' => Some(0x1b),
        '\\' | '4' => Some(0x1c),
        ']' | '5' => Some(0x1d),
        '^' | '6' => Some(0x1e),
        '_' | '-' | '/' | '7' => Some(0x1f),
        '?' | '8' => Some(0x7f),
        _ => None,
    }
}

fn cursor_key(out: &mut Vec<u8>, letter: u8, param: u8, application_cursor: bool) {
    if param > 1 {
        out.extend_from_slice(format!("\x1b[1;{param}").as_bytes());
    } else if application_cursor {
        out.extend_from_slice(b"\x1bO");
    } else {
        out.extend_from_slice(b"\x1b[");
    }
    out.push(letter);
}

fn tilde_key(out: &mut Vec<u8>, code: u8, param: u8) {
    if param > 1 {
        out.extend_from_slice(format!("\x1b[{code};{param}~").as_bytes());
    } else {
        out.extend_from_slice(format!("\x1b[{code}~").as_bytes());
    }
}

#[cfg(test)]
mod tests {
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
}
