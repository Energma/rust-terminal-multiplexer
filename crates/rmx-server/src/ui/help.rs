//! F1 help: every binding of the effective keymap, grouped by mode, so what
//! users see is always what the current config does.

use crossterm::event::{KeyCode, KeyEvent, MouseEvent, MouseEventKind};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};

use super::{Ctx, Outcome, centered, panel, put};

#[derive(Default)]
pub struct Help {
    scroll: usize,
}

enum Line {
    Heading(String),
    Binding(String, String),
    Text(String),
}

fn lines(ctx: &Ctx) -> Vec<Line> {
    let mut out = vec![
        Line::Text(format!(
            "Leader key: {}   ·   F10 menu   ·   right-click a pane for its menu",
            ctx.keymap.leader().display_short()
        )),
        Line::Text(format!("Settings: {}", ctx.config_path.display())),
    ];
    for mode in ctx.keymap.modes() {
        let bindings = ctx.keymap.bindings(mode);
        if bindings.is_empty() {
            continue;
        }
        out.push(Line::Heading(match mode {
            "normal" => "Anywhere".to_owned(),
            "leader" => format!(
                "After the leader key ({})",
                ctx.keymap.leader().display_short()
            ),
            "select" => "Select / scroll mode".to_owned(),
            "resize" => "Resize mode".to_owned(),
            "locked" => "Locked (all keys go to the pane)".to_owned(),
            other => format!("Mode: {other}"),
        }));
        for b in bindings {
            let keys = b
                .keys
                .iter()
                .map(|k| k.display_short())
                .collect::<Vec<_>>()
                .join(" ");
            out.push(Line::Binding(keys, b.action.label()));
        }
    }
    out
}

impl Help {
    pub fn handle_key(&mut self, key: &KeyEvent) -> Outcome {
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') | KeyCode::F(1) | KeyCode::Enter => {
                return Outcome::CLOSE;
            }
            KeyCode::Up | KeyCode::Char('k') => self.scroll = self.scroll.saturating_sub(1),
            KeyCode::Down | KeyCode::Char('j') => self.scroll += 1,
            KeyCode::PageUp => self.scroll = self.scroll.saturating_sub(15),
            KeyCode::PageDown | KeyCode::Char(' ') => self.scroll += 15,
            KeyCode::Home => self.scroll = 0,
            _ => {}
        }
        Outcome::STAY
    }

    pub fn handle_mouse(&mut self, ev: &MouseEvent) -> Outcome {
        match ev.kind {
            MouseEventKind::ScrollUp => self.scroll = self.scroll.saturating_sub(3),
            MouseEventKind::ScrollDown => self.scroll += 3,
            _ => {}
        }
        Outcome::STAY
    }

    pub fn render(&self, ctx: &Ctx, screen: Rect, buf: &mut Buffer) {
        let theme = ctx.theme;
        let rect = centered(screen, 80, screen.height.saturating_sub(4));
        let inner = panel(buf, rect, "Keyboard Shortcuts — Esc to close", theme);
        let base = Style::default().fg(theme.menu_fg).bg(theme.menu_bg);
        let all = lines(ctx);
        let max_scroll = all.len().saturating_sub(usize::from(inner.height));
        for (row, line) in all
            .iter()
            .skip(self.scroll.min(max_scroll))
            .take(usize::from(inner.height))
            .enumerate()
        {
            let y = inner.y + row as u16;
            match line {
                Line::Heading(h) => put(
                    buf,
                    inner.x + 1,
                    y,
                    h,
                    inner.width,
                    base.fg(theme.accent).add_modifier(Modifier::BOLD),
                ),
                Line::Text(t) => put(
                    buf,
                    inner.x + 1,
                    y,
                    t,
                    inner.width.saturating_sub(2),
                    base.add_modifier(Modifier::DIM),
                ),
                Line::Binding(keys, label) => {
                    put(buf, inner.x + 3, y, keys, 22, base.fg(theme.accent));
                    put(
                        buf,
                        inner.x + 26,
                        y,
                        label,
                        inner.width.saturating_sub(27),
                        base,
                    );
                }
            }
        }
    }
}
