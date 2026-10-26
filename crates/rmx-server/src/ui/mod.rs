//! Overlays drawn on top of the panes: menus, the command palette, prompts,
//! dialogs, help and the session manager.
//!
//! They are composed on the server like everything else, so every client
//! (and every OS) gets the same UI. Overlays form a stack per client: the top
//! one gets input, so e.g. a rename prompt can sit on the session manager.
//! Overlays never mutate core state; they return an [`Effect`] for the core.

pub mod dialog;
pub mod help;
pub mod manager;
pub mod menu;
pub mod palette;

use std::collections::HashMap;
use std::path::Path;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseEvent};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::widgets::{Block, BorderType, Clear, Widget};
use rmx_config::Theme;
use rmx_config::action::Action;
use rmx_config::keymap::Keymap;
use rmx_core::layout::PaneId;

use crate::pane::Pane;
use crate::state::{TabId, Workspace, WorkspaceId};

/// Read-only view of the core that overlays render and navigate against.
pub struct Ctx<'a> {
    pub theme: &'a Theme,
    pub keymap: &'a Keymap,
    pub workspaces: &'a [Workspace],
    pub panes: &'a HashMap<PaneId, Pane>,
    /// Interactive clients per workspace (including this one).
    pub attached: &'a HashMap<WorkspaceId, usize>,
    /// Where this client ran `rmx`, if known.
    pub launch_cwd: Option<&'a Path>,
    pub current: WorkspaceId,
    pub config_path: &'a Path,
    pub commands: &'a [rmx_config::CustomCommand],
}

pub enum Overlay {
    Menu(menu::MenuBar),
    Context(menu::ContextMenu),
    Palette(palette::Palette),
    Prompt(dialog::Prompt),
    Confirm(dialog::Confirm),
    Help(help::Help),
    Manager(manager::Manager),
}

/// What an overlay asks the core to do after handling input.
pub struct Outcome {
    /// Remove this overlay from the stack.
    pub pop: bool,
    pub effect: Option<Effect>,
}

impl Outcome {
    pub const STAY: Self = Self {
        pop: false,
        effect: None,
    };
    pub const CLOSE: Self = Self {
        pop: true,
        effect: None,
    };

    pub fn run(action: Action) -> Self {
        Self {
            pop: true,
            effect: Some(Effect::Action(action)),
        }
    }

    pub fn cmd(pop: bool, cmd: Cmd) -> Self {
        Self {
            pop,
            effect: Some(Effect::Cmd(cmd)),
        }
    }
}

pub enum Effect {
    Action(Action),
    Cmd(Cmd),
}

/// Core operations that only overlays trigger.
#[derive(Debug, Clone)]
pub enum Cmd {
    RenameTab {
        workspace: WorkspaceId,
        tab: TabId,
        name: String,
    },
    RenameWorkspace {
        workspace: WorkspaceId,
        name: String,
    },
    NewWorkspace {
        /// `None` names it after its folder.
        name: Option<String>,
        /// `None` uses the current pane's folder.
        cwd: Option<std::path::PathBuf>,
    },
    Find(String),
    Open {
        workspace: WorkspaceId,
        tab: Option<TabId>,
        pane: Option<PaneId>,
    },
    NewTab {
        workspace: WorkspaceId,
    },
    Split {
        workspace: WorkspaceId,
        tab: TabId,
        right: bool,
    },
    CloseWorkspace {
        workspace: WorkspaceId,
    },
    CloseTab {
        workspace: WorkspaceId,
        tab: TabId,
    },
    /// Ask before closing; carries the follow-up command.
    Confirm {
        message: String,
        then: Box<Cmd>,
    },
    Prompt {
        title: String,
        initial: String,
        then: PromptThen,
    },
    QuitServer,
}

/// What a prompt's text is used for.
#[derive(Debug, Clone)]
pub enum PromptThen {
    RenameTab { workspace: WorkspaceId, tab: TabId },
    RenameWorkspace { workspace: WorkspaceId },
    NewWorkspace { cwd: Option<std::path::PathBuf> },
    Find,
}

impl Overlay {
    pub fn handle_key(&mut self, key: &KeyEvent, ctx: &Ctx) -> Outcome {
        match self {
            Overlay::Menu(o) => o.handle_key(key, ctx),
            Overlay::Context(o) => o.handle_key(key),
            Overlay::Palette(o) => o.handle_key(key, ctx),
            Overlay::Prompt(o) => o.handle_key(key),
            Overlay::Confirm(o) => o.handle_key(key),
            Overlay::Help(o) => o.handle_key(key),
            Overlay::Manager(o) => o.handle_key(key, ctx),
        }
    }

