//! The action registry: every command rmx can perform, whatever triggered it.
//!
//! Keys, menus, the palette, the CLI and the API all speak [`Action`], so a new
//! feature is one variant plus one [`ActionInfo`] row. TOML parsing is hand-rolled
//! (not serde) so errors can name the exact argument and suggest fixes.

use std::ops::Range;
use std::sync::LazyLock;

use serde::{Deserialize, Serialize};
use serde_spanned::Spanned;
use toml::de::{DeTable, DeValue};

use crate::diag::Diag;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Dir {
    Left,
    Right,
    Up,
    Down,
}

impl Dir {
    pub const ALL: [Dir; 4] = [Dir::Left, Dir::Right, Dir::Up, Dir::Down];

    pub fn as_str(self) -> &'static str {
        match self {
            Dir::Left => "left",
            Dir::Right => "right",
            Dir::Up => "up",
            Dir::Down => "down",
        }
    }
}

/// Where a `run` command's process goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RunIn {
    NewTab,
    SplitRight,
    SplitDown,
    Background,
}

impl RunIn {
    const NAMES: [&'static str; 4] = ["new-tab", "split-right", "split-down", "background"];

    pub fn as_str(self) -> &'static str {
        match self {
            RunIn::NewTab => "new-tab",
            RunIn::SplitRight => "split-right",
            RunIn::SplitDown => "split-down",
            RunIn::Background => "background",
        }
    }
}

/// Built-in layout presets, plus `"next"` which cycles through them.
pub const LAYOUTS: &[&str] = &[
    "even-horizontal",
    "even-vertical",
    "main-left",
    "main-top",
    "tiled",
];

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Action {
    // tabs
    NewTab,
    CloseTab,
    RenameTab,
    NextTab,
    PrevTab,
    /// 1-based, like the numbers on the tab bar.
    GoToTab {
        index: usize,
    },
    MoveTab {
        delta: i32,
    },
    // panes
    SplitPane {
        direction: Dir,
        /// Percent of the split pane given to the new one, 1..=99.
        size: Option<u16>,
        command: Option<String>,
    },
    ClosePane,
    MoveFocus {
        direction: Dir,
    },
    ResizePane {
        direction: Dir,
        amount: u16,
    },
    TogglePaneZoom,
    SwapPane {
        direction: Dir,
    },
    ApplyLayout {
        name: String,
    },
    ToggleSyncInput,
    // select mode / clipboard / scrolling
    SelectMode,
    Find,
    FindNext,
    FindPrev,
    Copy,
    Paste,
    CopyAndExit,
    SelectMove {
        direction: Dir,
    },
    /// Shift+arrow style: moves the cursor, starting a selection if needed.
    SelectExtend {
        direction: Dir,
    },
    SelectLineStart,
    SelectLineEnd,
    SelectStart,
    SelectLine,
    SelectWordLeft,
    SelectWordRight,
    ScrollUp {
        lines: u16,
    },
    ScrollDown {
        lines: u16,
    },
    ScrollPageUp,
    ScrollPageDown,
    ScrollToTop,
    ScrollToBottom,
    // ui
    CommandPalette,
    OpenMenu,
    ShowHelp,
    WorkspaceSwitcher,
    // workspaces / session
    NewWorkspace,
    RenameWorkspace,
    NextWorkspace,
    PrevWorkspace,
    Detach,
    Quit,
    ReloadConfig,
    OpenConfig,
    // modes
    EnterMode {
        mode: String,
        sticky: bool,
    },
    ExitMode,
    Lock,
    // misc
    Run {
        command: String,
        r#in: RunIn,
    },
    SendKeys {
        keys: String,
    },
}

/// Menu-bar grouping.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Category {
    File,
    Edit,
    View,
    Tab,
    Pane,
    Workspace,
    Help,
}

/// Static registry row; `label` is the argument-free form.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ActionInfo {
    pub id: &'static str,
    pub label: &'static str,
    pub category: Category,
    pub description: &'static str,
}

const fn info(
    id: &'static str,
    label: &'static str,
    category: Category,
    description: &'static str,
) -> ActionInfo {
    ActionInfo {
        id,
        label,
        category,
        description,
    }
}

