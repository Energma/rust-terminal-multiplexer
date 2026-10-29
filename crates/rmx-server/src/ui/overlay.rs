//! The overlay stack entry: one variant per overlay kind, with input and
//! drawing dispatched to the concrete overlay.

use crossterm::event::{KeyEvent, MouseEvent};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

use super::{Ctx, Outcome, dialog, help, manager, menu, palette};

pub enum Overlay {
    Menu(menu::MenuBar),
    Context(menu::ContextMenu),
    Palette(palette::Palette),
    Prompt(dialog::Prompt),
    Confirm(dialog::Confirm),
    Help(help::Help),
    Manager(manager::Manager),
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
