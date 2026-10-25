//! The Windows-style menu bar (F10 or click) and the right-click context menu.

use crossterm::event::{KeyCode, KeyEvent, MouseButton, MouseEvent, MouseEventKind};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use rmx_config::action::{Action, Dir};

use super::{Ctx, Outcome, contains, panel, put};

pub enum Item {
    Entry(&'static str, Action),
    Separator,
}

pub struct Menu {
    pub title: &'static str,
    pub items: Vec<Item>,
}

fn split(direction: Dir) -> Action {
    Action::SplitPane {
        direction,
        size: None,
        command: None,
    }
}

fn layout(name: &str) -> Action {
    Action::ApplyLayout { name: name.into() }
}

/// The menu bar. Labels use Windows Terminal / VS Code vocabulary.
pub fn menus() -> Vec<Menu> {
    use Item::{Entry, Separator};
    vec![
        Menu {
            title: "File",
            items: vec![
                Entry("New Tab", Action::NewTab),
                Entry("New Workspace", Action::NewWorkspace),
                Separator,
                Entry("Close Pane", Action::ClosePane),
                Entry("Close Tab", Action::CloseTab),
                Separator,
                Entry("Leave (keep running)", Action::Detach),
                Entry("Quit rmx", Action::Quit),
            ],
        },
        Menu {
            title: "Edit",
            items: vec![
                Entry("Select / Scroll Mode", Action::SelectMode),
                Entry("Copy", Action::Copy),
                Entry("Paste", Action::Paste),
                Entry("Find…", Action::Find),
                Separator,
                Entry("Settings (config.toml)", Action::OpenConfig),
                Entry("Reload Settings", Action::ReloadConfig),
            ],
        },
        Menu {
            title: "View",
            items: vec![
                Entry("Command Palette…", Action::CommandPalette),
                Entry("Session Manager…", Action::WorkspaceSwitcher),
                Separator,
                Entry("Zoom Pane", Action::TogglePaneZoom),
                Entry("Type in All Panes", Action::ToggleSyncInput),
                Separator,
                Entry("Layout: Side by Side", layout("even-horizontal")),
                Entry("Layout: Stacked", layout("even-vertical")),
                Entry("Layout: Main Left", layout("main-left")),
                Entry("Layout: Main Top", layout("main-top")),
                Entry("Layout: Grid", layout("tiled")),
            ],
        },
        Menu {
            title: "Tab",
            items: vec![
                Entry("New Tab", Action::NewTab),
                Entry("Rename Tab…", Action::RenameTab),
                Separator,
                Entry("Next Tab", Action::NextTab),
                Entry("Previous Tab", Action::PrevTab),
                Entry("Move Tab Left", Action::MoveTab { delta: -1 }),
                Entry("Move Tab Right", Action::MoveTab { delta: 1 }),
                Separator,
                Entry("Close Tab", Action::CloseTab),
            ],
        },
        Menu {
            title: "Pane",
            items: vec![
                Entry("Split Right", split(Dir::Right)),
                Entry("Split Down", split(Dir::Down)),
                Entry("Split Left", split(Dir::Left)),
                Entry("Split Up", split(Dir::Up)),
                Separator,
                Entry(
                    "Focus Left",
                    Action::MoveFocus {
                        direction: Dir::Left,
                    },
                ),
                Entry(
                    "Focus Right",
                    Action::MoveFocus {
                        direction: Dir::Right,
                    },
                ),
                Entry("Focus Up", Action::MoveFocus { direction: Dir::Up }),
                Entry(
                    "Focus Down",
                    Action::MoveFocus {
                        direction: Dir::Down,
                    },
                ),
                Entry(
                    "Swap with Right",
                    Action::SwapPane {
                        direction: Dir::Right,
                    },
                ),
                Entry(
                    "Resize Mode",
                    Action::EnterMode {
                        mode: "resize".into(),
                        sticky: true,
                    },
                ),
                Separator,
                Entry("Close Pane", Action::ClosePane),
            ],
        },
        Menu {
            title: "Workspace",
            items: vec![
                Entry("Session Manager…", Action::WorkspaceSwitcher),
                Entry("New Workspace", Action::NewWorkspace),
                Entry("Rename Workspace…", Action::RenameWorkspace),
                Separator,
                Entry("Next Workspace", Action::NextWorkspace),
                Entry("Previous Workspace", Action::PrevWorkspace),
                Separator,
                Entry("Lock Keys (send all to pane)", Action::Lock),
                Entry("Leave (keep running)", Action::Detach),
            ],
        },
        Menu {
            title: "Help",
            items: vec![
                Entry("Keyboard Shortcuts", Action::ShowHelp),
                Entry("Command Palette…", Action::CommandPalette),
                Entry("Settings (config.toml)", Action::OpenConfig),
            ],
        },
    ]
}

/// Column and width of each menu title in the bar; shared by drawing and
/// mouse hit-testing so they can't disagree.
pub fn title_positions(menus: &[Menu]) -> Vec<(u16, u16)> {
    let mut x = 7; // after the " ≡ rmx " badge
    menus
        .iter()
        .map(|m| {
            let w = m.title.len() as u16 + 2;
            let pos = (x, w);
            x += w;
            pos
        })
        .collect()
}

/// Draws the menu bar row; `open` highlights the active title.
pub fn render_bar(buf: &mut Buffer, row: Rect, ctx: &Ctx, open: Option<usize>, workspace: &str) {
    let theme = ctx.theme;
    let base = Style::default().fg(theme.status_fg).bg(theme.status_bg);
    buf.set_style(row, base);
    put(
        buf,
        row.x,
        row.y,
        " ≡ rmx",
        6,
        base.fg(theme.accent).add_modifier(Modifier::BOLD),
    );
    let menus = menus();
    for (i, (m, (x, w))) in menus.iter().zip(title_positions(&menus)).enumerate() {
        let style = if open == Some(i) {
            Style::default()
                .fg(theme.menu_highlight_fg)
                .bg(theme.menu_highlight_bg)
        } else {
            base
        };
        put(buf, row.x + x, row.y, &format!(" {} ", m.title), w, style);
        // Underline the accelerator letter, as Windows does.
        if let Some(cell) = buf.cell_mut((row.x + x + 1, row.y)) {
            cell.set_style(style.add_modifier(Modifier::UNDERLINED));
        }
    }
    let right = format!(" {workspace} ");
    let rw = right.chars().count() as u16;
    if rw + 60 < row.width {
        put(
            buf,
            row.right() - rw,
            row.y,
            &right,
            rw,
            base.fg(theme.accent),
        );
    }
}

pub struct MenuBar {
    pub open: usize,
    selected: usize,
}

impl MenuBar {
    pub fn new(open: usize) -> Self {
        let mut bar = Self { open, selected: 0 };
        bar.selected = bar.first_entry();
        bar
    }

