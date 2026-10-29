//! [`Emulator`] backed by the `vt100` crate.

use std::num::NonZeroU16;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

use super::hooks::Hooks;
use super::style::cell_style;
use super::{Emulator, MouseMode};

pub struct Vt100Emulator {
    parser: vt100::Parser<Hooks>,
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

fn non_zero(n: u16) -> NonZeroU16 {
    NonZeroU16::new(n).unwrap_or(NonZeroU16::MIN)
}