use Category as C;

/// One row per action id, in menu order.
pub const ACTIONS: &[ActionInfo] = &[
    info(
        "new-tab",
        "New tab",
        C::Tab,
        "Open a new tab with the default shell",
    ),
    info(
        "close-tab",
        "Close tab",
        C::Tab,
        "Close the current tab and all its panes",
    ),
    info(
        "rename-tab",
        "Rename tab",
        C::Tab,
        "Give the current tab a name",
    ),
    info(
        "next-tab",
        "Next tab",
        C::Tab,
        "Switch to the tab on the right",
    ),
    info(
        "prev-tab",
        "Previous tab",
        C::Tab,
        "Switch to the tab on the left",
    ),
    info(
        "go-to-tab",
        "Go to tab",
        C::Tab,
        "Switch to tab number `index` (1-based)",
    ),
    info(
        "move-tab",
        "Move tab",
        C::Tab,
        "Move the current tab `delta` places",
    ),
    info(
        "split-pane",
        "Split pane",
        C::Pane,
        "Split the focused pane in `direction`",
    ),
    info(
        "close-pane",
        "Close pane",
        C::Pane,
        "Close the focused pane",
    ),
    info(
        "move-focus",
        "Move focus",
        C::Pane,
        "Focus the neighbouring pane",
    ),
    info(
        "resize-pane",
        "Resize pane",
        C::Pane,
        "Move the focused pane's border",
    ),
    info(
        "toggle-pane-zoom",
        "Zoom pane",
        C::Pane,
        "Toggle the focused pane filling the tab",
    ),
    info(
        "swap-pane",
        "Swap pane",
        C::Pane,
        "Swap the focused pane with its neighbour",
    ),
    info(
        "apply-layout",
        "Apply layout",
        C::Pane,
        "Rearrange the tab's panes",
    ),
    info(
        "toggle-sync-input",
        "Synchronize input",
        C::Pane,
        "Type into every pane of the tab at once",
    ),
    info(
        "select-mode",
        "Select mode",
        C::Edit,
        "Scroll back and select text with the keyboard",
    ),
    info("find", "Find", C::Edit, "Search the scrollback"),
    info(
        "find-next",
        "Find next",
        C::Edit,
        "Jump to the next search match",
    ),
    info(
        "find-prev",
        "Find previous",
        C::Edit,
        "Jump to the previous search match",
    ),
    info(
        "copy",
        "Copy",
        C::Edit,
        "Copy the selection to the clipboard",
    ),
    info(
        "paste",
        "Paste",
        C::Edit,
        "Paste the clipboard into the focused pane",
    ),
    info(
        "copy-and-exit",
        "Copy and exit",
        C::Edit,
        "Copy the selection and leave select mode",
    ),
    info(
        "select-move",
        "Move cursor",
        C::Edit,
        "Move the select-mode cursor",
    ),
    info(
        "select-extend",
        "Extend selection",
        C::Edit,
        "Move the cursor, starting a selection if there is none (Shift+arrows)",
    ),
    info(
        "select-line-start",
        "Start of line",
        C::Edit,
        "Move the cursor to the start of the line",
    ),
    info(
        "select-line-end",
        "End of line",
        C::Edit,
        "Move the cursor to the end of the line",
    ),
    info(
        "select-start",
        "Start selection",
        C::Edit,
        "Start or clear a selection at the cursor",
    ),
    info("select-line", "Select line", C::Edit, "Select whole lines"),
    info(
        "select-word-left",
        "Previous word",
        C::Edit,
        "Move the cursor one word left",
    ),
    info(
        "select-word-right",
        "Next word",
        C::Edit,
        "Move the cursor one word right",
    ),
    info(
        "scroll-up",
        "Scroll up",
        C::View,
        "Scroll the pane up `lines` lines",
    ),
    info(
        "scroll-down",
        "Scroll down",
        C::View,
        "Scroll the pane down `lines` lines",
    ),
    info(
        "scroll-page-up",
        "Scroll page up",
        C::View,
        "Scroll up one screen",
    ),
    info(
        "scroll-page-down",
        "Scroll page down",
        C::View,
        "Scroll down one screen",
    ),
    info(
        "scroll-to-top",
        "Scroll to top",
        C::View,
        "Jump to the oldest scrollback line",
    ),
    info(
        "scroll-to-bottom",
        "Scroll to bottom",
        C::View,
        "Jump back to the live screen",
    ),
    info(
        "command-palette",
        "Command palette",
        C::View,
        "Search and run any action",
    ),
    info("open-menu", "Open menu", C::View, "Open the menu bar"),
    info("show-help", "Show help", C::Help, "List the key bindings"),
    info(
        "workspace-switcher",
        "Switch workspace",
        C::Workspace,
        "Pick a workspace, tab or pane",
    ),
    info(
        "new-workspace",
        "New workspace",
        C::Workspace,
        "Create a new workspace",
    ),
    info(
        "rename-workspace",
        "Rename workspace",
        C::Workspace,
        "Give the current workspace a name",
    ),
    info(
        "next-workspace",
        "Next workspace",
        C::Workspace,
        "Switch to the next workspace",
    ),
    info(
        "prev-workspace",
        "Previous workspace",
        C::Workspace,
        "Switch to the previous workspace",
    ),
    info(
        "detach",
        "Leave (keep running)",
        C::File,
        "Detach; everything keeps running in the background",
    ),
    info(
        "quit",
        "Quit",
        C::File,
        "Close every pane and stop the background service",
    ),
    info(
        "reload-config",
        "Reload config",
        C::File,
        "Re-read config.toml",
    ),
    info(
        "open-config",
        "Open config",
        C::File,
        "Edit config.toml in a new tab",
    ),
    info(
        "enter-mode",
        "Enter mode",
        C::View,
        "Switch to another key table",
    ),
    info("exit-mode", "Exit mode", C::View, "Return to normal mode"),
    info(
        "lock",
        "Lock keys",
        C::View,
        "Send every key to the pane until unlocked",
    ),
    info(
        "run",
        "Run command",
        C::File,
        "Run `command` in a new tab, split or the background",
    ),
    info(
        "send-keys",
        "Send keys",
        C::Edit,
        "Type `keys` into the focused pane",
    ),
];

