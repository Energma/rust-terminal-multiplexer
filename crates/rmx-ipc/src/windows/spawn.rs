//! Starting the server detached from the console and, when allowed, its job.

use std::os::windows::process::CommandExt;
use std::path::Path;
use std::process::{Command, Stdio};

use anyhow::Context;
use windows_sys::Win32::Foundation::ERROR_ACCESS_DENIED;
use windows_sys::Win32::System::Threading::{
    CREATE_BREAKAWAY_FROM_JOB, CREATE_NEW_PROCESS_GROUP, DETACHED_PROCESS,
};

use crate::{Endpoint, SpawnInfo};

/// No lock on Windows: if two clients both spawn, the second server's bind
/// fails on `FILE_FLAG_FIRST_PIPE_INSTANCE` and it exits, and both clients
/// connect to the first.
pub(crate) async fn spawn_lock(_ep: &Endpoint) -> anyhow::Result<NoLock> {
    Ok(NoLock)
}

/// Stands in for the Unix lock guard so the shared code holds "a lock" on
/// every platform.
pub(crate) struct NoLock;

pub(crate) fn creation_flags(breakaway: bool) -> u32 {
    let mut flags = DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP;
    if breakaway {
        // Leave the terminal's / VS Code's job object, which would otherwise
        // kill the server when that host closes.
        flags |= CREATE_BREAKAWAY_FROM_JOB;
    }
    flags
}

/// Split out of [`spawn_detached`] so tests can inspect it without spawning.
pub(crate) fn detached_command(exe: &Path, args: &[&str], breakaway: bool) -> Command {
    let mut cmd = Command::new(exe);
    cmd.args(args)
        .current_dir(crate::neutral_dir())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .creation_flags(creation_flags(breakaway));
    cmd
}

pub(crate) fn spawn_detached(exe: &Path, args: &[&str]) -> anyhow::Result<SpawnInfo> {
    let (child, breakaway) = match detached_command(exe, args, true).spawn() {
        Ok(c) => (c, true),
        // The job forbids breakaway: run inside it and let the caller warn.
        Err(e) if e.raw_os_error() == Some(ERROR_ACCESS_DENIED as i32) => {
            let c = detached_command(exe, args, false)
                .spawn()
                .with_context(|| format!("starting {}", exe.display()))?;
            (c, false)
        }
        Err(e) => return Err(e).with_context(|| format!("starting {}", exe.display())),
    };
    Ok(SpawnInfo {
        pid: child.id(),
        breakaway,
    })
}
