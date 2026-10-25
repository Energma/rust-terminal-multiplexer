//! Small modal dialogs: a one-line text prompt and a yes/no confirmation.

use crossterm::event::{KeyCode, KeyEvent, MouseButton, MouseEvent, MouseEventKind};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};

use super::{Cmd, Ctx, Outcome, PromptThen, TextInput, centered, contains, panel, put};

pub struct Prompt {
    title: String,
    input: TextInput,
    then: PromptThen,
}

impl Prompt {
    pub fn new(title: String, initial: &str, then: PromptThen) -> Self {
        Self {
            title,
            input: TextInput::new(initial),
            then,
        }
    }

    pub fn input_mut(&mut self) -> &mut TextInput {
        &mut self.input
    }

    pub fn handle_key(&mut self, key: &KeyEvent) -> Outcome {
        match key.code {
            KeyCode::Esc => Outcome::CLOSE,
            KeyCode::Enter => {
                let text = self.input.text.trim().to_owned();
                if text.is_empty() {
                    return Outcome::CLOSE;
                }
                let cmd = match self.then.clone() {
                    PromptThen::RenameTab { workspace, tab } => Cmd::RenameTab {
                        workspace,
                        tab,
                        name: text,
                    },
                    PromptThen::RenameWorkspace { workspace } => Cmd::RenameWorkspace {
                        workspace,
                        name: text,
                    },
                    PromptThen::NewWorkspace => Cmd::NewWorkspace { name: text },
                    PromptThen::Find => Cmd::Find(text),
                };
                Outcome::cmd(true, cmd)
            }
            _ => {
                self.input.handle(key);
                Outcome::STAY
            }
        }
    }

    pub fn render(&self, ctx: &Ctx, screen: Rect, buf: &mut Buffer) {
        let rect = centered(screen, 56, 5);
        let inner = panel(buf, rect, &self.title, ctx.theme);
        if inner.height == 0 {
            return;
        }
        let base = Style::default().fg(ctx.theme.menu_fg).bg(ctx.theme.menu_bg);
        self.input.render(
            buf,
            inner.x + 1,
            inner.y,
            inner.width.saturating_sub(2),
            base.add_modifier(Modifier::BOLD),
        );
        put(
            buf,
            inner.x + 1,
            inner.y + 2,
            "Enter to confirm · Esc to cancel",
            inner.width.saturating_sub(2),
            base.add_modifier(Modifier::DIM),
        );
    }
}

pub struct Confirm {
    message: String,
    then: Cmd,
    yes: bool,
}

impl Confirm {
    pub fn new(message: String, then: Cmd) -> Self {
        // Default to "No": closing things should take a deliberate choice.
        Self {
            message,
            then,
            yes: false,
        }
    }

    fn buttons(&self, screen: Rect) -> (Rect, Rect) {
        let rect = self.rect(screen);
        let y = rect.bottom().saturating_sub(2);
        let mid = rect.x + rect.width / 2;
        (
            Rect::new(mid.saturating_sub(10), y, 7, 1),
            Rect::new(mid + 3, y, 6, 1),
        )
    }

    fn rect(&self, screen: Rect) -> Rect {
        centered(
            screen,
            (self.message.chars().count() as u16 + 6).clamp(36, 70),
            7,
        )
    }

    pub fn handle_key(&mut self, key: &KeyEvent) -> Outcome {
        match key.code {
            KeyCode::Char('y' | 'Y') => Outcome::cmd(true, self.then.clone()),
            KeyCode::Char('n' | 'N') | KeyCode::Esc => Outcome::CLOSE,
            KeyCode::Left | KeyCode::Right | KeyCode::Tab => {
                self.yes = !self.yes;
                Outcome::STAY
            }
            KeyCode::Enter if self.yes => Outcome::cmd(true, self.then.clone()),
            KeyCode::Enter => Outcome::CLOSE,
            _ => Outcome::STAY,
        }
    }

    pub fn handle_mouse(&mut self, ev: &MouseEvent, screen: Rect) -> Outcome {
        if let MouseEventKind::Up(MouseButton::Left) = ev.kind {
            let (yes, no) = self.buttons(screen);
            if contains(yes, ev.column, ev.row) {
                return Outcome::cmd(true, self.then.clone());
            }
            if contains(no, ev.column, ev.row) {
                return Outcome::CLOSE;
            }
        }
        Outcome::STAY
    }

    pub fn render(&self, ctx: &Ctx, screen: Rect, buf: &mut Buffer) {
        let theme = ctx.theme;
        let inner = panel(buf, self.rect(screen), "Confirm", theme);
        let base = Style::default().fg(theme.menu_fg).bg(theme.menu_bg);
        put(
            buf,
            inner.x + 1,
            inner.y + 1,
            &self.message,
            inner.width.saturating_sub(2),
            base,
        );
        let hl = Style::default()
            .fg(theme.menu_highlight_fg)
            .bg(theme.menu_highlight_bg);
        let (yes, no) = self.buttons(screen);
        put(
            buf,
            yes.x,
            yes.y,
            "[ Yes ]",
            yes.width,
            if self.yes { hl } else { base },
        );
        put(
            buf,
            no.x,
            no.y,
            "[ No ]",
            no.width,
            if self.yes { base } else { hl },
        );
    }
}
