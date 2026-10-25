//! Command palette (Ctrl+Shift+P style): fuzzy search over every action,
//! user `[[commands]]` and workspaces, each shown with its key binding so the
//! palette doubles as the way people discover shortcuts.

use crossterm::event::{KeyCode, KeyEvent, MouseButton, MouseEvent, MouseEventKind};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use rmx_config::action::{Action, palette_actions};

use super::{Cmd, Ctx, Outcome, TextInput, centered, contains, fuzzy_score, panel, put};
use crate::state::WorkspaceId;

enum Entry {
    Action {
        label: String,
        detail: String,
        action: Action,
    },
    Workspace {
        id: WorkspaceId,
        name: String,
    },
}

impl Entry {
    fn text(&self) -> String {
        match self {
            Entry::Action { label, detail, .. } => format!("{detail}: {label}"),
            Entry::Workspace { name, .. } => format!("Workspace: {name}"),
        }
    }
}

#[derive(Default)]
pub struct Palette {
    input: TextInput,
    selected: usize,
}

impl Palette {
    fn entries(&self, ctx: &Ctx) -> Vec<Entry> {
        let mut all: Vec<Entry> = palette_actions()
            .into_iter()
            .map(|action| Entry::Action {
                label: action.label(),
                detail: format!("{:?}", action.category()),
                action,
            })
            .collect();
        all.extend(ctx.commands.iter().map(|c| Entry::Action {
            label: c.name.clone(),
            detail: "Custom".into(),
            action: c.action.clone(),
        }));
        all.extend(ctx.workspaces.iter().map(|w| Entry::Workspace {
            id: w.id,
            name: w.name.clone(),
        }));

        let mut scored: Vec<(i32, usize, Entry)> = all
            .into_iter()
            .enumerate()
            .filter_map(|(i, e)| fuzzy_score(&self.input.text, &e.text()).map(|s| (s, i, e)))
            .collect();
        scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
        scored.into_iter().map(|(_, _, e)| e).collect()
    }

    fn choose(&self, ctx: &Ctx, index: usize) -> Outcome {
        match self.entries(ctx).into_iter().nth(index) {
            Some(Entry::Action { action, .. }) => Outcome::run(action),
            Some(Entry::Workspace { id, .. }) => Outcome::cmd(
                true,
                Cmd::Open {
                    workspace: id,
                    tab: None,
                    pane: None,
                },
            ),
            None => Outcome::STAY,
        }
    }

    pub fn paste(&mut self, text: &str) {
        self.input.insert_str(text);
        self.selected = 0;
    }

    pub fn handle_key(&mut self, key: &KeyEvent, ctx: &Ctx) -> Outcome {
        let count = self.entries(ctx).len();
        match key.code {
            KeyCode::Esc => return Outcome::CLOSE,
            KeyCode::Enter => return self.choose(ctx, self.selected),
            KeyCode::Up => self.selected = self.selected.saturating_sub(1),
            KeyCode::Down | KeyCode::Tab => {
                self.selected = (self.selected + 1).min(count.saturating_sub(1))
            }
            KeyCode::PageUp => self.selected = self.selected.saturating_sub(10),
            KeyCode::PageDown => self.selected = (self.selected + 10).min(count.saturating_sub(1)),
            _ => {
                if self.input.handle(key) {
                    self.selected = 0;
                }
            }
        }
        Outcome::STAY
    }

    fn rect(screen: Rect) -> Rect {
        let r = centered(screen, 72, 20);
        Rect {
            y: screen.y + 2,
            ..r
        }
        .intersection(screen)
    }

    pub fn handle_mouse(&mut self, ev: &MouseEvent, ctx: &Ctx, screen: Rect) -> Outcome {
        let rect = Self::rect(screen);
        // Same geometry as `render`: border, input row, divider, then the list.
        let list = Rect::new(
            rect.x + 1,
            rect.y + 3,
            rect.width.saturating_sub(2),
            rect.height.saturating_sub(4),
        );
        let count = self.entries(ctx).len();
        let scroll = super::list_scroll(
            self.selected.min(count.saturating_sub(1)),
            usize::from(list.height),
        );
        match ev.kind {
            MouseEventKind::Down(MouseButton::Left) if !contains(rect, ev.column, ev.row) => {
                Outcome::CLOSE
            }
            MouseEventKind::Up(MouseButton::Left) if contains(list, ev.column, ev.row) => {
                self.choose(ctx, scroll + usize::from(ev.row - list.y))
            }
            MouseEventKind::ScrollDown => {
                self.selected = (self.selected + 1).min(count.saturating_sub(1));
                Outcome::STAY
            }
            MouseEventKind::ScrollUp => {
                self.selected = self.selected.saturating_sub(1);
                Outcome::STAY
            }
            _ => Outcome::STAY,
        }
    }

    pub fn render(&self, ctx: &Ctx, screen: Rect, buf: &mut Buffer) {
        let theme = ctx.theme;
        let rect = Self::rect(screen);
        let inner = panel(buf, rect, "Command Palette", theme);
        if inner.height < 3 {
            return;
        }
        let base = Style::default().fg(theme.menu_fg).bg(theme.menu_bg);
        put(buf, inner.x + 1, inner.y, "›", 1, base.fg(theme.accent));
        self.input.render(
            buf,
            inner.x + 3,
            inner.y,
            inner.width.saturating_sub(4),
            base,
        );
        put(
            buf,
            inner.x,
            inner.y + 1,
            &"─".repeat(usize::from(inner.width)),
            inner.width,
            base.fg(theme.border),
        );

        let entries = self.entries(ctx);
        let rows = usize::from(inner.height - 2);
        let selected = self.selected.min(entries.len().saturating_sub(1));
        // Keep the selection in view; `scroll` is recomputed each frame.
        let scroll = super::list_scroll(selected, rows);
        let hl = Style::default()
            .fg(theme.menu_highlight_fg)
            .bg(theme.menu_highlight_bg);
        for (i, entry) in entries.iter().enumerate().skip(scroll).take(rows) {
            let y = inner.y + 2 + (i - scroll) as u16;
            let style = if i == selected { hl } else { base };
            buf.set_style(Rect::new(inner.x, y, inner.width, 1), style);
            let (label, detail, hint) = match entry {
                Entry::Action {
                    label,
                    detail,
                    action,
                } => (label.clone(), detail.clone(), ctx.keymap.key_hint(action)),
                Entry::Workspace { name, .. } => {
                    (format!("Switch to {name}"), "Workspace".into(), None)
                }
            };
            put(
                buf,
                inner.x + 1,
                y,
                &format!("{detail:<9}"),
                9,
                style.add_modifier(Modifier::DIM),
            );
            put(
                buf,
                inner.x + 11,
                y,
                &label,
                inner.width.saturating_sub(12),
                style,
            );
            if let Some(hint) = hint {
                let w = hint.chars().count() as u16;
                if w + 30 < inner.width {
                    put(
                        buf,
                        inner.right() - w - 1,
                        y,
                        &hint,
                        w,
                        style.fg(theme.accent),
                    );
                }
            }
        }
        if entries.is_empty() {
            put(
                buf,
                inner.x + 1,
                inner.y + 2,
                "No matching commands",
                inner.width,
                base.add_modifier(Modifier::DIM),
            );
        }
    }
}
