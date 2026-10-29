//! Terminal setup and teardown: raw mode, the alternate screen and input
//! protocols, restored on drop and on panic.

use std::io::{self, Stdout};

use anyhow::Result;
use crossterm::event::{
    DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, KeyboardEnhancementFlags,
    PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags,
};
use crossterm::terminal::{self, EnterAlternateScreen, LeaveAlternateScreen};
use crossterm::{cursor, execute};

/// Puts the terminal in raw/alternate-screen mode and always restores it,
/// including on panic, so a crash never leaves the user's shell unusable.
pub(super) struct TerminalGuard {
    pub(super) enhanced_keys: bool,
    pub(super) mouse: bool,
}

impl TerminalGuard {
    pub(super) fn enter() -> Result<Self> {
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
