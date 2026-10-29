//! The system clipboard, with an OSC 52 fallback for remote sessions.

use std::io::Write;

/// The native clipboard, falling back to OSC 52 (which travels through SSH
/// to the user's real terminal) when there is no local clipboard.
pub(super) struct Clipboard {
    native: Option<arboard::Clipboard>,
    remote: bool,
}

impl Clipboard {
    pub(super) fn new() -> Self {
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

    pub(super) fn set(&mut self, text: &str, out: &mut impl Write) {
        let ok = self
            .native
            .as_mut()
            .is_some_and(|c| c.set_text(text.to_owned()).is_ok());
        if !ok || self.remote {
            let _ = write!(out, "\x1b]52;c;{}\x07", base64(text.as_bytes()));
            let _ = out.flush();
        }
    }

    pub(super) fn get(&mut self) -> Option<String> {
        self.native.as_mut().and_then(|c| c.get_text().ok())
    }
}

pub(super) fn base64(data: &[u8]) -> String {
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
