//! The `[theme]` colour palette and its by-name slot lookup.

use ratatui::style::Color;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Theme {
    pub status_bg: Color,
    pub status_fg: Color,
    pub accent: Color,
    pub border: Color,
    pub active_border: Color,
    pub menu_bg: Color,
    pub menu_fg: Color,
    pub menu_highlight_bg: Color,
    pub menu_highlight_fg: Color,
}

/// Catppuccin Mocha: readable on dark terminals, and familiar to many users.
impl Default for Theme {
    fn default() -> Self {
        Self {
            status_bg: Color::Rgb(0x1e, 0x1e, 0x2e),
            status_fg: Color::Rgb(0xcd, 0xd6, 0xf4),
            accent: Color::Rgb(0xcb, 0xa6, 0xf7),
            border: Color::Rgb(0x45, 0x47, 0x5a),
            active_border: Color::Rgb(0x89, 0xb4, 0xfa),
            menu_bg: Color::Rgb(0x31, 0x32, 0x44),
            menu_fg: Color::Rgb(0xcd, 0xd6, 0xf4),
            menu_highlight_bg: Color::Rgb(0x89, 0xb4, 0xfa),
            menu_highlight_fg: Color::Rgb(0x1e, 0x1e, 0x2e),
        }
    }
}

pub(super) const THEME_KEYS: &[&str] = &[
    "status_bg",
    "status_fg",
    "accent",
    "border",
    "active_border",
    "menu_bg",
    "menu_fg",
    "menu_highlight_bg",
    "menu_highlight_fg",
];

impl Theme {
    pub(super) fn slot(&mut self, name: &str) -> Option<&mut Color> {
        Some(match name {
            "status_bg" => &mut self.status_bg,
            "status_fg" => &mut self.status_fg,
            "accent" => &mut self.accent,
            "border" => &mut self.border,
            "active_border" => &mut self.active_border,
            "menu_bg" => &mut self.menu_bg,
            "menu_fg" => &mut self.menu_fg,
            "menu_highlight_bg" => &mut self.menu_highlight_bg,
            "menu_highlight_fg" => &mut self.menu_highlight_fg,
            _ => return None,
        })
    }
}
