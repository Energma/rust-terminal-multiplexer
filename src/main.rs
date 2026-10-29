//! `rmx` — the rust-multiplexer command.
//!
//! Plain `rmx` attaches to the last workspace (starting the background
//! server if needed). Subcommands use Windows-friendly names, and tmux's
//! names work as aliases so muscle memory carries over.

use clap::Parser;

mod cli;
mod logging;
mod update;

use cli::Cli;

fn main() {
    let cli = Cli::parse();
    // CLI output is often piped (`rmx keys | head`); exit quietly when the
    // reader goes away instead of panicking. The server keeps ignoring
    // SIGPIPE, since a vanished client must not kill it.
    #[cfg(unix)]
    if !matches!(cli.command, Some(cli::Command::Server { .. })) {
        unsafe {
            libc::signal(libc::SIGPIPE, libc::SIG_DFL);
        }
    }
    match cli::run(cli) {
        Ok(code) => std::process::exit(code),
        Err(err) => {
            eprintln!("rmx: {err:#}");
            std::process::exit(1);
        }
    }
}
