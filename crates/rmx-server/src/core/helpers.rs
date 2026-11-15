//! Small free helpers shared by actions and the control API.

use std::path::{Path, PathBuf};

use anyhow::Result;
use rmx_config::action::Dir;
use rmx_core::layout::Side;

pub(super) fn side(d: Dir) -> Side {
    match d {
        Dir::Left => Side::Left,
        Dir::Right => Side::Right,
        Dir::Up => Side::Up,
        Dir::Down => Side::Down,
    }
}

/// `\n`, `\r`, `\t`, `\e` and `\\` escapes for `send-keys` style input.
pub(super) fn unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('r') => out.push('\r'),
            Some('t') => out.push('\t'),
            Some('e') => out.push('\x1b'),
            Some(other) => out.push(other),
            None => out.push('\\'),
        }
    }
    out
}

/// The terminal editor to fall back on when the configured one isn't
/// installed: the user's, or one every system of that OS has.
pub(super) fn fallback_editor() -> String {
    std::env::var("VISUAL")
        .or_else(|_| std::env::var("EDITOR"))
        .unwrap_or_else(|_| if cfg!(windows) { "notepad" } else { "nano" }.into())
}

/// Whether the program `command` starts with can be found, so a missing
/// editor gets a fallback instead of a tab that dies or nothing at all.
pub(super) fn installed(command: &str) -> bool {
    let Some(program) = command.split_whitespace().next() else {
        return false;
    };
    if program.contains(['/', '\\']) {
        return Path::new(program).is_file();
    }
    // `code` is `code.cmd` on Windows.
    let exts: &[&str] = if cfg!(windows) {
        &["", ".exe", ".cmd", ".bat"]
    } else {
        &[""]
    };
    let path = std::env::var_os("PATH").unwrap_or_default();
    std::env::split_paths(&path).any(|dir| {
        exts.iter()
            .any(|e| dir.join(format!("{program}{e}")).is_file())
    })
}

/// The git repository `dir` is in, or `dir` itself outside any repository.
pub(super) fn project_root(dir: &Path) -> PathBuf {
    dir.ancestors()
        // `exists`, not `is_dir`: `.git` is a file in worktrees and submodules.
        .find(|d| d.join(".git").exists())
        .unwrap_or(dir)
        .to_path_buf()
}

pub(super) fn run_background(command: &str, cwd: Option<PathBuf>) -> Result<()> {
    let mut cmd = if cfg!(windows) {
        let mut c = std::process::Command::new("cmd.exe");
        c.args(["/C", command]);
        c
    } else {
        let mut c = std::process::Command::new("/bin/sh");
        c.args(["-c", command]);
        c
    };
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // The server has no console; without this a window would flash.
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    if let Some(dir) = cwd {
        cmd.current_dir(dir);
    }
    cmd.stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    cmd.spawn()?;
    Ok(())
}
