//! Platform-dependent choices for spawning: default shell, how commands
//! are run, the home directory, PATH lookup and PTY sizing.

use std::path::PathBuf;

use portable_pty::PtySize;

pub(super) fn pty_size(rows: u16, cols: u16) -> PtySize {
    PtySize {
        rows: rows.max(1),
        cols: cols.max(1),
        pixel_width: 0,
        pixel_height: 0,
    }
}

/// On Windows prefer PowerShell 7, then Windows PowerShell, over portable-pty's
/// `%ComSpec%` (cmd.exe) default. Elsewhere portable-pty's `$SHELL`/passwd
/// lookup is already right.
pub(super) fn default_shell() -> Option<String> {
    if cfg!(windows) {
        ["pwsh.exe", "powershell.exe"]
            .into_iter()
            .find_map(find_on_path)
    } else {
        None
    }
}

/// "pwsh", "vim", "bash" … from a command line or program path.
pub(super) fn program_name(command: Option<&str>) -> String {
    let fallback = std::env::var("SHELL").unwrap_or_else(|_| "shell".into());
    let line = command.unwrap_or(&fallback);
    let first = line.split_whitespace().next().unwrap_or("shell");
    let base = first.rsplit(['/', '\\']).next().unwrap_or(first);
    base.strip_suffix(".exe").unwrap_or(base).to_owned()
}

/// Runs `command` through the platform shell so quoting and PATH lookup
/// behave as users expect from typing it.
pub(super) fn command_argv(command: &str) -> Vec<String> {
    if cfg!(windows) {
        vec!["cmd.exe".into(), "/C".into(), command.into()]
    } else {
        vec!["/bin/sh".into(), "-c".into(), command.into()]
    }
}

pub(super) fn dirs_home() -> Option<PathBuf> {
    std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).map(PathBuf::from)
}

fn find_on_path(program: &str) -> Option<String> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join(program))
        .find(|candidate| candidate.is_file())
        .map(|p| p.to_string_lossy().into_owned())
}
