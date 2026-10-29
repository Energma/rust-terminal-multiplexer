//! Where things go on a client's screen: the pane area between the bars,
//! the status row, and each pane's rect within a tab.

use ratatui::layout::Rect;
use rmx_core::layout::{PaneId, Separator};

use crate::state::Tab;

/// Rows taken by the menu bar (top) and status bar (bottom).
pub const CHROME_ROWS: u16 = 2;

/// Where panes go within a client's screen.
pub fn pane_area(screen: Rect) -> Rect {
    Rect::new(
        screen.x,
        screen.y + 1,
        screen.width,
        screen.height.saturating_sub(CHROME_ROWS),
    )
}

pub fn status_row(screen: Rect) -> Rect {
    Rect::new(screen.x, screen.bottom().saturating_sub(1), screen.width, 1)
}

/// Pane rects for a tab within `area` (zoom shows only the active pane).
pub fn tab_rects(tab: &Tab, area: Rect) -> (Vec<(PaneId, Rect)>, Vec<Separator>) {
    if tab.zoomed {
        (vec![(tab.active_pane, area)], Vec::new())
    } else {
        tab.layout.compute(area)
    }
}
