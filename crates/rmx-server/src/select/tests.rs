use super::*;
use rmx_core::emulator::Vt100Emulator;

fn emu_with(lines: &[&str], rows: u16) -> Vt100Emulator {
    let mut emu = Vt100Emulator::new(rows, 20, 100);
    emu.process(lines.join("\r\n").as_bytes());
    emu
}

#[test]
fn selects_across_lines_including_history() {
    let mut emu = emu_with(&["alpha one", "beta two", "gamma three", "delta"], 2);
    let mut s = Select::new(0, &emu);
    s.cursor = (0, 6);
    s.start(false);
    s.cursor = (2, 4);
    assert_eq!(s.text(&mut emu).unwrap(), "one\nbeta two\ngamma");
    s.start(true);
    assert_eq!(
        s.text(&mut emu).unwrap(),
        "alpha one\nbeta two\ngamma three"
    );
}

#[test]
fn find_wraps_and_scrolls_into_view() {
    let mut emu = emu_with(&["needle a", "x", "y", "z", "needle b"], 2);
    let mut s = Select::new(0, &emu);
    assert_eq!(s.cursor, (4, 8), "starts at the shell cursor");
    // Backwards (upwards) first finds the match left of the cursor…
    assert!(s.find("NEEDLE", false, &mut emu));
    assert_eq!(s.cursor, (4, 0));
    // …then the one in history.
    assert!(s.find("needle", false, &mut emu));
    assert_eq!(s.cursor, (0, 0));
    assert_eq!(emu.view_offset(), 3, "scrolled back to show line 0");
    assert!(s.find("needle", true, &mut emu));
    assert_eq!(s.cursor, (4, 0));
    assert!(!s.find("absent", true, &mut emu));
    assert!(s.status().unwrap().contains("no match"));
}

#[test]
fn selection_follows_text_when_history_is_full() {
    let mut emu = Vt100Emulator::new(2, 10, 3);
    emu.process(b"a\r\nb\r\nc\r\nd\r\ne");
    let mut s = Select::new(0, &emu);
    s.cursor = (1, 0); // "c"
    s.start(false);
    emu.process(b"\r\nf\r\ng");
    s.rebase(&emu);
    assert_eq!(emu.line_text(s.cursor.0), "c");
    assert_eq!(s.text(&mut emu).unwrap(), "c");
}

#[test]
fn word_motion() {
    let mut emu = emu_with(&["foo bar  baz"], 1);
    let mut s = Select::new(0, &emu);
    s.cursor = (0, 0);
    s.move_word(true, &mut emu);
    assert_eq!(s.cursor.1, 4);
    s.move_word(true, &mut emu);
    assert_eq!(s.cursor.1, 9);
    s.move_word(false, &mut emu);
    assert_eq!(s.cursor.1, 4);
}
