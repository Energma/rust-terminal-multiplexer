//! Converting vt100 cell attributes and colours to ratatui styles.

use ratatui::style::{Color, Modifier, Style};

pub(super) fn cell_style(cell: &vt100::Cell) -> Style {
    let mut modifier = Modifier::empty();
    for (on, m) in [
        (cell.bold(), Modifier::BOLD),
        (cell.dim(), Modifier::DIM),
        (cell.italic(), Modifier::ITALIC),
        (cell.underline(), Modifier::UNDERLINED),
        (cell.inverse(), Modifier::REVERSED),
    ] {
        if on {
            modifier |= m;
        }
    }
    Style::default()
        .fg(color(cell.fgcolor()))
        .bg(color(cell.bgcolor()))
        .add_modifier(modifier)
}

fn color(c: vt100::Color) -> Color {
    match c {
        vt100::Color::Default => Color::Reset,
        vt100::Color::Idx(i) => Color::Indexed(i),
        vt100::Color::Rgb(r, g, b) => Color::Rgb(r, g, b),
    }
}
