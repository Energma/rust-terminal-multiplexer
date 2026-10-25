//! Encodes mouse events for programs that asked for them (vim, htop, …).

use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use rmx_core::emulator::MouseMode;

/// Encodes `ev` at pane-relative `(col, row)`, or `None` if the program's
/// mouse mode doesn't want this kind of event.
pub fn encode(ev: &MouseEvent, col: u16, row: u16, mode: MouseMode, sgr: bool) -> Option<Vec<u8>> {
    let button_code = |b: MouseButton| match b {
        MouseButton::Left => 0,
        MouseButton::Middle => 1,
        MouseButton::Right => 2,
    };
    let (mut code, release) = match ev.kind {
        MouseEventKind::Down(b) => (button_code(b), false),
        MouseEventKind::Up(b) if mode != MouseMode::Press => (button_code(b), true),
        MouseEventKind::Drag(b)
            if matches!(mode, MouseMode::ButtonMotion | MouseMode::AnyMotion) =>
        {
            (button_code(b) + 32, false)
        }
        MouseEventKind::Moved if mode == MouseMode::AnyMotion => (35, false),
        MouseEventKind::ScrollUp => (64, false),
        MouseEventKind::ScrollDown => (65, false),
        MouseEventKind::ScrollLeft => (66, false),
        MouseEventKind::ScrollRight => (67, false),
        _ => return None,
    };
    if mode == MouseMode::None {
        return None;
    }
    if ev.modifiers.contains(KeyModifiers::SHIFT) {
        code += 4;
    }
    if ev.modifiers.contains(KeyModifiers::ALT) {
        code += 8;
    }
    if ev.modifiers.contains(KeyModifiers::CONTROL) {
        code += 16;
    }
    let (x, y) = (u32::from(col) + 1, u32::from(row) + 1);
    if sgr {
        let end = if release { 'm' } else { 'M' };
        return Some(format!("\x1b[<{code};{x};{y}{end}").into_bytes());
    }
    // Legacy X10 encoding: one byte per value, so coordinates cap at 223.
    let code = if release { 3 } else { code };
    let byte = |v: u32| (32 + v.min(223)) as u8;
    Some(vec![0x1b, b'[', b'M', byte(code), byte(x), byte(y)])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(kind: MouseEventKind) -> MouseEvent {
        MouseEvent {
            kind,
            column: 0,
            row: 0,
            modifiers: KeyModifiers::NONE,
        }
    }

    #[test]
    fn sgr_press_and_release() {
        let down = ev(MouseEventKind::Down(MouseButton::Left));
        assert_eq!(
            encode(&down, 4, 9, MouseMode::PressRelease, true).unwrap(),
            b"\x1b[<0;5;10M"
        );
        let up = ev(MouseEventKind::Up(MouseButton::Left));
        assert_eq!(
            encode(&up, 4, 9, MouseMode::PressRelease, true).unwrap(),
            b"\x1b[<0;5;10m"
        );
    }

    #[test]
    fn respects_the_requested_mode() {
        let up = ev(MouseEventKind::Up(MouseButton::Left));
        assert_eq!(encode(&up, 0, 0, MouseMode::Press, true), None);
        let drag = ev(MouseEventKind::Drag(MouseButton::Left));
        assert_eq!(encode(&drag, 0, 0, MouseMode::PressRelease, true), None);
        assert!(encode(&drag, 0, 0, MouseMode::ButtonMotion, true).is_some());
        assert_eq!(
            encode(&ev(MouseEventKind::ScrollUp), 0, 0, MouseMode::None, true),
            None
        );
    }

    #[test]
    fn legacy_encoding() {
        let wheel = ev(MouseEventKind::ScrollDown);
        assert_eq!(
            encode(&wheel, 0, 0, MouseMode::Press, false).unwrap(),
            vec![0x1b, b'[', b'M', 32 + 65, 33, 33]
        );
    }
}