    fn first_entry(&self) -> usize {
        let menus = menus();
        menus[self.open]
            .items
            .iter()
            .position(|i| matches!(i, Item::Entry(..)))
            .unwrap_or(0)
    }

    fn step(&mut self, delta: isize) {
        let menus = menus();
        let items = &menus[self.open].items;
        let n = items.len() as isize;
        let mut i = self.selected as isize;
        for _ in 0..n {
            i = (i + delta).rem_euclid(n);
            if matches!(items[i as usize], Item::Entry(..)) {
                break;
            }
        }
        self.selected = i as usize;
    }

    fn switch(&mut self, delta: isize) {
        let n = menus().len() as isize;
        self.open = ((self.open as isize + delta).rem_euclid(n)) as usize;
        self.selected = self.first_entry();
    }

    fn dropdown_rect(&self, ctx: &Ctx) -> Rect {
        let menus = menus();
        let (x, _) = title_positions(&menus)[self.open];
        let items = &menus[self.open].items;
        let width = items
            .iter()
            .map(|i| match i {
                Item::Entry(label, action) => {
                    label.chars().count()
                        + ctx
                            .keymap
                            .key_hint(action)
                            .map_or(0, |h| h.chars().count() + 3)
                }
                Item::Separator => 0,
            })
            .max()
            .unwrap_or(10) as u16
            + 4;
        Rect::new(x, 1, width.max(18), items.len() as u16 + 2)
    }

