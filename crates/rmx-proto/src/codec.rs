//! Wire framing: a little-endian `u32` length prefix followed by a postcard body.

use serde::Serialize;
use serde::de::DeserializeOwned;

/// Frames larger than this are rejected rather than allocated, so a corrupt
/// or hostile peer can't make us reserve gigabytes.
pub const MAX_FRAME_LEN: usize = 16 * 1024 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum CodecError {
    #[error("frame of {0} bytes exceeds the {MAX_FRAME_LEN} byte limit")]
    TooLarge(usize),
    #[error("malformed message: {0}")]
    Malformed(#[from] postcard::Error),
}

pub fn encode<T: Serialize>(msg: &T) -> Result<Vec<u8>, CodecError> {
    let body = postcard::to_stdvec(msg)?;
    if body.len() > MAX_FRAME_LEN {
        return Err(CodecError::TooLarge(body.len()));
    }
    let mut out = Vec::with_capacity(4 + body.len());
    out.extend_from_slice(&(body.len() as u32).to_le_bytes());
    out.extend_from_slice(&body);
    Ok(out)
}

/// Accumulates bytes from a stream and yields complete messages.
#[derive(Default)]
pub struct Decoder {
    buf: Vec<u8>,
}

impl Decoder {
    pub fn push(&mut self, bytes: &[u8]) {
        self.buf.extend_from_slice(bytes);
    }

    /// Returns the next complete message, or `Ok(None)` if more bytes are needed.
    pub fn try_next<T: DeserializeOwned>(&mut self) -> Result<Option<T>, CodecError> {
        let Some(len_bytes) = self.buf.first_chunk::<4>() else {
            return Ok(None);
        };
        let len = u32::from_le_bytes(*len_bytes) as usize;
        if len > MAX_FRAME_LEN {
            return Err(CodecError::TooLarge(len));
        }
        if self.buf.len() < 4 + len {
            return Ok(None);
        }
        let msg = postcard::from_bytes(&self.buf[4..4 + len])?;
        self.buf.drain(..4 + len);
        Ok(Some(msg))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ClientMsg, Frame, ServerMsg, WireCell};
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use ratatui::buffer::Cell;
    use ratatui::style::{Color, Style};

    #[test]
    fn round_trips_messages_split_across_reads() {
        let mut cell = Cell::new("界");
        cell.set_style(Style::default().fg(Color::Rgb(1, 2, 3)));
        let msgs = vec![
            ServerMsg::Welcome { mouse: true },
            ServerMsg::Frame(Frame {
                full: true,
                cols: 80,
                rows: 24,
                cells: vec![(3, 4, WireCell::from(&cell))],
                cursor: Some((1, 2)),
                title: "t".into(),
            }),
        ];
        let wire: Vec<u8> = msgs.iter().flat_map(|m| encode(m).unwrap()).collect();

        let mut dec = Decoder::default();
        let mut got = Vec::new();
        // One byte at a time: the worst case for a stream reader.
        for b in wire {
            dec.push(&[b]);
            while let Some(m) = dec.try_next::<ServerMsg>().unwrap() {
                got.push(m);
            }
        }
        assert_eq!(got, msgs);
    }

    #[test]
    fn wire_cell_round_trips_style() {
        let mut cell = Cell::new("x");
        cell.set_style(
            Style::default()
                .fg(Color::Red)
                .bg(Color::Rgb(9, 8, 7))
                .add_modifier(ratatui::style::Modifier::BOLD),
        );
        let back = Cell::from(&WireCell::from(&cell));
        assert_eq!(back.symbol(), "x");
        assert_eq!(back.fg, Color::Indexed(1));
        assert_eq!(back.bg, Color::Rgb(9, 8, 7));
        assert_eq!(back.modifier, cell.modifier);
    }

    #[test]
    fn round_trips_key_events() {
        let msg = ClientMsg::Key(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::ALT));
        let mut dec = Decoder::default();
        dec.push(&encode(&msg).unwrap());
        assert_eq!(dec.try_next::<ClientMsg>().unwrap(), Some(msg));
    }

    #[test]
    fn rejects_oversized_frames_before_buffering_them() {
        let mut dec = Decoder::default();
        dec.push(&(u32::MAX).to_le_bytes());
        assert!(matches!(
            dec.try_next::<ClientMsg>(),
            Err(CodecError::TooLarge(_))
        ));
    }
}