    /// Inserts pasted text into the overlay's text field, if it has one.
    pub fn paste(&mut self, text: &str) {
        match self {
            Overlay::Prompt(o) => o.input_mut().insert_str(text),
            Overlay::Palette(o) => o.paste(text),
            Overlay::Manager(o) => o.paste(text),
            Overlay::Menu(_) | Overlay::Context(_) | Overlay::Confirm(_) | Overlay::Help(_) => {}
        }
    }

    /// `screen` is the client's full area (menus need absolute positions).
    pub fn handle_mouse(&mut self, ev: &MouseEvent, ctx: &Ctx, screen: Rect) -> Outcome {
        match self {
            Overlay::Menu(o) => o.handle_mouse(ev, ctx),
            Overlay::Context(o) => o.handle_mouse(ev, screen),
            Overlay::Palette(o) => o.handle_mouse(ev, ctx, screen),
            Overlay::Prompt(_) => Outcome::STAY,
            Overlay::Confirm(o) => o.handle_mouse(ev, screen),
            Overlay::Help(o) => o.handle_mouse(ev),
            Overlay::Manager(o) => o.handle_mouse(ev, ctx, screen),
        }
    }

    pub fn render(&self, ctx: &Ctx, screen: Rect, buf: &mut Buffer) {
        match self {
            Overlay::Menu(o) => o.render(ctx, buf),
            Overlay::Context(o) => o.render(ctx, screen, buf),
            Overlay::Palette(o) => o.render(ctx, screen, buf),
            Overlay::Prompt(o) => o.render(ctx, screen, buf),
            Overlay::Confirm(o) => o.render(ctx, screen, buf),
            Overlay::Help(o) => o.render(ctx, screen, buf),
            Overlay::Manager(o) => o.render(ctx, screen, buf),
        }
    }
}

/// A rect of at most `w`×`h` centred in `area`.
pub fn centered(area: Rect, w: u16, h: u16) -> Rect {
    let w = w.min(area.width);
    let h = h.min(area.height);
    Rect::new(
        area.x + (area.width - w) / 2,
        area.y + (area.height - h) / 2,
        w,
        h,
    )
}

/// Clears `rect` and draws a rounded, titled box; returns the inner area.
pub fn panel(buf: &mut Buffer, rect: Rect, title: &str, theme: &Theme) -> Rect {
    Clear.render(rect, buf);
    let base = Style::default().fg(theme.menu_fg).bg(theme.menu_bg);
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(base.fg(theme.accent))
        .title_style(base.fg(theme.accent))
        .style(base);
    let block = if title.is_empty() {
        block
    } else {
        block.title(format!(" {title} "))
    };
    let inner = block.inner(rect);
    block.render(rect, buf);
    inner
}

/// Writes `text` clipped to `width` cells.
/// Writes `text` clipped to `width` cells and to the buffer, so a too-small
/// screen truncates the UI instead of panicking.
pub fn put(buf: &mut Buffer, x: u16, y: u16, text: &str, width: u16, style: Style) {
    let area = buf.area;
    if y < area.y || y >= area.bottom() || x < area.x || x >= area.right() {
        return;
    }
    let width = width.min(area.right() - x);
    if width > 0 {
        buf.set_stringn(x, y, text, usize::from(width), style);
    }
}

/// First visible row of a list so `selected` stays in view; shared by
/// drawing and mouse hit-testing so clicks land on what's shown.
pub fn list_scroll(selected: usize, visible_rows: usize) -> usize {
    selected.saturating_sub(visible_rows.saturating_sub(1))
}

pub fn contains(r: Rect, x: u16, y: u16) -> bool {
    x >= r.x && x < r.right() && y >= r.y && y < r.bottom()
}

/// Single-line text editing shared by prompts, the palette and filters.
#[derive(Debug, Default, Clone)]
pub struct TextInput {
    pub text: String,
    /// Cursor position in chars.
    pub cursor: usize,
}

impl TextInput {
    pub fn new(text: &str) -> Self {
        Self {
            text: text.to_owned(),
            cursor: text.chars().count(),
        }
    }

    /// Inserts text at the cursor, dropping line breaks and control chars.
    pub fn insert_str(&mut self, text: &str) {
        for c in text.chars().filter(|c| !c.is_control()) {
            let at = self
                .text
                .char_indices()
                .nth(self.cursor)
                .map_or(self.text.len(), |(i, _)| i);
            self.text.insert(at, c);
            self.cursor += 1;
        }
    }

