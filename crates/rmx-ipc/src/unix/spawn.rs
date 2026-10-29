//! Starting the server in its own session, detached from the terminal.

use std::io;
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Command, Stdio};

use anyhow::Context;

use crate::SpawnInfo;

/// Split out of [`spawn_detached`] so tests can inspect it without spawning.
pub(crate) fn detached_command(exe: &Path, args: &[&str]) -> Command {
    let mut cmd = Command::new(exe);
    cmd.args(args)
        .current_dir(crate::neutral_dir())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    // SAFETY: the closure runs between fork and exec and only calls
    // async-signal-safe functions (setsid, signal).
    unsafe {
        cmd.pre_exec(|| {
            // New session: no controlling terminal, so closing the terminal
            // doesn't SIGHUP the server.
            if libc::setsid() == -1 {
                return Err(io::Error::last_os_error());
            }
            // Ignored dispositions survive exec; belt and braces for SIGHUP,
            // and a dead client socket must be an EPIPE, not a kill.
            libc::signal(libc::SIGHUP, libc::SIG_IGN);
            libc::signal(libc::SIGPIPE, libc::SIG_IGN);
            Ok(())
        });
    }
    cmd
}

pub(crate) fn spawn_detached(exe: &Path, args: &[&str]) -> anyhow::Result<SpawnInfo> {
    let mut child = detached_command(exe, args)
        .spawn()
        .with_context(|| format!("starting {}", exe.display()))?;
    let pid = child.id();
    // Reap it if it exits while we're still running, so it doesn't linger as
    // a zombie; if we exit first, init adopts it.
    std::thread::Builder::new()
        .name("rmx-reaper".into())
        .spawn(move || {
            let _ = child.wait();
        })
        .context("spawning reaper thread")?;
    Ok(SpawnInfo {
        pid,
        breakaway: true,
    })
}
