//! Messages between the rust-multiplexer client and server.
//!
//! The client sends logical input events; the server owns all state and sends
//! back composed frames as cell diffs (tmux passes the tty fd to its server
//! instead, which is impossible on Windows). See docs/PLAN.md §4.1 and §5.8.

pub mod codec;

use crossterm::event::{KeyEvent, MouseEvent};
use ratatui::buffer::Cell;
use ratatui::style::{Color, Modifier};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

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

/// The client's half of a connection.
pub struct ClientEnd {
    pub tx: mpsc::Sender<ClientMsg>,
    pub rx: mpsc::Receiver<ServerMsg>,
}

/// The server's half of a connection.
pub struct ServerEnd {
    pub tx: mpsc::Sender<ServerMsg>,
    pub rx: mpsc::Receiver<ClientMsg>,
}

/// An in-process connection. Phase 1 runs the server on a task inside the
/// client process; phase 5 bridges these same channels over a local socket.
pub fn in_memory() -> (ClientEnd, ServerEnd) {
    // Small buffers: a slow client should apply back-pressure, not queue
    // frames without bound.
    let (client_tx, server_rx) = mpsc::channel(256);
    let (server_tx, client_rx) = mpsc::channel(8);
    (
        ClientEnd {
            tx: client_tx,
            rx: client_rx,
        },
        ServerEnd {
            tx: server_tx,
            rx: server_rx,
        },
    )
}

/// One screen cell on the wire.
///
/// Our own type rather than ratatui's `Cell`: ratatui's `Color` deserializes
/// via `deserialize_any`, which postcard (not self-describing) can't support,
/// and a dedicated type keeps the protocol stable across ratatui upgrades.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WireCell {
    pub symbol: String,
    pub fg: WireColor,
    pub bg: WireColor,
    pub modifier: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum WireColor {
    Reset,
    Indexed(u8),
    Rgb(u8, u8, u8),
}

impl From<&Cell> for WireCell {
    fn from(cell: &Cell) -> Self {
        Self {
            symbol: cell.symbol().to_owned(),
            fg: cell.fg.into(),
            bg: cell.bg.into(),
            modifier: cell.modifier.bits(),
        }
    }
}

impl From<&WireCell> for Cell {
    fn from(wire: &WireCell) -> Self {
        let mut cell = Cell::default();
        cell.set_symbol(&wire.symbol);
        cell.fg = wire.fg.into();
        cell.bg = wire.bg.into();
        cell.modifier = Modifier::from_bits_truncate(wire.modifier);
        cell
    }
}

impl From<Color> for WireColor {
    fn from(c: Color) -> Self {
        // Named colors are the first 16 palette entries; sending them as
        // indices keeps the enum small without losing anything.
        match c {
            Color::Reset => Self::Reset,
            Color::Black => Self::Indexed(0),
            Color::Red => Self::Indexed(1),
            Color::Green => Self::Indexed(2),
            Color::Yellow => Self::Indexed(3),
            Color::Blue => Self::Indexed(4),
            Color::Magenta => Self::Indexed(5),
            Color::Cyan => Self::Indexed(6),
            Color::Gray => Self::Indexed(7),
            Color::DarkGray => Self::Indexed(8),
            Color::LightRed => Self::Indexed(9),
            Color::LightGreen => Self::Indexed(10),
            Color::LightYellow => Self::Indexed(11),
            Color::LightBlue => Self::Indexed(12),
            Color::LightMagenta => Self::Indexed(13),
            Color::LightCyan => Self::Indexed(14),
            Color::White => Self::Indexed(15),
            Color::Indexed(i) => Self::Indexed(i),
            Color::Rgb(r, g, b) => Self::Rgb(r, g, b),
        }
    }
}

impl From<WireColor> for Color {
    fn from(c: WireColor) -> Self {
        match c {
            WireColor::Reset => Self::Reset,
            WireColor::Indexed(i) => Self::Indexed(i),
            WireColor::Rgb(r, g, b) => Self::Rgb(r, g, b),
        }
    }
}
