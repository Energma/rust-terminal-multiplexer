//! Composes what one client sees and diffs it against what it saw last.
//!
//! Screen layout, top to bottom: menu bar, the active tab's panes, status bar.
//! Overlays are drawn last, over everything.

use std::collections::{HashMap, HashSet};

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use rmx_config::Theme;
use rmx_core::layout::{PaneId, Separator};
use rmx_proto::{Frame, WireCell};

use crate::pane::Pane;
use crate::select::Select;
use crate::state::{Tab, Workspace};
use crate::ui::{self, Ctx, Overlay, put};

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

pub struct Compositor {
    prev: Buffer,
    next: Buffer,
    /// Set until the client has received a full frame at the current size.
    needs_full: bool,
}

impl Compositor {
    pub fn new(cols: u16, rows: u16) -> Self {
        let area = Rect::new(0, 0, cols, rows);
        Self {
            prev: Buffer::empty(area),
            next: Buffer::empty(area),
            needs_full: true,
        }
    }

    pub fn resize(&mut self, cols: u16, rows: u16) {
        *self = Self::new(cols, rows);
    }

    pub fn area(&self) -> Rect {
        self.next.area
    }

    pub fn compose(&mut self, view: &View) -> Frame {
        let screen = self.next.area;
        self.next.reset();
        let cursor = draw(&mut self.next, screen, view);

        let full = std::mem::take(&mut self.needs_full);
        if full {
            // A full frame is a diff against the blank buffer the client
            // starts from, so default cells needn't be sent.
            self.prev = Buffer::empty(screen);
        }
        let cells = self
            .prev
            .diff(&self.next)
            .into_iter()
            .map(|(x, y, c)| (x, y, WireCell::from(c)))
            .collect();
        std::mem::swap(&mut self.prev, &mut self.next);

        let tab = view.workspace.active();
        let title = format!(
            "{} · {}",
            crate::tab_title(tab, view.ctx.panes),
            view.workspace.name
        );
        Frame {
            full,
            cols: screen.width,
            rows: screen.height,
            cells,
            cursor,
            title,
        }
    }
}

