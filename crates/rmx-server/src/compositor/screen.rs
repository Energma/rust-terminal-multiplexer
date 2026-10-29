//! Drawing one full client screen: menu bar, panes with their badges,
//! separators, status bar, hints, toast and overlays, in that order.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use rmx_config::Theme;
use rmx_core::layout::PaneId;

use super::hints::draw_hints;
use super::status::draw_status;
use super::{View, draw_separators, pane_area, status_row, tab_rects};
use crate::pane::Pane;
use crate::state::Tab;
use crate::ui::{self, Overlay, put};

/// Draws a full client screen; returns where the terminal cursor goes.
pub(super) fn draw(buf: &mut Buffer, screen: Rect, view: &View) -> Option<(u16, u16)> {
    let ctx = view.ctx;
    let theme = ctx.theme;
    let ws = view.workspace;
    let tab = ws.active();

    let menu_open = view.overlays.iter().find_map(|o| match o {
        Overlay::Menu(m) => Some(m.open),
        _ => None,
    });
    ui::menu::render_bar(
        buf,
        Rect::new(screen.x, screen.y, screen.width, 1),
        ctx,
        menu_open,
        &ws.name,
    );

    // Panes are sized for the smallest attached client; larger clients see
    // the unused margin dotted, as tmux does.
    let full = pane_area(screen);
    let area = Rect::new(
        full.x,
        full.y,
        view.view_size.0.min(full.width),
        view.view_size.1.min(full.height),
    );
    if area != full {
        let dots = Style::default().fg(theme.border);
        for y in full.top()..full.bottom() {
            for x in full.left()..full.right() {
                if (x >= area.right() || y >= area.bottom())
                    && let Some(c) = buf.cell_mut((x, y))
                {
                    c.set_symbol("·").set_style(dots);
                }
            }
        }
    }

    let (rects, seps) = tab_rects(tab, area);
    let mut cursor = None;
    for &(id, rect) in &rects {
        // Never draw outside this client's buffer, whatever the layout says.
        let rect = rect.intersection(buf.area);
        if rect.is_empty() {
            continue;
        }
        let Some(pane) = ctx.panes.get(&id) else {
            continue;
        };
        pane.emulator.render(rect, buf);
        if let Some(sel) = view.select.filter(|s| s.pane == id) {
            sel.render(pane.emulator.as_ref(), rect, buf, theme);
        }
        badges(buf, rect, pane, tab, id, view, theme);
        if id == tab.active_pane
            && view.select.is_none()
            && let Some((row, col)) = pane.emulator.cursor()
            && col < rect.width
            && row < rect.height
        {
            cursor = Some((rect.x + col, rect.y + row));
        }
    }
    draw_separators(buf, area, &seps, &rects, tab.active_pane, theme);
    draw_status(buf, status_row(screen), view);

    if let Some(hints) = view.hints.filter(|h| !h.is_empty()) {
        draw_hints(buf, full, hints, theme);
    }
    if let Some((text, error)) = view.toast.filter(|_| screen.height >= 3) {
        let w = (text.chars().count() as u16 + 2).min(screen.width);
        let style = if error {
            Style::default()
                .fg(theme.menu_highlight_fg)
                .bg(ratatui::style::Color::Red)
        } else {
            Style::default()
                .fg(theme.menu_highlight_fg)
                .bg(theme.menu_highlight_bg)
        };
        put(
            buf,
            screen.right().saturating_sub(w),
            screen.y + 1,
            &format!(" {text} "),
            w,
            style,
        );
    }
    for overlay in view.overlays {
        overlay.render(ctx, screen, buf);
    }
    // Overlays draw their own block cursor in text fields.
    if view.overlays.is_empty() {
        cursor
    } else {
        None
    }
}

fn badges(
    buf: &mut Buffer,
    rect: Rect,
    pane: &Pane,
    tab: &Tab,
    id: PaneId,
    view: &View,
    theme: &Theme,
) {
    let style = Style::default()
        .fg(theme.menu_highlight_fg)
        .bg(theme.accent)
        .add_modifier(Modifier::BOLD);
    let mut label = String::new();
    if tab.zoomed && id == tab.active_pane {
        label.push_str(" ZOOM ");
    }
    let offset = pane.emulator.view_offset();
    if offset > 0 {
        label.push_str(&format!(" ↑{offset}/{} ", pane.emulator.history_len()));
    }
    if let Some(sel) = view.select.filter(|s| s.pane == id)
        && let Some(status) = sel.status()
    {
        label.push_str(&format!(" {status} "));
    }
    if !label.is_empty() {
        let w = (label.chars().count() as u16).min(rect.width);
        put(
            buf,
            rect.right().saturating_sub(w),
            rect.y,
            &label,
            w,
            style,
        );
    }
}