static IDS: LazyLock<Vec<&'static str>> = LazyLock::new(|| ACTIONS.iter().map(|a| a.id).collect());

/// Every action id, for did-you-mean suggestions and completion.
pub fn all_action_ids() -> &'static [&'static str] {
    &IDS
}

pub fn action_info(id: &str) -> Option<&'static ActionInfo> {
    ACTIONS.iter().find(|a| a.id == id)
}

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

    /// Parses an action from a CLI/API string: a bare id (`new-tab`) or a TOML
    /// inline table (`{ action = "split-pane", direction = "down" }`).
    pub fn parse(src: &str) -> Result<Action, String> {
        let src = src.trim();
        let fmt = |d: Diag| match d.help {
            Some(h) => format!("{} ({h})", d.message),
            None => d.message,
        };
        let value = if src.starts_with('{') {
            DeValue::parse(src).map_err(|e| e.message().to_string())?
        } else {
            Spanned::new(0..src.len(), DeValue::String(src.into()))
        };
        match from_value(&value).map_err(fmt)? {
            Some(a) => Ok(a),
            None => Err("\"none\" only makes sense in a key binding".into()),
        }
    }
}

/// Every action users can pick from the command palette, with sensible default
/// arguments. Select-mode motions, scrolling, `exit-mode` and actions that need
/// free-form arguments (`run`, `send-keys`) are left out.
pub fn palette_actions() -> Vec<Action> {
    use Action::*;
    let mut v = vec![NewTab, CloseTab, RenameTab, NextTab, PrevTab];
    v.extend((1..=9).map(|index| GoToTab { index }));
    v.extend([MoveTab { delta: -1 }, MoveTab { delta: 1 }]);
    for direction in [Dir::Right, Dir::Down] {
        v.push(SplitPane {
            direction,
            size: None,
            command: None,
        });
    }
    v.push(ClosePane);
    v.extend(Dir::ALL.map(|direction| MoveFocus { direction }));
    v.extend(Dir::ALL.map(|direction| ResizePane {
        direction,
        amount: 2,
    }));
    v.push(TogglePaneZoom);
    v.extend(Dir::ALL.map(|direction| SwapPane { direction }));
    v.push(ApplyLayout {
        name: "next".into(),
    });
    v.extend(LAYOUTS.iter().map(|n| ApplyLayout { name: (*n).into() }));
    v.extend([
        ToggleSyncInput,
        SelectMode,
        Find,
        Copy,
        Paste,
        OpenMenu,
        ShowHelp,
        WorkspaceSwitcher,
        NewWorkspace,
        RenameWorkspace,
        NextWorkspace,
        PrevWorkspace,
        Detach,
        Quit,
        ReloadConfig,
        OpenConfig,
        EnterMode {
            mode: "resize".into(),
            sticky: true,
        },
        Lock,
    ]);
    v
}

