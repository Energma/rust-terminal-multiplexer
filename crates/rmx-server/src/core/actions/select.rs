//! Select-mode actions: find, copy/paste, cursor movement and scrolling.

use anyhow::Result;
use ratatui::layout::Rect;
use rmx_config::action::Action;
use rmx_core::layout::PaneId;
use rmx_proto::ServerMsg;

use crate::core::{ClientId, Core};
use crate::ui::PromptThen;

impl Core {
    pub(super) fn select_action(
        &mut self,
        id: ClientId,
        pane: PaneId,
        area: Rect,
        action: Action,
    ) -> Result<()> {
        match action {
            Action::SelectMode => self.enter_select(id, pane),
            Action::Find => self.push_prompt(id, "Find", String::new(), PromptThen::Find),
            Action::FindNext | Action::FindPrev => {
                let forward = action == Action::FindPrev;
                self.with_select(id, pane, |sel, emu, _| {
                    if let Some(q) = sel.search.clone() {
                        sel.find(&q, forward, emu);
                    }
                });
            }
            Action::Copy | Action::CopyAndExit => {
                self.copy_selection(id);
                if action == Action::CopyAndExit {
                    self.exit_select(id);
                }
            }
            Action::Paste => {
                if let Some(c) = self.clients.get(&id) {
                    let _ = c.tx.try_send(ServerMsg::GetClipboard);
                }
            }
            Action::SelectMove { direction } => {
                self.with_select(id, pane, |sel, emu, cols| {
                    sel.move_cursor(direction, cols, emu)
                });
            }
            Action::SelectExtend { direction } => {
                self.with_select(id, pane, |sel, emu, cols| sel.extend(direction, cols, emu));
            }
            Action::SelectLineStart => self.with_select(id, pane, |sel, _, _| sel.line_start()),
            Action::SelectLineEnd => self.with_select(id, pane, |sel, emu, _| sel.line_end(emu)),
            Action::SelectStart => self.with_select(id, pane, |sel, _, _| sel.start(false)),
            Action::SelectLine => self.with_select(id, pane, |sel, _, _| sel.start(true)),
            Action::SelectWordLeft => {
                self.with_select(id, pane, |sel, emu, _| sel.move_word(false, emu))
            }
            Action::SelectWordRight => {
                self.with_select(id, pane, |sel, emu, _| sel.move_word(true, emu))
            }
            Action::ScrollUp { lines } => {
                self.scroll(id, pane, isize::from(lines as i16).saturating_neg())
            }
            Action::ScrollDown { lines } => self.scroll(id, pane, lines as isize),
            Action::ScrollPageUp => self.scroll(id, pane, -(isize::from(area.height as i16) - 1)),
            Action::ScrollPageDown => self.scroll(id, pane, isize::from(area.height as i16) - 1),
            Action::ScrollToTop => self.with_select(id, pane, |sel, emu, _| sel.go_top(emu)),
            Action::ScrollToBottom => self.with_select(id, pane, |sel, emu, _| sel.go_bottom(emu)),
            _ => unreachable!("routed by dispatch"),
        }
        Ok(())
    }
}