/// Draws a full client screen; returns where the terminal cursor goes.
fn draw(buf: &mut Buffer, screen: Rect, view: &View) -> Option<(u16, u16)> {
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

/// Draws pane borders with proper junction glyphs; borders touching the
/// focused pane use the active colour.
pub fn draw_separators(
    buf: &mut Buffer,
    area: Rect,
    seps: &[Separator],
    rects: &[(PaneId, Rect)],
    focus: PaneId,
    theme: &Theme,
) {
    let mut cells: HashSet<(u16, u16)> = HashSet::new();
    for s in seps {
        for i in 0..s.len {
            cells.insert(if s.vertical {
                (s.x, s.y + i)
            } else {
                (s.x + i, s.y)
            });
        }
    }
    let focus_rect = rects.iter().find(|(p, _)| *p == focus).map(|(_, r)| *r);
    let near_focus = |x: u16, y: u16| {
        focus_rect
            .is_some_and(|r| x + 1 >= r.x && x <= r.right() && y + 1 >= r.y && y <= r.bottom())
    };
    for &(x, y) in &cells {
        let up = y > area.y && cells.contains(&(x, y - 1));
        let down = cells.contains(&(x, y + 1));
        let left = x > area.x && cells.contains(&(x - 1, y));
        let right = cells.contains(&(x + 1, y));
        let glyph = match (up, down, left, right) {
            (true, true, true, true) => "┼",
            (true, true, false, true) => "├",
            (true, true, true, false) => "┤",
            (false, true, true, true) => "┬",
            (true, false, true, true) => "┴",
            (_, _, true, _) | (_, _, _, true) if !(up || down) => "─",
            (true, _, _, _) | (_, true, _, _) if !(left || right) => "│",
            _ => "┼",
        };
        let color = if near_focus(x, y) {
            theme.active_border
        } else {
            theme.border
        };
        if let Some(c) = buf.cell_mut((x, y)) {
            c.reset();
            c.set_symbol(glyph).set_style(Style::default().fg(color));
        }
    }
}

/// Column spans of each tab label in the status bar, for drawing and clicks.
pub fn tab_spans(ws: &Workspace, panes: &HashMap<PaneId, Pane>) -> Vec<(u16, u16, String)> {
    let mut x = 0;
    ws.tabs
        .iter()
        .enumerate()
        .map(|(i, tab)| {
            let mut flags = String::new();
            if tab.zoomed {
                flags.push_str(" ⤢");
            }
            if tab.sync_input {
                flags.push_str(" ⇉");
            }
            let mut title: String = crate::tab_title(tab, panes).chars().take(20).collect();
            title.push_str(&flags);
            let label = format!(" {} {} ", i + 1, title);
            let w = label.chars().count() as u16;
            let span = (x, w, label);
            x += w + 1;
            span
        })
        .collect()
}

fn draw_status(buf: &mut Buffer, row: Rect, view: &View) {
    let theme = view.ctx.theme;
    let base = Style::default().fg(theme.status_fg).bg(theme.status_bg);
    buf.set_style(row, base);
    let ws = view.workspace;

    let mode_label = match view.mode {
        "normal" => None,
        "leader" => Some("LEADER"),
        "select" => Some("SELECT"),
        "resize" => Some("RESIZE"),
        "locked" => Some("LOCKED"),
        other => Some(other),
    };
    let leader = view.ctx.keymap.leader().display_short();
    let hint = match view.mode {
        "select" => "arrows move · v select · Enter copy · / find · Esc done".to_owned(),
        "resize" => "arrows resize · Esc done".to_owned(),
        "locked" => format!("all keys go to the pane · {leader} unlocks"),
        _ => format!("{leader} leader · F10 menu · F1 help"),
    };
    let mut right = String::new();
    if let Some(m) = mode_label {
        right.push_str(&format!(" {m} "));
    }
    let right_hint = format!(" {hint} ");
    let hint_w = right_hint.chars().count() as u16;
    let mode_w = right.chars().count() as u16;

    let tabs_end = row.right().saturating_sub(hint_w + mode_w + 1);
    for (i, (x, w, label)) in tab_spans(ws, view.ctx.panes).into_iter().enumerate() {
        let x = row.x + x;
        if x + w > tabs_end {
            put(buf, x, row.y, "…", 1, base);
            break;
        }
        let style = if i == ws.active_tab {
            Style::default()
                .fg(theme.menu_highlight_fg)
                .bg(theme.accent)
                .add_modifier(Modifier::BOLD)
        } else {
            base
        };
        put(buf, x, row.y, &label, w, style);
    }
    let mut x = row.right().saturating_sub(hint_w + mode_w);
    if mode_w > 0 {
        let style = Style::default()
            .fg(theme.menu_highlight_fg)
            .bg(theme.active_border)
            .add_modifier(Modifier::BOLD);
        put(buf, x, row.y, &right, mode_w, style);
        x += mode_w;
    }
    put(
        buf,
        x,
        row.y,
        &right_hint,
        hint_w,
        base.add_modifier(Modifier::DIM),
    );
}

/// The "which-key" popup listing what can follow the pressed prefix.
fn draw_hints(buf: &mut Buffer, area: Rect, hints: &[(String, String)], theme: &Theme) {
    let key_w = hints
        .iter()
        .map(|(k, _)| k.chars().count())
        .max()
        .unwrap_or(1) as u16;
    let col_w = (key_w + 24).min(area.width);
    let cols = (area.width.saturating_sub(4) / col_w).clamp(1, 4);
    let rows = (hints.len() as u16).div_ceil(cols);
    let w = (cols * col_w + 2).min(area.width);
    let h = (rows + 2).min(area.height);
    let rect = Rect::new(
        area.right().saturating_sub(w),
        area.bottom().saturating_sub(h),
        w,
        h,
    );
    let inner = ui::panel(buf, rect, "Keys", theme);
    let base = Style::default().fg(theme.menu_fg).bg(theme.menu_bg);
    for (i, (key, label)) in hints.iter().enumerate() {
        let (c, r) = ((i as u16) / rows, (i as u16) % rows);
        if r >= inner.height {
            continue;
        }
        let x = inner.x + c * col_w;
        put(
            buf,
            x,
            inner.y + r,
            key,
            key_w,
            base.fg(theme.accent).add_modifier(Modifier::BOLD),
        );
        put(
            buf,
            x + key_w + 1,
            inner.y + r,
            label,
            col_w.saturating_sub(key_w + 2),
            base,
        );
    }
}
