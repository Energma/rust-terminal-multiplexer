//! vt100 callbacks for the sequences it leaves to us: query replies, the
//! window title, OSC 7 working directory and scrolled-off line counts.

/// Receives the sequences vt100 does not handle itself.
#[derive(Default)]
pub(super) struct Hooks {
    pub(super) replies: Vec<u8>,
    pub(super) title: String,
    pub(super) cwd: Option<String>,
    pub(super) dropped: u64,
}

impl vt100::Callbacks for Hooks {
    fn on_scroll(&mut self, _: vt100::capture::RowContents<'_>, alternate_screen: bool) {
        if !alternate_screen {
            self.dropped += 1;
        }
    }

    fn set_window_title(&mut self, _: &mut vt100::Screen, title: &[u8]) {
        self.title = String::from_utf8_lossy(title).into_owned();
    }

    // OSC 7 (`file://host/path`) is how shells report their directory; it
    // lets new panes and restored sessions open where the user was.
    fn unhandled_osc(&mut self, _: &mut vt100::Screen, params: &[&[u8]]) {
        if let [b"7", url, ..] = params
            && let Some(rest) = std::str::from_utf8(url)
                .ok()
                .and_then(|u| u.strip_prefix("file://"))
        {
            let path = rest.find('/').map_or("", |i| &rest[i..]);
            if !path.is_empty() {
                self.cwd = Some(percent_decode(path));
            }
        }
    }

    // ConPTY sends a cursor position request (DSR 6) on startup and blocks the
    // shell until it is answered, and many TUIs probe with DA. vt100 doesn't
    // reply to either, so we do.
    fn unhandled_csi(
        &mut self,
        screen: &mut vt100::Screen,
        i1: Option<u8>,
        _i2: Option<u8>,
        params: &[&[u16]],
        c: char,
    ) {
        let first = params.first().and_then(|p| p.first()).copied().unwrap_or(0);
        match (i1, c, first) {
            (None, 'n', 6) => {
                let (row, col) = screen.cursor_position();
                self.replies
                    .extend_from_slice(format!("\x1b[{};{}R", row + 1, col + 1).as_bytes());
            }
            (None, 'n', 5) => self.replies.extend_from_slice(b"\x1b[0n"),
            // VT100 with advanced video option: the answer most emulators give.
            (None, 'c', 0) => self.replies.extend_from_slice(b"\x1b[?1;2c"),
            (Some(b'>'), 'c', 0) => self.replies.extend_from_slice(b"\x1b[>0;0;0c"),
            _ => {}
        }
    }
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && let Some(b) = s
                .get(i + 1..i + 3)
                .and_then(|h| u8::from_str_radix(h, 16).ok())
        {
            out.push(b);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}