type Value<'i> = Spanned<DeValue<'i>>;

/// Parses a binding/command action value. `Ok(None)` is the `"none"` unbind marker.
pub(crate) fn from_value(v: &Value<'_>) -> Result<Option<Action>, Diag> {
    match v.get_ref() {
        DeValue::String(id) => {
            if id == "none" {
                return Ok(None);
            }
            let mut args = Args::new(id, v.span(), None);
            build(id, v.span(), &mut args).map(Some)
        }
        DeValue::Table(t) => {
            let (id, id_span) = match find(t, "action") {
                Some(a) => match a.get_ref() {
                    DeValue::String(s) => (s.as_ref(), a.span()),
                    other => {
                        return Err(Diag::new(
                            a.span(),
                            format!("`action` must be a string, found {}", other.type_str()),
                        ));
                    }
                },
                None if find(t, "mode").is_some() => ("enter-mode", v.span()),
                None => {
                    return Err(Diag::new(v.span(), "missing `action` in action table")
                        .help("write { action = \"split-pane\", direction = \"right\" }"));
                }
            };
            if id == "none" {
                return Err(Diag::new(id_span, "\"none\" takes no arguments")
                    .help("write \"none\" as a plain string to unbind a key"));
            }
            let mut args = Args::new(id, v.span(), Some(t));
            let action = build(id, id_span, &mut args)?;
            args.finish()?;
            Ok(Some(action))
        }
        other => Err(Diag::new(
            v.span(),
            format!(
                "an action must be a string or an inline table, found {}",
                other.type_str()
            ),
        )
        .help("e.g. \"new-tab\" or { action = \"go-to-tab\", index = 1 }")),
    }
}

fn find<'a, 'i>(t: &'a DeTable<'i>, name: &str) -> Option<&'a Value<'i>> {
    t.iter().find(|(k, _)| k.get_ref() == name).map(|(_, v)| v)
}

