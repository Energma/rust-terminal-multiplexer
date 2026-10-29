//! The open menu bar overlay: which menu is dropped down, its selection and
//! where the drop-down sits.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

use super::items::{render_items, step_entry};
use super::{Item, menus, title_positions};
use crate::ui::Ctx;

pub struct MenuBar {
    pub open: usize,
    pub(super) selected: usize,
}

impl MenuBar {
    pub fn new(open: usize) -> Self {
        let mut bar = Self { open, selected: 0 };
        bar.selected = bar.first_entry();
        bar
    }

    pub(super) fn first_entry(&self) -> usize {
        let menus = menus();
        menus[self.open]
            .items
            .iter()
            .position(|i| matches!(i, Item::Entry(..)))
            .unwrap_or(0)
    }

    pub(super) fn step(&mut self, delta: isize) {
        let menus = menus();
        self.selected = step_entry(&menus[self.open].items, self.selected, delta);
    }

    pub(super) fn switch(&mut self, delta: isize) {
        let n = menus().len() as isize;
        self.open = ((self.open as isize + delta).rem_euclid(n)) as usize;
        self.selected = self.first_entry();
    }

    pub(super) fn dropdown_rect(&self, ctx: &Ctx) -> Rect {
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

    pub fn render(&self, ctx: &Ctx, buf: &mut Buffer) {
        let rect = self.dropdown_rect(ctx).intersection(buf.area);
        let menus = menus();
        render_items(buf, rect, ctx, &menus[self.open].items, self.selected, "");
    }
}
