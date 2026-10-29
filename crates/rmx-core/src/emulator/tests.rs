use super::*;
use ratatui::style::{Color, Modifier};

#[test]
fn replies_to_cursor_position_request() {
    let mut emu = Vt100Emulator::new(24, 80, 0);
    emu.process(b"ab\r\ncd\x1b[6n");
    assert_eq!(emu.take_replies(), b"\x1b[2;3R");
    assert!(emu.take_replies().is_empty(), "replies are drained");
}

#[test]
fn replies_to_device_attributes() {
    let mut emu = Vt100Emulator::new(24, 80, 0);
    emu.process(b"\x1b[c\x1b[>c");
    assert_eq!(emu.take_replies(), b"\x1b[?1;2c\x1b[>0;0;0c");
}

#[test]
fn captures_osc_title() {
    let mut emu = Vt100Emulator::new(24, 80, 0);
    emu.process(b"\x1b]2;my title\x07");
    assert_eq!(emu.title(), "my title");
}

#[test]
fn renders_text_colors_and_attributes() {
    let mut emu = Vt100Emulator::new(2, 4, 0);
    emu.process(b"\x1b[1;31mA\x1b[0m\x1b[38;2;1;2;3mB");
    let area = Rect::new(0, 0, 4, 2);
    let mut buf = Buffer::empty(area);
    emu.render(area, &mut buf);

    let a = &buf[(0, 0)];
    assert_eq!(a.symbol(), "A");
    assert_eq!(a.fg, Color::Indexed(1));
    assert!(a.modifier.contains(Modifier::BOLD));
    let b = &buf[(1, 0)];
    assert_eq!(b.fg, Color::Rgb(1, 2, 3));
    assert!(!b.modifier.contains(Modifier::BOLD));
    assert_eq!(buf[(2, 0)].symbol(), " ");
}

#[test]
fn wide_chars_leave_their_continuation_cell_blank() {
    let mut emu = Vt100Emulator::new(1, 4, 0);
    emu.process("界x".as_bytes());
    let area = Rect::new(0, 0, 4, 1);
    let mut buf = Buffer::empty(area);
    emu.render(area, &mut buf);
    assert_eq!(buf[(0, 0)].symbol(), "界");
    assert_eq!(buf[(1, 0)].symbol(), " ");
    assert_eq!(buf[(2, 0)].symbol(), "x");
}

#[test]
fn scrollback_lines_are_addressable_from_the_top() {
    let mut emu = Vt100Emulator::new(2, 10, 100);
    emu.process(b"l0\r\nl1\r\nl2\r\nl3");
    assert_eq!(emu.history_len(), 2);
    let lines: Vec<_> = (0..4).map(|i| emu.line_text(i)).collect();
    assert_eq!(lines, ["l0", "l1", "l2", "l3"]);
    assert_eq!(emu.view_offset(), 0, "reading restores the view");
    emu.set_view_offset(2);
    assert_eq!(emu.cursor(), None, "no cursor while scrolled back");
    let area = Rect::new(0, 0, 10, 2);
    let mut buf = Buffer::empty(area);
    emu.render(area, &mut buf);
    assert_eq!(buf[(1, 0)].symbol(), "0");
}

#[test]
fn counts_lines_dropped_from_a_full_history() {
    let mut emu = Vt100Emulator::new(2, 10, 3);
    emu.process(b"1\r\n2\r\n3\r\n4\r\n5\r\n6\r\n7");
    assert_eq!(emu.history_len(), 3);
    assert_eq!(emu.lines_dropped(), 2);
    assert_eq!(emu.line_text(0), "3", "oldest kept line");
}

#[test]
fn tracks_osc7_cwd_and_mouse_mode() {
    let mut emu = Vt100Emulator::new(5, 5, 0);
    emu.process(b"\x1b]7;file://host/home/me/my%20dir\x07");
    assert_eq!(emu.cwd(), Some("/home/me/my dir"));
    assert_eq!(emu.mouse_mode(), MouseMode::None);
    emu.process(b"\x1b[?1002h\x1b[?1006h");
    assert_eq!(emu.mouse_mode(), MouseMode::ButtonMotion);
    assert!(emu.mouse_sgr());
}

#[test]
fn hidden_cursor_is_none() {
    let mut emu = Vt100Emulator::new(5, 5, 0);
    assert_eq!(emu.cursor(), Some((0, 0)));
    emu.process(b"\x1b[?25l");
    assert_eq!(emu.cursor(), None);
}