    /// Returns true if the key edited the text or moved the cursor.
    pub fn handle(&mut self, key: &KeyEvent) -> bool {
        let byte = |s: &str, c: usize| s.char_indices().nth(c).map_or(s.len(), |(i, _)| i);
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        match key.code {
            KeyCode::Char('u') if ctrl => {
                self.text.clear();
                self.cursor = 0;
            }
            KeyCode::Char('w') if ctrl => {
                let before: String = self.text.chars().take(self.cursor).collect();
                let trimmed = before.trim_end();
                let keep = trimmed.rfind(' ').map_or(0, |i| i + 1);
                let removed = before.chars().count() - trimmed[..keep].chars().count();
                let start = byte(&self.text, self.cursor - removed);
                let end = byte(&self.text, self.cursor);
                self.text.replace_range(start..end, "");
                self.cursor -= removed;
            }
            KeyCode::Char(c) if !ctrl => {
                let at = byte(&self.text, self.cursor);
                self.text.insert(at, c);
                self.cursor += 1;
            }
            KeyCode::Backspace if self.cursor > 0 => {
                let at = byte(&self.text, self.cursor - 1);
                self.text.remove(at);
                self.cursor -= 1;
            }
            KeyCode::Delete if self.cursor < self.text.chars().count() => {
                let at = byte(&self.text, self.cursor);
                self.text.remove(at);
            }
            KeyCode::Left => self.cursor = self.cursor.saturating_sub(1),
            KeyCode::Right => self.cursor = (self.cursor + 1).min(self.text.chars().count()),
            KeyCode::Home => self.cursor = 0,
            KeyCode::End => self.cursor = self.text.chars().count(),
            _ => return false,
        }
        true
    }

    /// Draws the text with a block cursor.
    pub fn render(&self, buf: &mut Buffer, x: u16, y: u16, width: u16, style: Style) {
        if width == 0 {
            return;
        }
        let chars: Vec<char> = self.text.chars().collect();
        // Scroll horizontally so the cursor stays visible.
        let start = (self.cursor + 1).saturating_sub(usize::from(width));
        let visible: String = chars.iter().skip(start).take(usize::from(width)).collect();
        put(buf, x, y, &visible, width, style);
        let cx = x + (self.cursor - start) as u16;
        if cx < x + width
            && let Some(cell) = buf.cell_mut((cx, y))
        {
            cell.set_style(style.add_modifier(ratatui::style::Modifier::REVERSED));
        }
    }
}

/// Case-insensitive subsequence match; higher is better, `None` if no match.
/// Consecutive and word-start matches score higher, so "sr" ranks
/// "Split right" above "Select mode … resize".
pub fn fuzzy_score(query: &str, candidate: &str) -> Option<i32> {
    if query.is_empty() {
        return Some(0);
    }
    let cand: Vec<char> = candidate.to_lowercase().chars().collect();
    let mut score = 0;
    let mut pos = 0;
    let mut prev: Option<usize> = None;
    for q in query.to_lowercase().chars().filter(|c| !c.is_whitespace()) {
        let found = (pos..cand.len()).find(|&i| cand[i] == q)?;
        score += 1;
        if prev == Some(found.wrapping_sub(1)) {
            score += 5;
        }
        if found == 0 || matches!(cand[found - 1], ' ' | '-' | ':' | '/') {
            score += 3;
        }
        score -= (found - pos).min(5) as i32;
        prev = Some(found);
        pos = found + 1;
    }
    Some(score)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fuzzy_prefers_word_starts_and_runs() {
        assert!(
            fuzzy_score("sr", "Split right").unwrap()
                > fuzzy_score("sr", "Close current").unwrap_or(-99)
        );
        assert!(fuzzy_score("zoom", "Toggle pane zoom").is_some());
        assert_eq!(fuzzy_score("xyz", "Split right"), None);
    }

    #[test]
    fn text_input_edits_unicode() {
        let mut t = TextInput::new("héllo");
        t.handle(&KeyEvent::from(KeyCode::Backspace));
        assert_eq!(t.text, "héll");
        t.handle(&KeyEvent::from(KeyCode::Home));
        t.handle(&KeyEvent::from(KeyCode::Char('¡')));
        assert_eq!(t.text, "¡héll");
        t.handle(&KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL));
        assert_eq!(t.text, "");
    }
}
