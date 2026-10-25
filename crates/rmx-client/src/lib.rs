//! The rust-multiplexer client: a thin terminal front end.
//!
//! It forwards input as logical events and paints the cell diffs the server
//! sends. It holds no session state, which is what makes detach possible.
//! The one thing it owns is the system clipboard, since that belongs to the
//! machine the user sits at.

use std::io::{self, Stdout, Write};
use std::thread;

use anyhow::Result;
use crossterm::event::{
    self, DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture,
    Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, KeyboardEnhancementFlags,
    PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags,
};
use crossterm::terminal::{
    self, BeginSynchronizedUpdate, EndSynchronizedUpdate, EnterAlternateScreen,
    LeaveAlternateScreen, SetTitle,
};
use crossterm::{cursor, execute, queue};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::buffer::{Buffer, Cell};
use ratatui::layout::Rect;
use ratatui::widgets::Widget;
use rmx_proto::{Attach, ClientEnd, ClientMsg, Hello, PROTOCOL_VERSION, ServerMsg};
use tokio::sync::mpsc;

/// How this client wants to join the server.
#[derive(Debug, Clone)]
pub struct Options {
    pub attach: Attach,
    pub open_manager: bool,
}

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
            ServerMsg::Frame(frame) => {
                let area = Rect::new(0, 0, frame.cols, frame.rows);
                if frame.full || screen.area != area {
                    screen = Buffer::empty(area);
                }
                for (x, y, cell) in &frame.cells {
                    if let Some(dst) = screen.cell_mut((*x, *y)) {
                        *dst = Cell::from(cell);
                    }
                }
                if frame.title != title {
                    title = frame.title;
                    execute!(terminal.backend_mut(), SetTitle(format!("{title} — rmx")))?;
                }
                // Synchronized output stops terminals showing half-drawn
                // frames; terminals without support ignore it.
                queue!(terminal.backend_mut(), BeginSynchronizedUpdate)?;
                terminal.draw(|f| {
                    f.render_widget(Blit(&screen), f.area());
                    if let Some(pos) = frame.cursor {
                        f.set_cursor_position(pos);
                    }
                })?;
                execute!(terminal.backend_mut(), EndSynchronizedUpdate)?;
            }
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

/// Far below the 16 MiB frame limit, which would drop the whole connection.
const MAX_PASTE: usize = 8 * 1024 * 1024;

fn cap_len(mut text: String, max: usize) -> String {
    if text.len() > max {
        let mut end = max;
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        text.truncate(end);
    }
    text
}

/// Turns raw terminal key events into the ones we act on, or `None` to drop.
///
/// Windows reports key releases (dropped, except the Alt release that
/// carries an Alt+numpad character such as Alt+0169), and reports AltGr as
/// Ctrl+Alt, which would turn `@` or `{` on European layouts into shortcuts.
fn normalize_key(mut key: KeyEvent) -> Option<KeyEvent> {
    let altgr = KeyModifiers::CONTROL | KeyModifiers::ALT;
    if cfg!(windows)
        && let KeyCode::Char(c) = key.code
    {
        if key.kind == KeyEventKind::Release
            && key.modifiers.contains(KeyModifiers::ALT)
            && !c.is_ascii_alphabetic()
        {
            key.kind = KeyEventKind::Press;
            key.modifiers = KeyModifiers::NONE;
        } else if key.modifiers.contains(altgr) && !c.is_ascii_alphanumeric() && !c.is_control() {
            key.modifiers -= altgr;
        }
    }
    (key.kind != KeyEventKind::Release).then_some(key)
}

/// crossterm's `event::read` blocks, so input lives on its own thread.
fn spawn_input_thread(tx: mpsc::Sender<ClientMsg>) {
    thread::Builder::new()
        .name("input".into())
        .spawn(move || {
            loop {
                let msg = match event::read() {
                    // Windows reports key releases too; only presses and
                    // auto-repeats are input.
                    Ok(Event::Key(key)) => match normalize_key(key) {
                        Some(key) => ClientMsg::Key(key),
                        None => continue,
                    },
                    Ok(Event::Mouse(ev)) => ClientMsg::Mouse(ev),
                    Ok(Event::Paste(text)) => ClientMsg::Paste(cap_len(text, MAX_PASTE)),
                    Ok(Event::Resize(cols, rows)) => ClientMsg::Resize { cols, rows },
                    Ok(_) => continue,
                    Err(err) => {
                        tracing::error!(%err, "reading terminal input failed");
                        break;
                    }
                };
                if tx.blocking_send(msg).is_err() {
                    break;
                }
            }
        })
        .expect("spawning the input thread");
}

