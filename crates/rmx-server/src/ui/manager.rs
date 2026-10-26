//! The session manager: a full-screen tree of workspaces › tabs › panes with
//! a live preview, from which users open, create, rename, close and split.
//!
//! It replaces tmux's `choose-tree`, but is built to be the place you *drive*
//! sessions from, with every operation one key away and listed on screen.

use std::collections::HashSet;
use std::path::Path;
use std::time::{Duration, Instant};

use crossterm::event::{KeyCode, KeyEvent, MouseButton, MouseEvent, MouseEventKind};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use rmx_core::layout::PaneId;

use super::{Cmd, Ctx, Outcome, PromptThen, TextInput, contains, panel, put};
use crate::compositor::draw_separators;
use crate::state::{TabId, WorkspaceId};

fn tab_id(ctx: &Ctx, w: WorkspaceId, t: usize) -> Option<TabId> {
    ctx.workspaces
        .iter()
        .find(|x| x.id == w)?
        .tabs
        .get(t)
        .map(|tab| tab.id)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Row {
    /// "+ New workspace in <folder where rmx was run>".
    NewHere,
    Workspace(WorkspaceId),
    Tab(WorkspaceId, usize),
    Pane(WorkspaceId, usize, PaneId),
}

impl Row {
    fn workspace(self) -> Option<WorkspaceId> {
        match self {
            Row::NewHere => None,
            Row::Workspace(w) | Row::Tab(w, _) | Row::Pane(w, _, _) => Some(w),
        }
    }
}

pub struct Manager {
    selected: Option<Row>,
    expanded_ws: HashSet<WorkspaceId>,
    expanded_tabs: HashSet<(WorkspaceId, usize)>,
    filter: Option<TextInput>,
    last_click: Option<(Instant, u16)>,
}

const FOOTER: &[(&str, &str)] = &[
    ("Enter", "open"),
    ("1-9", "jump"),
    ("←→", "fold"),
    ("n", "new workspace"),
    ("t", "new tab"),
    ("|", "split right"),
    ("-", "split down"),
    ("r", "rename"),
    ("x", "close"),
    ("/", "filter"),
    ("Esc", "back"),
];

impl Manager {
    pub fn new(ctx: &Ctx) -> Self {
        let mut expanded_ws = HashSet::new();
        expanded_ws.insert(ctx.current);
        let selected = ctx
            .workspaces
            .iter()
            .find(|w| w.id == ctx.current)
            .map(|w| Row::Tab(w.id, w.active_tab));
        Self {
            selected,
            expanded_ws,
            expanded_tabs: HashSet::new(),
            filter: None,
            last_click: None,
        }
    }

    fn rows(&self, ctx: &Ctx) -> Vec<Row> {
        let query = self
            .filter
            .as_ref()
            .map(|f| f.text.to_lowercase())
            .unwrap_or_default();
        let mut rows = Vec::new();
        if ctx.launch_cwd.is_some() && query.is_empty() {
            rows.push(Row::NewHere);
        }
        for ws in ctx.workspaces {
            let tab_name = |t: usize| crate::tab_title(&ws.tabs[t], ctx.panes).to_lowercase();
            let ws_match = ws.name.to_lowercase().contains(&query);
            let tab_matches: Vec<usize> = (0..ws.tabs.len())
                .filter(|&t| tab_name(t).contains(&query))
                .collect();
            if !query.is_empty() && !ws_match && tab_matches.is_empty() {
                continue;
            }
            rows.push(Row::Workspace(ws.id));
            // While filtering, expand everything that matched.
            if !self.expanded_ws.contains(&ws.id) && query.is_empty() {
                continue;
            }
            for (t, tab) in ws.tabs.iter().enumerate() {
                if !query.is_empty() && !ws_match && !tab_matches.contains(&t) {
                    continue;
                }
                rows.push(Row::Tab(ws.id, t));
                if self.expanded_tabs.contains(&(ws.id, t)) {
                    rows.extend(
                        tab.layout
                            .panes()
                            .into_iter()
                            .map(|p| Row::Pane(ws.id, t, p)),
                    );
                }
            }
        }
        rows
    }

    /// Index of the selection, falling back to the current workspace (e.g.
    /// after the selected item was closed).
    fn index(&self, rows: &[Row], ctx: &Ctx) -> usize {
        self.selected
            .and_then(|s| rows.iter().position(|r| *r == s))
            .or_else(|| rows.iter().position(|r| r.workspace() == Some(ctx.current)))
            .unwrap_or(0)
    }

    fn select(&mut self, rows: &[Row], index: usize) {
        self.selected = rows.get(index).copied();
    }

    fn open(&self, row: Row, ctx: &Ctx) -> Outcome {
        let tab = |w, t| tab_id(ctx, w, t);
        let cmd = match row {
            Row::NewHere => Cmd::NewWorkspace {
                name: None,
                cwd: ctx.launch_cwd.map(Path::to_path_buf),
            },
            Row::Workspace(w) => Cmd::Open {
                workspace: w,
                tab: None,
                pane: None,
            },
            Row::Tab(w, t) => Cmd::Open {
                workspace: w,
                tab: tab(w, t),
                pane: None,
            },
            Row::Pane(w, t, p) => Cmd::Open {
                workspace: w,
                tab: tab(w, t),
                pane: Some(p),
            },
        };
        Outcome::cmd(true, cmd)
    }

    /// The tab an operation on `row` applies to: the row's own tab, or the
    /// workspace's active tab.
    fn tab_for(row: Row, ctx: &Ctx) -> Option<(WorkspaceId, TabId)> {
        let (w, t) = match row {
            Row::Tab(w, t) | Row::Pane(w, t, _) => (w, t),
            Row::NewHere => return None,
            Row::Workspace(w) => (w, ctx.workspaces.iter().find(|x| x.id == w)?.active_tab),
        };
        Some((w, tab_id(ctx, w, t)?))
    }

    pub fn paste(&mut self, text: &str) {
        if let Some(filter) = &mut self.filter {
            filter.insert_str(text);
        }
    }

    pub fn handle_key(&mut self, key: &KeyEvent, ctx: &Ctx) -> Outcome {
        if let Some(filter) = &mut self.filter {
            match key.code {
                KeyCode::Esc => {
                    self.filter = None;
                    return Outcome::STAY;
                }
                KeyCode::Enter | KeyCode::Up | KeyCode::Down => {}
                _ => {
                    filter.handle(key);
                    return Outcome::STAY;
                }
            }
        }
        let rows = self.rows(ctx);
        if rows.is_empty() {
            return if key.code == KeyCode::Esc {
                Outcome::CLOSE
            } else {
                Outcome::STAY
            };
        }
        let i = self.index(&rows, ctx);
        let row = rows[i];
        // On the "new workspace here" row only opening, `n`, navigation and
        // filtering make sense.
        if row == Row::NewHere {
            match key.code {
                KeyCode::Enter | KeyCode::Right | KeyCode::Char('l') => return self.open(row, ctx),
                KeyCode::Up
                | KeyCode::Down
                | KeyCode::Home
                | KeyCode::End
                | KeyCode::PageUp
                | KeyCode::PageDown
                | KeyCode::Esc
                | KeyCode::Char('k' | 'j' | 'g' | 'G' | 'q' | 'n' | '/' | '1'..='9') => {}
                _ => return Outcome::STAY,
            }
        }
        // 1-9 open that workspace directly, the fastest way to switch.
        if let KeyCode::Char(c @ '1'..='9') = key.code
            && let Some(ws) = ctx.workspaces.get(usize::from(c as u8 - b'1'))
        {
            return self.open(Row::Workspace(ws.id), ctx);
        }
        let ws_name = |w: WorkspaceId| {
            ctx.workspaces
                .iter()
                .find(|x| x.id == w)
                .map(|x| x.name.clone())
                .unwrap_or_default()
        };
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => return Outcome::CLOSE,
            KeyCode::Up | KeyCode::Char('k') => self.select(&rows, i.saturating_sub(1)),
            KeyCode::Down | KeyCode::Char('j') => self.select(&rows, (i + 1).min(rows.len() - 1)),
            KeyCode::Home | KeyCode::Char('g') => self.select(&rows, 0),
            KeyCode::End | KeyCode::Char('G') => self.select(&rows, rows.len() - 1),
            KeyCode::PageUp => self.select(&rows, i.saturating_sub(10)),
            KeyCode::PageDown => self.select(&rows, (i + 10).min(rows.len() - 1)),
            KeyCode::Right | KeyCode::Char('l') => match row {
                Row::Workspace(w) if !self.expanded_ws.contains(&w) => {
                    self.expanded_ws.insert(w);
                }
                Row::Tab(w, t) if !self.expanded_tabs.contains(&(w, t)) => {
                    self.expanded_tabs.insert((w, t));
                }
                _ => self.select(&rows, (i + 1).min(rows.len() - 1)),
            },
            KeyCode::Left | KeyCode::Char('h') => match row {
                Row::NewHere => {}
                Row::Workspace(w) => {
                    self.expanded_ws.remove(&w);
                }
                Row::Tab(w, t) if self.expanded_tabs.contains(&(w, t)) => {
                    self.expanded_tabs.remove(&(w, t));
                }
                Row::Tab(w, _) => {
                    self.expanded_ws.remove(&w);
                    self.selected = Some(Row::Workspace(w));
                }
                Row::Pane(w, t, _) => {
                    self.expanded_tabs.remove(&(w, t));
                    self.selected = Some(Row::Tab(w, t));
                }
            },
            KeyCode::Char(' ') => match row {
                Row::NewHere => {}
                Row::Workspace(w) => {
                    if !self.expanded_ws.remove(&w) {
                        self.expanded_ws.insert(w);
                    }
                }
                Row::Tab(w, t) => {
                    if !self.expanded_tabs.remove(&(w, t)) {
                        self.expanded_tabs.insert((w, t));
                    }
                }
                Row::Pane(..) => {}
            },
            KeyCode::Enter => return self.open(row, ctx),
            KeyCode::Char('/') => self.filter = Some(TextInput::default()),
            KeyCode::Char('n') => {
                return Outcome::cmd(
                    false,
                    Cmd::Prompt {
                        title: "New workspace name".into(),
                        initial: ctx
                            .launch_cwd
                            .and_then(Path::file_name)
                            .map(|n| n.to_string_lossy().into_owned())
                            .unwrap_or_default(),
                        then: PromptThen::NewWorkspace {
                            cwd: ctx.launch_cwd.map(Path::to_path_buf),
                        },
                    },
                );
            }
            KeyCode::Char('t') => {
                let Some(w) = row.workspace() else {
                    return Outcome::STAY;
                };
                self.expanded_ws.insert(w);
                return Outcome::cmd(false, Cmd::NewTab { workspace: w });
            }
            KeyCode::Char('|' | 'v') => {
                if let Some((workspace, tab)) = Self::tab_for(row, ctx) {
                    return Outcome::cmd(
                        false,
                        Cmd::Split {
                            workspace,
                            tab,
                            right: true,
                        },
                    );
                }
            }
            KeyCode::Char('-' | 's') => {
                if let Some((workspace, tab)) = Self::tab_for(row, ctx) {
                    return Outcome::cmd(
                        false,
                        Cmd::Split {
                            workspace,
                            tab,
                            right: false,
                        },
                    );
                }
            }
            KeyCode::Char('r') | KeyCode::F(2) => {
                let (title, initial, then) = match row {
                    Row::NewHere => return Outcome::STAY,
                    Row::Workspace(w) => (
                        "Rename workspace".to_owned(),
                        ws_name(w),
                        PromptThen::RenameWorkspace { workspace: w },
                    ),
                    Row::Tab(w, t) | Row::Pane(w, t, _) => {
                        let ws = ctx.workspaces.iter().find(|x| x.id == w);
                        let name = ws
                            .map(|ws| crate::tab_title(&ws.tabs[t], ctx.panes))
                            .unwrap_or_default();
                        (
                            "Rename tab".to_owned(),
                            name,
                            PromptThen::RenameTab {
                                workspace: w,
                                tab: tab_id(ctx, w, t).unwrap_or_default(),
                            },
                        )
                    }
                };
                return Outcome::cmd(
                    false,
                    Cmd::Prompt {
                        title,
                        initial,
                        then,
                    },
                );
            }
            KeyCode::Char('x') | KeyCode::Delete => {
                let (message, then) = match row {
                    Row::NewHere => return Outcome::STAY,
                    Row::Workspace(w) => (
                        format!("Close workspace \"{}\" and all its tabs?", ws_name(w)),
                        Cmd::CloseWorkspace { workspace: w },
                    ),
                    Row::Tab(w, t) | Row::Pane(w, t, _) => (
                        format!("Close tab {} of \"{}\"?", t + 1, ws_name(w)),
                        Cmd::CloseTab {
                            workspace: w,
                            tab: tab_id(ctx, w, t).unwrap_or_default(),
                        },
                    ),
                };
                return Outcome::cmd(
                    false,
                    Cmd::Confirm {
                        message,
                        then: Box::new(then),
                    },
                );
            }
            _ => {}
        }
        Outcome::STAY
    }

    fn layout(screen: Rect) -> (Rect, Rect, Rect, Rect) {
        let outer = Rect::new(
            screen.x + 1,
            screen.y + 1,
            screen.width.saturating_sub(2),
            screen.height.saturating_sub(2),
        );
        let inner = Rect::new(
            outer.x + 1,
            outer.y + 1,
            outer.width.saturating_sub(2),
            outer.height.saturating_sub(2),
        );
        // Narrow screens get the list alone; the preview needs room to help.
        let list_w = if inner.width < 60 {
            inner.width
        } else {
            (inner.width * 2 / 5).clamp(24, 48)
        };
        let body_h = inner.height.saturating_sub(2);
        let list = Rect::new(inner.x, inner.y + 1, list_w, body_h.saturating_sub(1));
        let preview = Rect::new(
            inner.x + list_w + 1,
            inner.y,
            inner.width.saturating_sub(list_w + 1),
            body_h,
        );
        let footer = Rect::new(inner.x, inner.bottom().saturating_sub(1), inner.width, 1);
        (outer, list, preview, footer)
    }

    pub fn handle_mouse(&mut self, ev: &MouseEvent, ctx: &Ctx, screen: Rect) -> Outcome {
        let (_, list, _, _) = Self::layout(screen);
        let rows = self.rows(ctx);
        let i = self.index(&rows, ctx);
        match ev.kind {
            MouseEventKind::ScrollUp => self.select(&rows, i.saturating_sub(1)),
            MouseEventKind::ScrollDown => {
                self.select(&rows, (i + 1).min(rows.len().saturating_sub(1)))
            }
            MouseEventKind::Down(MouseButton::Left) if contains(list, ev.column, ev.row) => {
                let sel = self.index(&rows, ctx);
                let idx = super::list_scroll(sel, usize::from(list.height))
                    + usize::from(ev.row - list.y);
                if let Some(&row) = rows.get(idx) {
                    let double = self.last_click.is_some_and(|(t, y)| {
                        y == ev.row && t.elapsed() < Duration::from_millis(400)
                    });
                    self.selected = Some(row);
                    self.last_click = Some((Instant::now(), ev.row));
                    if double {
                        return self.open(row, ctx);
                    }
                }
            }
            _ => {}
        }
        Outcome::STAY
    }

    pub fn render(&self, ctx: &Ctx, screen: Rect, buf: &mut Buffer) {
        let theme = ctx.theme;
        let (outer, list, preview, footer) = Self::layout(screen);
        if list.height == 0 || list.width < 10 {
            return;
        }
        panel(buf, outer, "Session Manager", theme);
        let base = Style::default().fg(theme.menu_fg).bg(theme.menu_bg);
        let dim = base.add_modifier(Modifier::DIM);
        let accent = base.fg(theme.accent);
        let hl = Style::default()
            .fg(theme.menu_highlight_fg)
            .bg(theme.menu_highlight_bg);

        // Header: filter or counts.
        let header_y = list.y - 1;
        match &self.filter {
            Some(f) => {
                put(buf, list.x, header_y, "Filter:", 7, accent);
                f.render(
                    buf,
                    list.x + 8,
                    header_y,
                    list.width.saturating_sub(8),
                    base,
                );
            }
            None => {
                let tabs: usize = ctx.workspaces.iter().map(|w| w.tabs.len()).sum();
                let text = format!(
                    "{} workspaces · {} tabs · {} panes · ● = here",
                    ctx.workspaces.len(),
                    tabs,
                    ctx.panes.len()
                );
                put(buf, list.x, header_y, &text, list.width, dim);
            }
        }

        let rows = self.rows(ctx);
        let sel = self.index(&rows, ctx);
        let height = usize::from(list.height);
        let scroll = super::list_scroll(sel, height);
        for (i, row) in rows.iter().enumerate().skip(scroll).take(height) {
            let y = list.y + (i - scroll) as u16;
            let style = if i == sel { hl } else { base };
            buf.set_style(Rect::new(list.x, y, list.width, 1), style);
            if *row == Row::NewHere {
                let dir = ctx.launch_cwd.map(crate::shorten_home).unwrap_or_default();
                let room = usize::from(list.width).saturating_sub(19);
                let text = format!("+ New workspace in {}", ellipsize_start(&dir, room));
                let st = if i == sel {
                    style
                } else {
                    style.fg(theme.accent)
                };
                put(
                    buf,
                    list.x,
                    y,
                    &text,
                    list.width,
                    st.add_modifier(Modifier::BOLD),
                );
                continue;
            }
            let Some(ws) = ctx
                .workspaces
                .iter()
                .find(|w| Some(w.id) == row.workspace())
            else {
                continue;
            };
            let (text, meta) = match *row {
                Row::NewHere => continue,
                Row::Workspace(w) => {
                    let arrow = if self.expanded_ws.contains(&w) {
                        "▾"
                    } else {
                        "▸"
                    };
                    let dot = if w == ctx.current { "●" } else { " " };
                    let n = ws.tabs.len();
                    let number = ctx
                        .workspaces
                        .iter()
                        .position(|x| x.id == w)
                        .filter(|&i| i < 9)
                        .map_or("  ".to_owned(), |i| format!("{} ", i + 1));
                    // Who else is looking at it (this client counts for the current one).
                    let viewers = ctx
                        .attached
                        .get(&w)
                        .copied()
                        .unwrap_or(0)
                        .saturating_sub(usize::from(w == ctx.current));
                    // Where it lives, so sessions are easy to tell apart.
                    let dir = ctx
                        .panes
                        .get(&ws.active().active_pane)
                        .map(|p| crate::shorten_home(&p.cwd()))
                        .unwrap_or_default();
                    let mut meta = format!("{dir} · {n} tab{}", if n == 1 { "" } else { "s" });
                    if viewers > 0 {
                        meta.push_str(&format!(" · {viewers} attached"));
                    }
                    (format!("{arrow} {number}{dot} {}", ws.name), meta)
                }
                Row::Tab(w, t) => {
                    let tab = &ws.tabs[t];
                    let arrow = if self.expanded_tabs.contains(&(w, t)) {
                        "▾"
                    } else {
                        "▸"
                    };
                    let star = if t == ws.active_tab { "*" } else { " " };
                    let n = tab.layout.panes().len();
                    (
                        format!(
                            "    {arrow} {}{star} {}",
                            t + 1,
                            crate::tab_title(tab, ctx.panes)
                        ),
                        format!("{n} pane{}", if n == 1 { "" } else { "s" }),
                    )
                }
                Row::Pane(_, t, p) => {
                    let tab = &ws.tabs[t];
                    let n = tab
                        .layout
                        .panes()
                        .iter()
                        .position(|&x| x == p)
                        .map_or(0, |i| i + 1);
                    let title = ctx.panes.get(&p).map(crate::pane_title).unwrap_or_default();
                    let active = if tab.active_pane == p { "›" } else { " " };
                    (format!("        {active}{n} {title}"), format!("%{p}"))
                }
            };
            // Details never take more than half the row, so names stay readable.
            let meta = ellipsize_start(&meta, usize::from(list.width / 2));
            let meta_w = meta.chars().count() as u16;
            put(
                buf,
                list.x,
                y,
                &text,
                list.width.saturating_sub(meta_w + 1),
                style,
            );
            put(
                buf,
                list.right().saturating_sub(meta_w + 1),
                y,
                &meta,
                meta_w,
                if i == sel { style } else { dim },
            );
        }

        // Preview of the selected (or the workspace's active) tab, live.
        if preview.width >= 10
            && let Some(&row) = rows.get(sel)
            && let Some(ws) = ctx
                .workspaces
                .iter()
                .find(|w| Some(w.id) == row.workspace())
        {
            let t = match row {
                Row::Tab(_, t) | Row::Pane(_, t, _) => t,
                Row::Workspace(_) | Row::NewHere => ws.active_tab,
            };
            let tab = &ws.tabs[t];
            let focus = match row {
                Row::Pane(_, _, p) => p,
                _ => tab.active_pane,
            };
            let title = format!(
                "{} › {} {}",
                ws.name,
                t + 1,
                crate::tab_title(tab, ctx.panes)
            );
            let inner = super::panel(buf, preview, &title, theme);
            let (rects, seps) = tab.layout.compute(inner);
            for (pane_id, rect) in &rects {
                if let Some(pane) = ctx.panes.get(pane_id) {
                    pane.emulator.render(*rect, buf);
                }
            }
            draw_separators(buf, inner, &seps, &rects, focus, theme);
            if let Some(pane) = ctx.panes.get(&focus) {
                let info = format!(" {} ", crate::shorten_home(&pane.cwd()));
                let w = (info.chars().count() as u16).min(preview.width.saturating_sub(4));
                put(buf, preview.x + 2, preview.bottom() - 1, &info, w, accent);
            }
        }

        if preview.width >= 10 && rows.get(sel) == Some(&Row::NewHere) {
            let inner = super::panel(buf, preview, "New workspace", theme);
            let dir = ctx.launch_cwd.map(crate::shorten_home).unwrap_or_default();
            let lines = [
                format!("Start a new workspace in {dir}"),
                String::new(),
                "Enter  create it, named after the folder".to_owned(),
                "n      create it with a name you choose".to_owned(),
                "1-9    or jump to an existing workspace".to_owned(),
            ];
            for (k, line) in lines.iter().enumerate() {
                put(
                    buf,
                    inner.x + 1,
                    inner.y + 1 + k as u16,
                    line,
                    inner.width.saturating_sub(2),
                    base,
                );
            }
        }

        // Footer: every operation, always visible.
        let mut x = footer.x;
        for (key, what) in FOOTER {
            let k = format!(" {key} ");
            let w = format!("{what}  ");
            let need = (k.chars().count() + w.chars().count()) as u16;
            if x + need > footer.right() {
                break;
            }
            put(buf, x, footer.y, &k, need, hl);
            x += k.chars().count() as u16;
            put(buf, x, footer.y, &w, need, dim);
            x += w.chars().count() as u16;
        }
    }
}

/// Keeps the end of `s` (the useful part of a path) within `max` chars.
fn ellipsize_start(s: &str, max: usize) -> String {
    let n = s.chars().count();
    if n <= max || max == 0 {
        return if max == 0 {
            String::new()
        } else {
            s.to_owned()
        };
    }
    let tail: String = s.chars().skip(n - (max - 1)).collect();
    format!("…{tail}")
}

#[cfg(test)]
mod tests {
    use super::ellipsize_start;

    #[test]
    fn ellipsize_keeps_the_end() {
        assert_eq!(ellipsize_start("~/code/api", 20), "~/code/api");
        assert_eq!(ellipsize_start("/very/long/path/to/api", 8), "…/to/api");
        assert_eq!(ellipsize_start("abc", 0), "");
    }
}