fn build(id: &str, id_span: Range<usize>, a: &mut Args<'_, '_>) -> Result<Action, Diag> {
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

/// Argument reader that records which names the action asked for, so leftovers
/// can be reported as unknown with a suggestion from the real argument list.
struct Args<'a, 'i> {
    id: String,
    span: Range<usize>,
    table: Option<&'a DeTable<'i>>,
    known: Vec<&'static str>,
}

impl<'a, 'i> Args<'a, 'i> {
    fn new(id: &str, span: Range<usize>, table: Option<&'a DeTable<'i>>) -> Self {
        Self {
            id: id.to_string(),
            span,
            table,
            known: Vec::new(),
        }
    }

    fn get(&mut self, name: &'static str) -> Option<&'a Value<'i>> {
        self.known.push(name);
        self.table.and_then(|t| find(t, name))
    }

    fn missing(&self, name: &str, example: &str) -> Diag {
        Diag::new(
            self.span.clone(),
            format!("action \"{}\" needs argument `{name}`", self.id),
        )
        .help(format!(
            "write {{ action = \"{}\", {name} = {example} }}",
            self.id
        ))
    }

    fn type_err(name: &str, want: &str, v: &Value<'_>) -> Diag {
        Diag::new(
            v.span(),
            format!("`{name}` must be {want}, found {}", v.get_ref().type_str()),
        )
    }

    fn string(&mut self, name: &'static str) -> Result<Option<(String, Range<usize>)>, Diag> {
        match self.get(name) {
            None => Ok(None),
            Some(v) => match v.get_ref() {
                DeValue::String(s) if s.trim().is_empty() => {
                    Err(Diag::new(v.span(), format!("`{name}` must not be empty")))
                }
                DeValue::String(s) => Ok(Some((s.to_string(), v.span()))),
                _ => Err(Self::type_err(name, "a string", v)),
            },
        }
    }

    fn req_string(&mut self, name: &'static str, example: &str) -> Result<String, Diag> {
        self.string(name)?
            .map(|(s, _)| s)
            .ok_or_else(|| self.missing(name, example))
    }

    fn int(&mut self, name: &'static str, min: i64, max: i64) -> Result<Option<i64>, Diag> {
        let Some(v) = self.get(name) else {
            return Ok(None);
        };
        let DeValue::Integer(i) = v.get_ref() else {
            return Err(Self::type_err(name, "an integer", v));
        };
        match i64::from_str_radix(i.as_str(), i.radix()) {
            Ok(n) if (min..=max).contains(&n) => Ok(Some(n)),
            _ => Err(Diag::new(
                v.span(),
                format!("`{name}` must be between {min} and {max}"),
            )),
        }
    }

    fn req_int(&mut self, name: &'static str, min: i64, max: i64, ex: &str) -> Result<i64, Diag> {
        self.int(name, min, max)?
            .ok_or_else(|| self.missing(name, ex))
    }

    fn bool(&mut self, name: &'static str) -> Result<Option<bool>, Diag> {
        match self.get(name) {
            None => Ok(None),
            Some(v) => match v.get_ref() {
                DeValue::Boolean(b) => Ok(Some(*b)),
                _ => Err(Self::type_err(name, "true or false", v)),
            },
        }
    }

    fn choice<T: Copy>(
        &mut self,
        name: &'static str,
        options: &[(&'static str, T)],
    ) -> Result<Option<T>, Diag> {
        let Some(v) = self.get(name) else {
            return Ok(None);
        };
        let names = options.iter().map(|(n, _)| *n);
        let list = names.clone().collect::<Vec<_>>().join(", ");
        let DeValue::String(s) = v.get_ref() else {
            return Err(Self::type_err(name, &format!("one of {list}"), v));
        };
        match options.iter().find(|(n, _)| *n == s.as_ref()) {
            Some((_, t)) => Ok(Some(*t)),
            None => {
                let d = Diag::new(
                    v.span(),
                    format!("invalid `{name}` \"{s}\" (expected {list})"),
                );
                Err(d.suggest(s, names))
            }
        }
    }

    fn dir(&mut self, name: &'static str) -> Result<Option<Dir>, Diag> {
        self.choice(name, &Dir::ALL.map(|d| (d.as_str(), d)))
    }

    fn req_dir(&mut self, name: &'static str) -> Result<Dir, Diag> {
        self.dir(name)?
            .ok_or_else(|| self.missing(name, "\"left\""))
    }

    fn run_in(&mut self, name: &'static str) -> Result<Option<RunIn>, Diag> {
        let opts = [
            RunIn::NewTab,
            RunIn::SplitRight,
            RunIn::SplitDown,
            RunIn::Background,
        ];
        debug_assert!(opts.iter().map(|o| o.as_str()).eq(RunIn::NAMES));
        self.choice(name, &opts.map(|o| (o.as_str(), o)))
    }

    /// Split size: `30` or `"30%"`, 1..=99.
    fn size(&mut self, name: &'static str) -> Result<Option<u16>, Diag> {
        let Some(v) = self.get(name) else {
            return Ok(None);
        };
        let n = match v.get_ref() {
            DeValue::Integer(i) => i64::from_str_radix(i.as_str(), i.radix()).ok(),
            DeValue::String(s) => s
                .trim()
                .strip_suffix('%')
                .unwrap_or(s.trim())
                .trim()
                .parse::<i64>()
                .ok(),
            _ => return Err(Self::type_err(name, "a percentage like 30 or \"30%\"", v)),
        };
        match n {
            Some(n @ 1..=99) => Ok(Some(n as u16)),
            _ => Err(Diag::new(
                v.span(),
                format!("`{name}` must be a percentage from 1 to 99"),
            )),
        }
    }

    fn finish(&self) -> Result<(), Diag> {
        let Some(t) = self.table else {
            return Ok(());
        };
        for (k, _) in t.iter() {
            let k_str: &str = k.get_ref();
            if k_str == "action" || self.known.contains(&k_str) {
                continue;
            }
            let d = Diag::new(
                k.span(),
                format!("unknown argument `{k_str}` for action \"{}\"", self.id),
            );
            return Err(if self.known.is_empty() {
                d.help(format!("\"{}\" takes no arguments", self.id))
            } else {
                match crate::diag::did_you_mean(k_str, self.known.iter().copied()) {
                    Some(s) => d.help(format!("did you mean `{s}`?")),
                    None => d.help(format!("valid arguments: {}", self.known.join(", "))),
                }
            });
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(src: &str) -> Result<Option<Action>, Diag> {
        let doc = format!("x = {src}");
        let t = DeTable::parse(&doc).unwrap();
        let v = find(t.get_ref(), "x").unwrap().clone();
        from_value(&v).map_err(|mut d| {
            // Make spans relative to `src` for easy assertions.
            d.span = d.span.start - 4..d.span.end - 4;
            d
        })
    }

    fn ok(src: &str) -> Action {
        parse(src).unwrap().unwrap()
    }

    fn err(src: &str) -> Diag {
        parse(src).unwrap_err()
    }

    #[test]
    fn registry_is_consistent() {
        let mut ids = all_action_ids().to_vec();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), ACTIONS.len(), "duplicate ids");
        for a in palette_actions() {
            assert!(action_info(a.id()).is_some());
            // Round-trip through the string/table parser.
            assert!(!a.label().is_empty());
        }
        // Every arg-less id parses from a plain string and maps back to its id.
        for id in all_action_ids() {
            if let Ok(Some(a)) = parse(&format!("\"{id}\"")) {
                assert_eq!(a.id(), *id);
            }
        }
    }

    #[test]
    fn string_form() {
        assert_eq!(ok("\"new-tab\""), Action::NewTab);
        assert_eq!(parse("\"none\"").unwrap(), None);
        assert_eq!(
            ok("\"split-pane\""),
            Action::SplitPane {
                direction: Dir::Right,
                size: None,
                command: None
            }
        );
        assert_eq!(ok("\"scroll-up\""), Action::ScrollUp { lines: 3 });
    }

    #[test]
    fn table_form_with_defaults() {
        assert_eq!(
            ok(r#"{ action = "split-pane", direction = "down", size = "30%" }"#),
            Action::SplitPane {
                direction: Dir::Down,
                size: Some(30),
                command: None
            }
        );
        assert_eq!(
            ok(r#"{ action = "split-pane", size = 25, command = "htop" }"#),
            Action::SplitPane {
                direction: Dir::Right,
                size: Some(25),
                command: Some("htop".into())
            }
        );
        assert_eq!(
            ok(r#"{ action = "resize-pane", direction = "left" }"#),
            Action::ResizePane {
                direction: Dir::Left,
                amount: 2
            }
        );
        assert_eq!(
            ok(r#"{ action = "go-to-tab", index = 3 }"#),
            Action::GoToTab { index: 3 }
        );
        assert_eq!(
            ok(r#"{ action = "run", command = "lazygit", in = "split-down" }"#),
            Action::Run {
                command: "lazygit".into(),
                r#in: RunIn::SplitDown
            }
        );
        assert_eq!(
            ok(r#"{ action = "enter-mode", mode = "resize" }"#),
            Action::EnterMode {
                mode: "resize".into(),
                sticky: false
            }
        );
    }

    #[test]
    fn mode_shorthand() {
        assert_eq!(
            ok(r#"{ mode = "resize", sticky = true }"#),
            Action::EnterMode {
                mode: "resize".into(),
                sticky: true
            }
        );
    }

    #[test]
    fn unknown_action_suggests() {
        let d = err("\"split-pain\"");
        assert_eq!(d.message, "unknown action \"split-pain\"");
        assert_eq!(d.help.as_deref(), Some("did you mean \"split-pane\"?"));
        assert_eq!(d.span, 0..12);
        let d = err(r#"{ action = "new-tba" }"#);
        assert_eq!(d.help.as_deref(), Some("did you mean \"new-tab\"?"));
        assert_eq!(d.span.start, 11);
    }

    #[test]
    fn argument_errors() {
        let d = err("\"go-to-tab\"");
        assert!(
            d.message.contains("needs argument `index`"),
            "{}",
            d.message
        );
        assert!(d.help.unwrap().contains("index = 1"));

        let d = err(r#"{ action = "move-focus" }"#);
        assert!(d.message.contains("`direction`"));

        let d = err(r#"{ action = "resize-pane", direction = "left", amout = 3 }"#);
        assert!(d.message.contains("unknown argument `amout`"));
        assert_eq!(d.help.as_deref(), Some("did you mean `amount`?"));
        assert_eq!(
            &r#"{ action = "resize-pane", direction = "left", amout = 3 }"#[d.span],
            "amout"
        );

        let d = err(r#"{ action = "new-tab", foo = 1 }"#);
        assert_eq!(d.help.as_deref(), Some("\"new-tab\" takes no arguments"));

        let d = err(r#"{ action = "go-to-tab", index = "3" }"#);
        assert!(
            d.message.contains("must be an integer, found string"),
            "{}",
            d.message
        );

        let d = err(r#"{ action = "go-to-tab", index = 0 }"#);
        assert!(d.message.contains("between 1"));

        let d = err(r#"{ action = "move-focus", direction = "lft" }"#);
        assert_eq!(d.help.as_deref(), Some("did you mean \"left\"?"));

        let d = err(r#"{ action = "split-pane", size = 100 }"#);
        assert!(d.message.contains("1 to 99"));

        let d = err(r#"{ action = "enter-mode", mode = "resize", sticky = "yes" }"#);
        assert!(d.message.contains("true or false"));

        assert!(err("3").message.contains("string or an inline table"));
        assert!(
            err(r#"{ direction = "left" }"#)
                .message
                .contains("missing `action`")
        );
    }

    #[test]
    fn labels_include_args() {
        assert_eq!(
            Action::SplitPane {
                direction: Dir::Right,
                size: None,
                command: None
            }
            .label(),
            "Split pane right"
        );
        assert_eq!(Action::GoToTab { index: 3 }.label(), "Go to tab 3");
        assert_eq!(
            Action::ResizePane {
                direction: Dir::Left,
                amount: 2
            }
            .label(),
            "Resize pane left by 2"
        );
        assert_eq!(Action::MoveTab { delta: -1 }.label(), "Move tab left");
        assert_eq!(Action::NewTab.label(), "New tab");
        assert_eq!(Action::NewTab.category(), Category::Tab);
    }

    #[test]
    fn palette_has_defaults_and_no_motions() {
        let p = palette_actions();
        assert!(p.contains(&Action::GoToTab { index: 9 }));
        assert!(p.contains(&Action::ApplyLayout {
            name: "tiled".into()
        }));
        assert!(!p.iter().any(|a| matches!(a, Action::SelectMove { .. })));
        assert!(!p.contains(&Action::CommandPalette));
    }

    #[test]
    fn public_parse() {
        assert_eq!(Action::parse("new-tab").unwrap(), Action::NewTab);
        assert_eq!(
            Action::parse(r#"{ action = "move-focus", direction = "up" }"#).unwrap(),
            Action::MoveFocus { direction: Dir::Up }
        );
        assert!(
            Action::parse("split-pain")
                .unwrap_err()
                .contains("split-pane")
        );
        assert!(Action::parse("none").is_err());
    }
}