/// The native clipboard, falling back to OSC 52 (which travels through SSH
/// to the user's real terminal) when there is no local clipboard.
struct Clipboard {
    native: Option<arboard::Clipboard>,
    remote: bool,
}

impl Clipboard {
    fn new() -> Self {
        // Over SSH the "local" clipboard is the server's, not the user's.
        let remote =
            std::env::var_os("SSH_CONNECTION").is_some() || std::env::var_os("SSH_TTY").is_some();
        let native = if remote {
            None
        } else {
            arboard::Clipboard::new().ok()
        };
        Self { native, remote }
    }

    fn set(&mut self, text: &str, out: &mut impl Write) {
        let ok = self
            .native
            .as_mut()
            .is_some_and(|c| c.set_text(text.to_owned()).is_ok());
        if !ok || self.remote {
            let _ = write!(out, "\x1b]52;c;{}\x07", base64(text.as_bytes()));
            let _ = out.flush();
        }
    }

    fn get(&mut self) -> Option<String> {
        self.native.as_mut().and_then(|c| c.get_text().ok())
    }
}

fn base64(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(T[((n >> (18 - 6 * i)) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// Copies the client's screen buffer into ratatui's frame.
struct Blit<'a>(&'a Buffer);

impl Widget for Blit<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let area = area.intersection(self.0.area);
        for y in area.top()..area.bottom() {
            for x in area.left()..area.right() {
                buf[(x, y)] = self.0[(x, y)].clone();
            }
        }
    }
}

/// Puts the terminal in raw/alternate-screen mode and always restores it,
/// including on panic, so a crash never leaves the user's shell unusable.
struct TerminalGuard {
    enhanced_keys: bool,
    mouse: bool,
}

impl TerminalGuard {
    fn enter() -> Result<Self> {
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            restore(&mut io::stdout(), true, true);
            previous(info);
        }));
        terminal::enable_raw_mode()?;
        execute!(io::stdout(), EnterAlternateScreen)?;
        // Unsupported on Windows; pastes then arrive as plain key events.
        let _ = execute!(io::stdout(), EnableBracketedPaste);
        // The kitty keyboard protocol lets Ctrl+Shift bindings work in
        // terminals that support it. The Windows console reports full
        // modifiers natively, and crossterm can't push flags there.
        let enhanced_keys = !cfg!(windows)
            && terminal::supports_keyboard_enhancement().unwrap_or(false)
            && execute!(
                io::stdout(),
                // Alternate keys make the terminal report Alt+Shift+1 as `!`.
                PushKeyboardEnhancementFlags(
                    KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES
                        | KeyboardEnhancementFlags::REPORT_ALTERNATE_KEYS
                )
            )
            .is_ok();
        Ok(Self {
            enhanced_keys,
            mouse: false,
        })
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        restore(&mut io::stdout(), self.enhanced_keys, self.mouse);
    }
}

fn restore(out: &mut Stdout, enhanced: bool, mouse: bool) {
    if enhanced {
        let _ = execute!(out, PopKeyboardEnhancementFlags);
    }
    if mouse {
        let _ = execute!(out, DisableMouseCapture);
    }
    let _ = execute!(out, DisableBracketedPaste);
    let _ = execute!(out, LeaveAlternateScreen, cursor::Show);
    let _ = terminal::disable_raw_mode();
}

#[cfg(test)]
mod tests {
    use super::base64;

    #[test]
    fn caps_paste_on_a_char_boundary() {
        // "ab€" is 5 bytes; caps that land inside a "€" back off to its start.
        assert_eq!(super::cap_len("ab€".repeat(4), 6), "ab€a");
        assert_eq!(super::cap_len("ab€".repeat(4), 8), "ab€ab");
    }

    #[test]
    fn drops_key_releases() {
        use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
        let mut k = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE);
        assert!(super::normalize_key(k).is_some());
        k.kind = KeyEventKind::Release;
        assert!(super::normalize_key(k).is_none());
    }

    #[test]
    fn base64_matches_rfc4648_vectors() {
        for (input, want) in [
            ("", ""),
            ("f", "Zg=="),
            ("fo", "Zm8="),
            ("foo", "Zm9v"),
            ("foobar", "Zm9vYmFy"),
        ] {
            assert_eq!(base64(input.as_bytes()), want);
        }
    }
}
