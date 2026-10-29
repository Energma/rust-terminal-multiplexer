//! The status bar: tab labels on the left, mode and key hints on the right.

use std::collections::HashMap;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use rmx_core::layout::PaneId;

use super::View;
use crate::pane::Pane;
use crate::state::Workspace;
use crate::ui::put;

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

pub(super) fn draw_status(buf: &mut Buffer, row: Rect, view: &View) {
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
