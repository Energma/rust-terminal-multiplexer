//! Terminal emulation for one pane.
//!
//! The [`Emulator`] trait exists so we can swap `vt100` for a fuller emulator
//! (e.g. `alacritty_terminal`, which reflows on resize) without touching the
//! server. See docs/PLAN.md §4.3.

use std::num::NonZeroU16;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};

pub trait Emulator: Send {
    /// Feeds bytes read from the PTY.
    fn process(&mut self, bytes: &[u8]);
    /// Bytes the emulator wants written back to the PTY (query replies).
    fn take_replies(&mut self) -> Vec<u8>;
    fn resize(&mut self, rows: u16, cols: u16);
    fn title(&self) -> &str;
    /// Cursor as `(row, col)`, or `None` when the program hid it.
    fn cursor(&self) -> Option<(u16, u16)>;
    fn application_cursor(&self) -> bool;
    fn bracketed_paste(&self) -> bool;
    /// Plain-text screen contents, for tests and `capture-pane` style commands.
    fn contents(&self) -> String;
    /// Draws the visible screen into `area` of `buf`.
    fn render(&self, area: Rect, buf: &mut Buffer);

    /// Lines of history above the screen.
    fn history_len(&self) -> usize;
    /// How many lines the view is scrolled back (0 = live screen).
    fn view_offset(&self) -> usize;
    fn set_view_offset(&mut self, lines: usize);
    /// Text of one line, addressed from the top of history (`0` = oldest),
    /// so selections and search survive scrolling.
    fn line_text(&mut self, line: usize) -> String;
    fn rows(&self) -> u16;
    fn cols(&self) -> u16;
    /// Lines discarded from the top of a full history so far. Positions
    /// counted from the top of history shift by this much as output arrives.
    fn lines_dropped(&self) -> u64;
    fn alternate_screen(&self) -> bool;
    fn mouse_mode(&self) -> MouseMode;
    /// Whether the program asked for SGR (1006) mouse encoding.
    fn mouse_sgr(&self) -> bool;
    /// Working directory reported by the shell via OSC 7, if any.
    fn cwd(&self) -> Option<&str>;
}

/// Which mouse events the program in a pane asked to receive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseMode {
    None,
    Press,
    PressRelease,
    ButtonMotion,
    AnyMotion,
}

pub struct Vt100Emulator {
    parser: vt100::Parser<Hooks>,
}

/// Receives the sequences vt100 does not handle itself.
#[derive(Default)]
struct Hooks {
    replies: Vec<u8>,
    title: String,
    cwd: Option<String>,
    dropped: u64,
}

impl vt100::Callbacks for Hooks {
    fn on_scroll(&mut self, _: vt100::capture::RowContents<'_>, alternate_screen: bool) {
        if !alternate_screen {
            self.dropped += 1;
        }
    }

    fn set_window_title(&mut self, _: &mut vt100::Screen, title: &[u8]) {
        self.title = String::from_utf8_lossy(title).into_owned();
    }

    // OSC 7 (`file://host/path`) is how shells report their directory; it
    // lets new panes and restored sessions open where the user was.
    fn unhandled_osc(&mut self, _: &mut vt100::Screen, params: &[&[u8]]) {
        if let [b"7", url, ..] = params
            && let Some(rest) = std::str::from_utf8(url)
                .ok()
                .and_then(|u| u.strip_prefix("file://"))
        {
            let path = rest.find('/').map_or("", |i| &rest[i..]);
            if !path.is_empty() {
                self.cwd = Some(percent_decode(path));
            }
        }
    }

    // ConPTY sends a cursor position request (DSR 6) on startup and blocks the
    // shell until it is answered, and many TUIs probe with DA. vt100 doesn't
    // reply to either, so we do.
    fn unhandled_csi(
        &mut self,
        screen: &mut vt100::Screen,
        i1: Option<u8>,
        _i2: Option<u8>,
        params: &[&[u16]],
        c: char,
    ) {
        let first = params.first().and_then(|p| p.first()).copied().unwrap_or(0);
        match (i1, c, first) {
            (None, 'n', 6) => {
                let (row, col) = screen.cursor_position();
                self.replies
                    .extend_from_slice(format!("\x1b[{};{}R", row + 1, col + 1).as_bytes());
            }
            (None, 'n', 5) => self.replies.extend_from_slice(b"\x1b[0n"),
            // VT100 with advanced video option: the answer most emulators give.
            (None, 'c', 0) => self.replies.extend_from_slice(b"\x1b[?1;2c"),
            (Some(b'>'), 'c', 0) => self.replies.extend_from_slice(b"\x1b[>0;0;0c"),
            _ => {}
        }
    }
}

impl Vt100Emulator {
    pub fn new(rows: u16, cols: u16, scrollback: usize) -> Self {
        Self {
            parser: vt100::Parser::new_with_callbacks(
                non_zero(rows),
                non_zero(cols),
                scrollback,
                Hooks::default(),
            ),
        }
    }
}

