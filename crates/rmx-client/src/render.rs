//! Frame rendering: applies the server's cell diffs to the client's screen
//! buffer and draws it.

use std::io::Stdout;

use anyhow::Result;
use crossterm::terminal::{BeginSynchronizedUpdate, EndSynchronizedUpdate, SetTitle};
use crossterm::{execute, queue};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::buffer::{Buffer, Cell};
use ratatui::layout::Rect;
use ratatui::widgets::Widget;
use rmx_proto::Frame;

/// Applies `frame` to `screen` (and the window title) and draws the result.
pub(super) fn paint(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    screen: &mut Buffer,
    title: &mut String,
    frame: Frame,
) -> Result<()> {
    let area = Rect::new(0, 0, frame.cols, frame.rows);
    if frame.full || screen.area != area {
        *screen = Buffer::empty(area);
    }
    for (x, y, cell) in &frame.cells {
        if let Some(dst) = screen.cell_mut((*x, *y)) {
            *dst = Cell::from(cell);
        }
    }
    if frame.title != *title {
        *title = frame.title;
        execute!(terminal.backend_mut(), SetTitle(format!("{title} — rmx")))?;
    }
    // Synchronized output stops terminals showing half-drawn
    // frames; terminals without support ignore it.
    queue!(terminal.backend_mut(), BeginSynchronizedUpdate)?;
    terminal.draw(|f| {
        f.render_widget(Blit(screen), f.area());
        if let Some(pos) = frame.cursor {
            f.set_cursor_position(pos);
        }
    })?;
    execute!(terminal.backend_mut(), EndSynchronizedUpdate)?;
    Ok(())
}

/// Copies the client's screen buffer into ratatui's frame.
struct Blit<'a>(&'a Buffer);

impl Widget for Blit<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let area = area.intersection(self.0.area);
        for y in area.top()..area.bottom() {
            for x in area.left()..area.right() {
                buf[(x, y)] = self.0[(x, y)].clone();
            }
        }
    }
}