    pub fn handle_key(&mut self, key: &KeyEvent, _ctx: &Ctx) -> Outcome {
        match key.code {
            KeyCode::Esc | KeyCode::F(10) => Outcome::CLOSE,
            KeyCode::Up => {
                self.step(-1);
                Outcome::STAY
            }
            KeyCode::Down => {
                self.step(1);
                Outcome::STAY
            }
            KeyCode::Left => {
                self.switch(-1);
                Outcome::STAY
            }
            KeyCode::Right | KeyCode::Tab => {
                self.switch(1);
                Outcome::STAY
            }
            KeyCode::Enter | KeyCode::Char(' ') => match &menus()[self.open].items[self.selected] {
                Item::Entry(_, action) => Outcome::run(action.clone()),
                Item::Separator => Outcome::STAY,
            },
            // Accelerators: a letter jumps to the menu or item starting with it.
            KeyCode::Char(c) => {
                let c = c.to_ascii_lowercase();
                let menus = menus();
                let hit = menus[self.open].items.iter().position(|i| {
                    matches!(i, Item::Entry(label, _) if label.to_ascii_lowercase().starts_with(c))
                });
                match hit {
                    Some(i) => match &menus[self.open].items[i] {
                        Item::Entry(_, action) => Outcome::run(action.clone()),
                        Item::Separator => Outcome::STAY,
                    },
                    None => {
                        if let Some(m) = menus
                            .iter()
                            .position(|m| m.title.to_ascii_lowercase().starts_with(c))
                        {
                            self.open = m;
                            self.selected = self.first_entry();
                        }
                        Outcome::STAY
                    }
                }
            }
            _ => Outcome::STAY,
        }
    }

    pub fn handle_mouse(&mut self, ev: &MouseEvent, ctx: &Ctx) -> Outcome {
        let (x, y) = (ev.column, ev.row);
        let menus = menus();
        let titles = title_positions(&menus);
        match ev.kind {
            MouseEventKind::Down(MouseButton::Left)
            | MouseEventKind::Moved
            | MouseEventKind::Drag(_) => {
                if y == 0 {
                    if let Some(i) = titles.iter().position(|&(tx, w)| x >= tx && x < tx + w) {
                        if i == self.open && matches!(ev.kind, MouseEventKind::Down(_)) {
                            return Outcome::CLOSE;
                        }
                        self.open = i;
                        self.selected = self.first_entry();
                    } else if matches!(ev.kind, MouseEventKind::Down(_)) {
                        return Outcome::CLOSE;
                    }
                    return Outcome::STAY;
                }
                let rect = self.dropdown_rect(ctx);
                if contains(rect, x, y) {
                    let idx = if y > rect.y {
                        usize::from(y - rect.y - 1)
                    } else {
                        usize::MAX
                    };
                    if idx < menus[self.open].items.len()
                        && matches!(menus[self.open].items[idx], Item::Entry(..))
                    {
                        self.selected = idx;
                    }
                    Outcome::STAY
                } else if matches!(ev.kind, MouseEventKind::Down(_)) {
                    Outcome::CLOSE
                } else {
                    Outcome::STAY
                }
            }
            MouseEventKind::Up(MouseButton::Left) => {
                let rect = self.dropdown_rect(ctx);
                if contains(rect, x, y) {
                    let idx = if y > rect.y {
                        usize::from(y - rect.y - 1)
                    } else {
                        usize::MAX
                    };
                    if let Some(Item::Entry(_, action)) = menus[self.open].items.get(idx) {
                        return Outcome::run(action.clone());
                    }
                }
                Outcome::STAY
            }
            _ => Outcome::STAY,
        }
    }

