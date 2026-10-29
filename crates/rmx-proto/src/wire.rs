//! Screen cells and colours in their wire form, with ratatui conversions.

use ratatui::buffer::Cell;
use ratatui::style::{Color, Modifier};
use serde::{Deserialize, Serialize};

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
