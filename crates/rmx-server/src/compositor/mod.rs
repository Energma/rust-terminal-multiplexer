//! Composes what one client sees and diffs it against what it saw last.
//!
//! Screen layout, top to bottom: menu bar, the active tab's panes, status bar.
//! Overlays are drawn last, over everything.

mod frame;
mod geometry;
mod hints;
mod screen;
mod separators;
mod status;

use crate::select::Select;
use crate::state::Workspace;
use crate::ui::{Ctx, Overlay};

pub use frame::Compositor;
pub use geometry::{CHROME_ROWS, pane_area, status_row, tab_rects};
pub use separators::draw_separators;
pub use status::tab_spans;

/// Everything the compositor needs about one client's view.
pub struct View<'a> {
    pub ctx: &'a Ctx<'a>,
    pub workspace: &'a Workspace,
    /// The workspace's shared pane-area size (smallest attached client).
    pub view_size: (u16, u16),
    pub mode: &'a str,
    pub hints: Option<&'a [(String, String)]>,
    pub toast: Option<(&'a str, bool)>,
    pub select: Option<&'a Select>,
    pub overlays: &'a [Overlay],
}