    pub fn render(&self, ctx: &Ctx, buf: &mut Buffer) {
        let rect = self.dropdown_rect(ctx).intersection(buf.area);
        let menus = menus();
        render_items(buf, rect, ctx, &menus[self.open].items, self.selected, "");
    }
}

fn render_items(
    buf: &mut Buffer,
    rect: Rect,
    ctx: &Ctx,
    items: &[Item],
    selected: usize,
    title: &str,
) {
    let theme = ctx.theme;
    let inner = panel(buf, rect, title, theme);
    let base = Style::default().fg(theme.menu_fg).bg(theme.menu_bg);
    let hl = Style::default()
        .fg(theme.menu_highlight_fg)
        .bg(theme.menu_highlight_bg);
    for (i, item) in items.iter().enumerate() {
        let y = inner.y + i as u16;
        if y >= inner.bottom() {
            break;
        }
        match item {
            Item::Separator => {
                put(
                    buf,
                    inner.x,
                    y,
                    &"─".repeat(usize::from(inner.width)),
                    inner.width,
                    base.fg(theme.border),
                );
            }
            Item::Entry(label, action) => {
                let style = if i == selected { hl } else { base };
                buf.set_style(Rect::new(inner.x, y, inner.width, 1), style);
                // Labels give way to the key hint rather than run under it.
                let hint = ctx.keymap.key_hint(action);
                let hint_w = hint.as_ref().map_or(0, |h| h.chars().count() as u16 + 2);
                let label_w = if hint_w + 2 < inner.width {
                    inner.width - 1 - hint_w
                } else {
                    inner.width.saturating_sub(1)
                };
                put(buf, inner.x + 1, y, label, label_w, style);
                if let Some(hint) = hint {
                    let w = hint.chars().count() as u16;
                    if w + 2 < inner.width {
                        put(
                            buf,
                            inner.right() - w - 1,
                            y,
                            &hint,
                            w,
                            style.add_modifier(Modifier::DIM),
                        );
                    }
                }
            }
        }
    }
}

/// Right-click menu for a pane.
pub struct ContextMenu {
    x: u16,
    y: u16,
    items: Vec<Item>,
    selected: usize,
}

impl ContextMenu {
    pub fn new(x: u16, y: u16, has_selection: bool) -> Self {
        use Item::{Entry, Separator};
        let mut items = vec![
            Entry("Split Right", split(Dir::Right)),
            Entry("Split Down", split(Dir::Down)),
            Entry("Zoom Pane", Action::TogglePaneZoom),
            Separator,
        ];
        if has_selection {
            items.push(Entry("Copy", Action::Copy));
        }
        items.extend([
            Entry("Paste", Action::Paste),
            Entry("Select / Scroll Mode", Action::SelectMode),
            Separator,
            Entry("Rename Tab…", Action::RenameTab),
            Entry("Session Manager…", Action::WorkspaceSwitcher),
            Entry("Close Pane", Action::ClosePane),
        ]);
        Self {
            x,
            y,
            items,
            selected: 0,
        }
    }

    fn rect(&self, screen: Rect) -> Rect {
        // Fits the longest label plus a leader-key hint.
        let w = 36;
        let h = self.items.len() as u16 + 2;
        // Flip towards the inside near the right/bottom edges.
        let x = if self.x + w > screen.right() {
            self.x.saturating_sub(w)
        } else {
            self.x
        };
        let y = if self.y + h > screen.bottom() {
            self.y.saturating_sub(h)
        } else {
            self.y
        };
        Rect::new(x, y, w, h).intersection(screen)
    }

    fn step(&mut self, delta: isize) {
        let n = self.items.len() as isize;
        let mut i = self.selected as isize;
        for _ in 0..n {
            i = (i + delta).rem_euclid(n);
            if matches!(self.items[i as usize], Item::Entry(..)) {
                break;
            }
        }
        self.selected = i as usize;
    }

    pub fn handle_key(&mut self, key: &KeyEvent) -> Outcome {
        match key.code {
            KeyCode::Esc => Outcome::CLOSE,
            KeyCode::Up => {
                self.step(-1);
                Outcome::STAY
            }
            KeyCode::Down => {
                self.step(1);
                Outcome::STAY
            }
            KeyCode::Enter => match &self.items[self.selected] {
                Item::Entry(_, a) => Outcome::run(a.clone()),
                Item::Separator => Outcome::STAY,
            },
            _ => Outcome::STAY,
        }
    }

    pub fn handle_mouse(&mut self, ev: &MouseEvent, screen: Rect) -> Outcome {
        let rect = self.rect(screen);
        let inside = contains(rect, ev.column, ev.row);
        let idx = if ev.row > rect.y {
            usize::from(ev.row - rect.y - 1)
        } else {
            usize::MAX
        };
        match ev.kind {
            MouseEventKind::Moved | MouseEventKind::Drag(_) if inside => {
                if matches!(self.items.get(idx), Some(Item::Entry(..))) {
                    self.selected = idx;
                }
                Outcome::STAY
            }
            MouseEventKind::Up(MouseButton::Left) if inside => match self.items.get(idx) {
                Some(Item::Entry(_, a)) => Outcome::run(a.clone()),
                _ => Outcome::STAY,
            },
            MouseEventKind::Down(_) if !inside => Outcome::CLOSE,
            _ => Outcome::STAY,
        }
    }

    pub fn render(&self, ctx: &Ctx, screen: Rect, buf: &mut Buffer) {
        render_items(buf, self.rect(screen), ctx, &self.items, self.selected, "");
    }
}
