//! Identity and presentation of an [`Action`]: its registry id, info row and
//! human label.

use super::{Action, ActionInfo, Category, RunIn, action_info};

impl Action {
    pub fn id(&self) -> &'static str {
        use Action::*;
        match self {
            NewTab => "new-tab",
            CloseTab => "close-tab",
            RenameTab => "rename-tab",
            NextTab => "next-tab",
            PrevTab => "prev-tab",
            GoToTab { .. } => "go-to-tab",
            MoveTab { .. } => "move-tab",
            SplitPane { .. } => "split-pane",
            ClosePane => "close-pane",
            MoveFocus { .. } => "move-focus",
            ResizePane { .. } => "resize-pane",
            TogglePaneZoom => "toggle-pane-zoom",
            SwapPane { .. } => "swap-pane",
            ApplyLayout { .. } => "apply-layout",
            ToggleSyncInput => "toggle-sync-input",
            SelectMode => "select-mode",
            Find => "find",
            FindNext => "find-next",
            FindPrev => "find-prev",
            Copy => "copy",
            Paste => "paste",
            CopyAndExit => "copy-and-exit",
            SelectMove { .. } => "select-move",
            SelectExtend { .. } => "select-extend",
            SelectLineStart => "select-line-start",
            SelectLineEnd => "select-line-end",
            SelectStart => "select-start",
            SelectLine => "select-line",
            SelectWordLeft => "select-word-left",
            SelectWordRight => "select-word-right",
            ScrollUp { .. } => "scroll-up",
            ScrollDown { .. } => "scroll-down",
            ScrollPageUp => "scroll-page-up",
            ScrollPageDown => "scroll-page-down",
            ScrollToTop => "scroll-to-top",
            ScrollToBottom => "scroll-to-bottom",
            CommandPalette => "command-palette",
            OpenMenu => "open-menu",
            ShowHelp => "show-help",
            WorkspaceSwitcher => "workspace-switcher",
            NewWorkspace => "new-workspace",
            RenameWorkspace => "rename-workspace",
            NextWorkspace => "next-workspace",
            PrevWorkspace => "prev-workspace",
            Detach => "detach",
            Quit => "quit",
            ReloadConfig => "reload-config",
            OpenConfig => "open-config",
            OpenEditor => "open-editor",
            EnterMode { .. } => "enter-mode",
            ExitMode => "exit-mode",
            Lock => "lock",
            Run { .. } => "run",
            SendKeys { .. } => "send-keys",
        }
    }

    pub fn info(&self) -> &'static ActionInfo {
        action_info(self.id()).expect("every action id has a registry row")
    }

    pub fn category(&self) -> Category {
        self.info().category
    }

    /// Human label including arguments, e.g. "Split pane right", "Go to tab 3".
    pub fn label(&self) -> String {
        use Action::*;
        match self {
            GoToTab { index } => format!("Go to tab {index}"),
            MoveTab { delta } => {
                let side = if *delta < 0 { "left" } else { "right" };
                match delta.unsigned_abs() {
                    1 => format!("Move tab {side}"),
                    n => format!("Move tab {side} by {n}"),
                }
            }
            SplitPane {
                direction,
                size,
                command,
            } => {
                let mut s = format!("Split pane {}", direction.as_str());
                if let Some(size) = size {
                    s.push_str(&format!(" ({size}%)"));
                }
                if let Some(cmd) = command {
                    s.push_str(&format!(" running `{cmd}`"));
                }
                s
            }
            MoveFocus { direction } => format!("Move focus {}", direction.as_str()),
            ResizePane { direction, amount } => {
                format!("Resize pane {} by {amount}", direction.as_str())
            }
            SwapPane { direction } => format!("Swap pane {}", direction.as_str()),
            ApplyLayout { name } if name == "next" => "Next layout".into(),
            ApplyLayout { name } => format!("Layout: {name}"),
            SelectMove { direction } => format!("Move cursor {}", direction.as_str()),
            SelectExtend { direction } => format!("Extend selection {}", direction.as_str()),
            ScrollUp { lines } => format!("Scroll up {lines} lines"),
            ScrollDown { lines } => format!("Scroll down {lines} lines"),
            EnterMode { mode, .. } => format!("Enter {mode} mode"),
            Run { command, r#in } => {
                let place = match r#in {
                    RunIn::NewTab => "in new tab",
                    RunIn::SplitRight => "in split right",
                    RunIn::SplitDown => "in split down",
                    RunIn::Background => "in background",
                };
                format!("Run `{command}` {place}")
            }
            SendKeys { keys } => format!("Send keys `{keys}`"),
            _ => self.info().label.to_string(),
        }
    }
}
