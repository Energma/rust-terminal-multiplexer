//! Terminal emulation for one pane.
//!
//! The [`Emulator`] trait exists so we can swap `vt100` for a fuller emulator
//! (e.g. `alacritty_terminal`, which reflows on resize) without touching the
//! server. See docs/PLAN.md §4.3.

mod hooks;
mod style;
#[cfg(test)]
mod tests;
mod vt;

pub use vt::Vt100Emulator;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

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
