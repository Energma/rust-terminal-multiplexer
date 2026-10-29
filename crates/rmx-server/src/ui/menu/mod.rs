//! The Windows-style menu bar (F10 or click) and the right-click context menu.

mod bar;
mod catalog;
mod context;
mod items;
mod menu_bar;
mod menu_bar_input;

use rmx_config::action::Action;

pub use bar::{render_bar, title_positions};
pub use catalog::menus;
pub use context::ContextMenu;
pub use menu_bar::MenuBar;

pub enum Item {
    Entry(&'static str, Action),
    Separator,
}

pub struct Menu {
    pub title: &'static str,
    pub items: Vec<Item>,
}
