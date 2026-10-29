//! Maps an action id plus its arguments to an [`Action`] variant.

use std::ops::Range;

use super::args::Args;
use crate::action::{Action, Dir, RunIn, all_action_ids};
use crate::diag::Diag;

pub(super) fn build(id: &str, id_span: Range<usize>, a: &mut Args<'_, '_>) -> Result<Action, Diag> {
    use Action::*;
    Ok(match id {
        "new-tab" => NewTab,
        "close-tab" => CloseTab,
        "rename-tab" => RenameTab,
        "next-tab" => NextTab,
        "prev-tab" => PrevTab,
        "go-to-tab" => GoToTab {
            index: a.req_int("index", 1, 999, "1")? as usize,
        },
        "move-tab" => MoveTab {
            delta: a.req_int("delta", -999, 999, "1")? as i32,
        },
        "split-pane" => SplitPane {
            direction: a.dir("direction")?.unwrap_or(Dir::Right),
            size: a.size("size")?,
            command: a.string("command")?.map(|(s, _)| s),
        },
        "close-pane" => ClosePane,
        "move-focus" => MoveFocus {
            direction: a.req_dir("direction")?,
        },
        "resize-pane" => ResizePane {
            direction: a.req_dir("direction")?,
            amount: a.int("amount", 1, 500)?.unwrap_or(2) as u16,
        },
        "toggle-pane-zoom" => TogglePaneZoom,
        "swap-pane" => SwapPane {
            direction: a.req_dir("direction")?,
        },
        "apply-layout" => ApplyLayout {
            name: a.req_string("name", "\"tiled\"")?,
        },
        "toggle-sync-input" => ToggleSyncInput,
        "select-mode" => SelectMode,
        "find" => Find,
        "find-next" => FindNext,
        "find-prev" => FindPrev,
        "copy" => Copy,
        "paste" => Paste,
        "copy-and-exit" => CopyAndExit,
        "select-move" => SelectMove {
            direction: a.req_dir("direction")?,
        },
        "select-extend" => SelectExtend {
            direction: a.req_dir("direction")?,
        },
        "select-line-start" => SelectLineStart,
        "select-line-end" => SelectLineEnd,
        "select-start" => SelectStart,
        "select-line" => SelectLine,
        "select-word-left" => SelectWordLeft,
        "select-word-right" => SelectWordRight,
        "scroll-up" => ScrollUp {
            lines: a.int("lines", 1, 10_000)?.unwrap_or(3) as u16,
        },
        "scroll-down" => ScrollDown {
            lines: a.int("lines", 1, 10_000)?.unwrap_or(3) as u16,
        },
        "scroll-page-up" => ScrollPageUp,
        "scroll-page-down" => ScrollPageDown,
        "scroll-to-top" => ScrollToTop,
        "scroll-to-bottom" => ScrollToBottom,
        "command-palette" => CommandPalette,
        "open-menu" => OpenMenu,
        "show-help" => ShowHelp,
        "workspace-switcher" => WorkspaceSwitcher,
        "new-workspace" => NewWorkspace,
        "rename-workspace" => RenameWorkspace,
        "next-workspace" => NextWorkspace,
        "prev-workspace" => PrevWorkspace,
        "detach" => Detach,
        "quit" => Quit,
        "reload-config" => ReloadConfig,
        "open-config" => OpenConfig,
        "enter-mode" => EnterMode {
            mode: a.req_string("mode", "\"resize\"")?,
            sticky: a.bool("sticky")?.unwrap_or(false),
        },
        "exit-mode" => ExitMode,
        "lock" => Lock,
        "run" => Run {
            command: a.req_string("command", "\"htop\"")?,
            r#in: a.run_in("in")?.unwrap_or(RunIn::NewTab),
        },
        "send-keys" => SendKeys {
            keys: a.req_string("keys", "\"ls\\r\"")?,
        },
        _ => {
            return Err(Diag::new(id_span, format!("unknown action \"{id}\""))
                .suggest(id, all_action_ids().iter().copied()));
        }
    })
}
