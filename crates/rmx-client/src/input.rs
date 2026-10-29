//! Input forwarding: reads terminal events on a dedicated thread and sends
//! them to the server as logical client messages.

use std::thread;

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use rmx_proto::ClientMsg;
use tokio::sync::mpsc;

/// Far below the 16 MiB frame limit, which would drop the whole connection.
pub(super) const MAX_PASTE: usize = 8 * 1024 * 1024;

pub(super) fn cap_len(mut text: String, max: usize) -> String {
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
pub(super) fn normalize_key(mut key: KeyEvent) -> Option<KeyEvent> {
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
pub(super) fn spawn_input_thread(tx: mpsc::Sender<ClientMsg>) {
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
