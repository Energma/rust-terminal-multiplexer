//! The client's main loop: the handshake, then acting on server messages
//! until the session ends.

use std::io::{self, Write};

use anyhow::Result;
use crossterm::event::EnableMouseCapture;
use crossterm::execute;
use crossterm::terminal;
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use rmx_proto::{ClientEnd, ClientMsg, Hello, PROTOCOL_VERSION, ServerMsg};

use crate::Options;
use crate::clipboard::Clipboard;
use crate::input::{MAX_PASTE, cap_len, spawn_input_thread};
use crate::render::paint;
use crate::terminal::TerminalGuard;

/// Runs until the server ends the session; returns its reason.
pub async fn run(mut conn: ClientEnd, opts: Options) -> Result<String> {
    let mut guard = TerminalGuard::enter()?;
    let mut terminal = Terminal::new(CrosstermBackend::new(io::stdout()))?;
    let mut clipboard = Clipboard::new();

    let (cols, rows) = terminal::size()?;
    conn.tx
        .send(ClientMsg::Hello(Hello {
            version: PROTOCOL_VERSION,
            cols,
            rows,
            enhanced_keys: guard.enhanced_keys,
            attach: opts.attach,
            open_manager: opts.open_manager,
            cwd: opts.cwd,
        }))
        .await?;
    spawn_input_thread(conn.tx.clone());

    let mut welcomed = false;
    let mut screen = Buffer::empty(Rect::new(0, 0, cols, rows));
    let mut title = String::new();

    while let Some(msg) = conn.rx.recv().await {
        match msg {
            ServerMsg::Welcome { mouse } => {
                welcomed = true;
                if mouse {
                    execute!(io::stdout(), EnableMouseCapture)?;
                    guard.mouse = true;
                }
            }
            ServerMsg::Frame(frame) => paint(&mut terminal, &mut screen, &mut title, frame)?,
            ServerMsg::Bell => {
                terminal.backend_mut().write_all(b"\x07")?;
                terminal.backend_mut().flush()?;
            }
            ServerMsg::SetClipboard(text) => clipboard.set(&text, terminal.backend_mut()),
            ServerMsg::GetClipboard => {
                let text = clipboard.get().map(|t| cap_len(t, MAX_PASTE));
                conn.tx.send(ClientMsg::Clipboard(text)).await?;
            }
            // Refused before attaching (no such workspace, version
            // mismatch): that's a failure for scripts, not a normal exit.
            ServerMsg::Exit { reason } if !welcomed => anyhow::bail!("{reason}"),
            ServerMsg::Exit { reason } => return Ok(reason),
            ServerMsg::Response { .. } | ServerMsg::Event(_) => {}
        }
    }
    Ok("lost connection to the server".into())
}
