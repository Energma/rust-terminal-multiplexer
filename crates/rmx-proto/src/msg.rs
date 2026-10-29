//! The messages each side sends, and the protocol version they belong to.

use crossterm::event::{KeyEvent, MouseEvent};
use serde::{Deserialize, Serialize};

use crate::wire::WireCell;

/// Bumped on any incompatible change; client and server refuse mismatches.
pub const PROTOCOL_VERSION: u32 = 2;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ClientMsg {
    /// First message of an interactive client.
    Hello(Hello),
    Key(KeyEvent),
    Mouse(MouseEvent),
    Paste(String),
    Resize {
        cols: u16,
        rows: u16,
    },
    /// Reply to [`ServerMsg::GetClipboard`]; `None` if the clipboard is
    /// unavailable (e.g. over SSH), so the server uses its own buffer.
    Clipboard(Option<String>),
    /// First message of a control (CLI/scripting) client. Carries the
    /// version so a mismatched server answers clearly instead of guessing.
    Request {
        version: u32,
        request: Request,
    },
    Detach,
}

impl ClientMsg {
    pub fn request(request: Request) -> Self {
        Self::Request {
            version: PROTOCOL_VERSION,
            request,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Hello {
    pub version: u32,
    pub cols: u16,
    pub rows: u16,
    /// The terminal reports unambiguous modifiers (kitty protocol, or the
    /// Windows console), so Ctrl+Shift bindings can work.
    pub enhanced_keys: bool,
    pub attach: Attach,
    /// Start with the session manager open.
    pub open_manager: bool,
    /// Directory `rmx` was run from, offered for new workspaces.
    pub cwd: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Attach {
    /// The most recently used workspace, or a new one if none exist.
    Last,
    Named(String),
    New {
        name: Option<String>,
        cwd: Option<String>,
    },
}

/// Commands from `rmx <subcommand>`; answered with one [`ServerMsg::Response`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Request {
    List,
    NewWorkspace {
        name: Option<String>,
        cwd: Option<String>,
        command: Option<String>,
    },
    CloseWorkspace {
        name: String,
    },
    RenameWorkspace {
        from: String,
        to: String,
    },
    /// Types `text` into the target pane (`workspace:tab.pane` or `%id`).
    SendKeys {
        target: Option<String>,
        text: String,
    },
    Capture {
        target: Option<String>,
    },
    Save,
    Restore,
    ReloadConfig,
    /// Stream [`ServerMsg::Event`]s until the client disconnects.
    Subscribe,
    Stop,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ServerMsg {
    Welcome {
        mouse: bool,
    },
    Frame(Frame),
    Bell,
    SetClipboard(String),
    GetClipboard,
    /// `body` is JSON on success, a human-readable error otherwise. A string
    /// rather than a JSON value because postcard can't encode `serde_json::Value`.
    Response {
        ok: bool,
        body: String,
    },
    /// One JSON event, for `rmx events`.
    Event(String),
    Exit {
        reason: String,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Frame {
    /// When set, the client starts from a blank `cols`×`rows` buffer before
    /// applying `cells` (first frame, or after a resize).
    pub full: bool,
    pub cols: u16,
    pub rows: u16,
    /// Changed cells as `(x, y, cell)`.
    pub cells: Vec<(u16, u16, WireCell)>,
    /// Cursor as `(x, y)`; `None` hides it.
    pub cursor: Option<(u16, u16)>,
    pub title: String,
}