impl Emulator for Vt100Emulator {
    fn process(&mut self, bytes: &[u8]) {
        self.parser.process(bytes);
    }

    fn take_replies(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.parser.callbacks_mut().replies)
    }

    fn resize(&mut self, rows: u16, cols: u16) {
        self.parser.set_size(non_zero(rows), non_zero(cols));
    }

    fn title(&self) -> &str {
        &self.parser.callbacks().title
    }

    fn cursor(&self) -> Option<(u16, u16)> {
        let screen = self.parser.screen();
        (!screen.hide_cursor() && screen.scrollback() == 0).then(|| screen.cursor_position())
    }

    fn application_cursor(&self) -> bool {
        self.parser.screen().application_cursor()
    }

    fn bracketed_paste(&self) -> bool {
        self.parser.screen().bracketed_paste()
    }

    fn contents(&self) -> String {
        self.parser.screen().contents()
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        let screen = self.parser.screen();
        let (rows, cols) = screen.size();
        let height = area.height.min(rows.get());
        let width = area.width.min(cols.get());
        for row in 0..height {
            for col in 0..width {
                let Some(src) = screen.cell(row, col) else {
                    continue;
                };
                let Some(dst) = buf.cell_mut((area.x + col, area.y + row)) else {
                    continue;
                };
                dst.reset();
                if src.is_wide_continuation() {
                    // Left blank: ratatui's diff skips columns covered by the
                    // wide glyph to the left, so this is never drawn.
                    continue;
                }
                let symbol = src.contents();
                dst.set_symbol(if symbol.is_empty() { " " } else { symbol });
                dst.set_style(cell_style(src));
            }
        }
    }

    fn history_len(&self) -> usize {
        self.parser.screen().scrollback_len()
    }

    fn view_offset(&self) -> usize {
        self.parser.screen().scrollback()
    }

    fn set_view_offset(&mut self, lines: usize) {
        self.parser.screen_mut().set_scrollback(lines);
    }

    fn line_text(&mut self, line: usize) -> String {
        let history = self.history_len();
        let saved = self.view_offset();
        // Scroll so `line` is on screen, read it, then restore the view.
        let offset = history.saturating_sub(line);
        self.set_view_offset(offset);
        let row = line + offset - history;
        let text = if row < usize::from(self.rows()) {
            self.read_row(row as u16)
        } else {
            String::new()
        };
        self.set_view_offset(saved);
        text
    }

    fn rows(&self) -> u16 {
        self.parser.screen().size().0.get()
    }

    fn cols(&self) -> u16 {
        self.parser.screen().size().1.get()
    }

    fn lines_dropped(&self) -> u64 {
        self.parser.callbacks().dropped
    }

    fn alternate_screen(&self) -> bool {
        self.parser.screen().alternate_screen()
    }

    fn mouse_mode(&self) -> MouseMode {
        match self.parser.screen().mouse_protocol_mode() {
            vt100::MouseProtocolMode::None => MouseMode::None,
            vt100::MouseProtocolMode::Press => MouseMode::Press,
            vt100::MouseProtocolMode::PressRelease => MouseMode::PressRelease,
            vt100::MouseProtocolMode::ButtonMotion => MouseMode::ButtonMotion,
            vt100::MouseProtocolMode::AnyMotion => MouseMode::AnyMotion,
        }
    }

    fn mouse_sgr(&self) -> bool {
        self.parser.screen().mouse_protocol_encoding() == vt100::MouseProtocolEncoding::Sgr
    }

    fn cwd(&self) -> Option<&str> {
        self.parser.callbacks().cwd.as_deref()
    }
}

impl Vt100Emulator {
    fn read_row(&self, row: u16) -> String {
        let cols = self.parser.screen().size().1.get();
        self.parser
            .screen()
            .rows(0, cols)
            .nth(usize::from(row))
            .unwrap_or_default()
    }
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && let Some(b) = s
                .get(i + 1..i + 3)
                .and_then(|h| u8::from_str_radix(h, 16).ok())
        {
            out.push(b);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn cell_style(cell: &vt100::Cell) -> Style {
    let mut modifier = Modifier::empty();
    for (on, m) in [
        (cell.bold(), Modifier::BOLD),
        (cell.dim(), Modifier::DIM),
        (cell.italic(), Modifier::ITALIC),
        (cell.underline(), Modifier::UNDERLINED),
        (cell.inverse(), Modifier::REVERSED),
    ] {
        if on {
            modifier |= m;
        }
    }
    Style::default()
        .fg(color(cell.fgcolor()))
        .bg(color(cell.bgcolor()))
        .add_modifier(modifier)
}

fn color(c: vt100::Color) -> Color {
    match c {
        vt100::Color::Default => Color::Reset,
        vt100::Color::Idx(i) => Color::Indexed(i),
        vt100::Color::Rgb(r, g, b) => Color::Rgb(r, g, b),
    }
}

fn non_zero(n: u16) -> NonZeroU16 {
    NonZeroU16::new(n).unwrap_or(NonZeroU16::MIN)
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
