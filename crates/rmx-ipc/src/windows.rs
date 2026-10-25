//! Windows named pipes, locked down per docs/PLAN.md §5.9.
//!
//! A default named pipe is readable by Everyone, so every instance gets an
//! explicit protected DACL (SYSTEM + current user only) and `O:` pinned to our
//! SID. The first instance uses `FILE_FLAG_FIRST_PIPE_INSTANCE`, so a squatter
//! can't sit on the name and pose as our server; and clients check the pipe's
//! owner SID, which only our own user can set, before trusting it. Remote
//! (SMB) clients are rejected, and tokio's client default of
//! `SECURITY_IDENTIFICATION` QoS keeps a server from impersonating us.

use std::io;
use std::os::windows::io::AsRawHandle;
use std::os::windows::process::CommandExt;
use std::path::Path;
use std::process::{Command, Stdio};
use std::ptr::null_mut;
use std::time::Duration;

use anyhow::{Context, bail};
use rmx_proto::{ClientEnd, ServerEnd};
use tokio::net::windows::named_pipe::{
    ClientOptions, NamedPipeClient, NamedPipeServer, ServerOptions,
};
use tokio::time::Instant;
use windows_sys::Win32::Foundation::{
    CloseHandle, ERROR_ACCESS_DENIED, ERROR_NO_DATA, ERROR_PIPE_BUSY, ERROR_SUCCESS, HANDLE,
    LocalFree,
};
use windows_sys::Win32::Security::Authorization::{
    ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW, GetSecurityInfo,
    SDDL_REVISION_1, SE_KERNEL_OBJECT,
};
use windows_sys::Win32::Security::{
    GetTokenInformation, OWNER_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR, PSID,
    SECURITY_ATTRIBUTES, TOKEN_QUERY, TOKEN_USER, TokenUser,
};
use windows_sys::Win32::System::Threading::{
    CREATE_BREAKAWAY_FROM_JOB, CREATE_NEW_PROCESS_GROUP, DETACHED_PROCESS, GetCurrentProcess,
    OpenProcessToken,
};

use crate::{Endpoint, SpawnInfo, client_end, server_end};

/// Builds an endpoint for an explicit pipe name (tests, overrides).
pub fn endpoint_at(pipe: impl Into<String>) -> Endpoint {
    Endpoint { pipe: pipe.into() }
}

pub(crate) fn endpoint(name: &str) -> anyhow::Result<Endpoint> {
    // The SID rather than the user name: it is unique, can't be renamed and
    // contains no characters that are awkward in a pipe name.
    let sid = current_user_sid()?;
    Ok(endpoint_at(format!(r"\\.\pipe\rmx-{sid}-{name}")))
}

fn current_user_sid() -> anyhow::Result<String> {
    // SAFETY: standard token query; the buffer is sized by the first call and
    // u64-aligned, which satisfies TOKEN_USER's alignment.
    unsafe {
        let mut token: HANDLE = null_mut();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
            return Err(io::Error::last_os_error()).context("opening process token");
        }
        let mut len = 0u32;
        GetTokenInformation(token, TokenUser, null_mut(), 0, &mut len);
        let mut buf = vec![0u64; (len as usize).div_ceil(8)];
        let ok = GetTokenInformation(token, TokenUser, buf.as_mut_ptr().cast(), len, &mut len);
        let err = io::Error::last_os_error();
        CloseHandle(token);
        if ok == 0 {
            return Err(err).context("reading the current user's SID");
        }
        let user = &*(buf.as_ptr() as *const TOKEN_USER);
        sid_to_string(user.User.Sid)
    }
}

/// # Safety
/// `sid` must point at a valid SID.
unsafe fn sid_to_string(sid: PSID) -> anyhow::Result<String> {
    let mut wide = null_mut();
    // SAFETY: caller guarantees `sid`; the output is LocalFree'd below.
    unsafe {
        if ConvertSidToStringSidW(sid, &mut wide) == 0 {
            return Err(io::Error::last_os_error()).context("formatting SID");
        }
        let len = (0..).take_while(|&i| *wide.add(i) != 0).count();
        let s = String::from_utf16_lossy(std::slice::from_raw_parts(wide, len));
        LocalFree(wide.cast());
        Ok(s)
    }
}

/// A security descriptor allocated by `ConvertStringSecurityDescriptor...`.
struct OwnedSd(PSECURITY_DESCRIPTOR);

// SAFETY: the descriptor is immutable after creation and only read by the
// kernel when creating pipe instances.
unsafe impl Send for OwnedSd {}
unsafe impl Sync for OwnedSd {}

impl Drop for OwnedSd {
    fn drop(&mut self) {
        // SAFETY: allocated with LocalAlloc by the conversion function.
        unsafe { LocalFree(self.0) };
    }
}

fn pipe_security(sid: &str) -> anyhow::Result<OwnedSd> {
    // O: our SID, so clients can verify the owner. D:P: protected DACL (no
    // inherited ACEs) granting full access to SYSTEM and us — nobody else.
    let sddl = format!("O:{sid}D:P(A;;GA;;;SY)(A;;GA;;;{sid})");
    let wide: Vec<u16> = sddl.encode_utf16().chain(std::iter::once(0)).collect();
    let mut sd: PSECURITY_DESCRIPTOR = null_mut();
    // SAFETY: `wide` is NUL-terminated; `sd` receives a LocalAlloc'd pointer.
    let ok = unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            wide.as_ptr(),
            SDDL_REVISION_1,
            &mut sd,
            null_mut(),
        )
    };
    if ok == 0 {
        return Err(io::Error::last_os_error()).context("building the pipe security descriptor");
    }
    Ok(OwnedSd(sd))
}

fn create_instance(pipe: &str, sd: &OwnedSd, first: bool) -> io::Result<NamedPipeServer> {
    let mut sa = SECURITY_ATTRIBUTES {
        nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: sd.0,
        bInheritHandle: 0,
    };
    // SAFETY: `sa` is a valid SECURITY_ATTRIBUTES pointing at a live
    // descriptor; the kernel copies it during the call.
    unsafe {
        ServerOptions::new()
            .first_pipe_instance(first)
            .reject_remote_clients(true)
            .create_with_security_attributes_raw(pipe, (&mut sa as *mut SECURITY_ATTRIBUTES).cast())
    }
}

pub struct Listener {
    pipe: String,
    sd: OwnedSd,
    /// Always one instance waiting, so clients never see "no such pipe"
    /// between accepts.
    next: NamedPipeServer,
}

impl Listener {
    /// Creates the first pipe instance; fails if the name already exists,
    /// whether that is our live server or an impostor.
    pub async fn bind(ep: &Endpoint) -> anyhow::Result<Listener> {
        let sd = pipe_security(&current_user_sid()?)?;
        let next = match create_instance(&ep.pipe, &sd, true) {
            Ok(p) => p,
            Err(e) if e.raw_os_error() == Some(ERROR_ACCESS_DENIED as i32) => {
                bail!(
                    "an rmx server is already running on {} (or another process holds that pipe name)",
                    ep.pipe
                );
            }
            Err(e) => return Err(e).with_context(|| format!("creating pipe {}", ep.pipe)),
        };
        Ok(Listener {
            pipe: ep.pipe.clone(),
            sd,
            next,
        })
    }

    /// Waits for the next client. The DACL already restricts who can open
    /// the pipe, so there is no per-peer check here.
    pub async fn accept(&mut self) -> anyhow::Result<ServerEnd> {
        loop {
            let res = self.next.connect().await;
            let fresh = create_instance(&self.pipe, &self.sd, false)
                .with_context(|| format!("creating next instance of {}", self.pipe))?;
            let conn = std::mem::replace(&mut self.next, fresh);
            match res {
                Ok(()) => return Ok(server_end(conn)),
                // A client (e.g. an `is_alive` probe) connected and left
                // before we got to it; that instance is spent, use the fresh one.
                Err(e) if e.raw_os_error() == Some(ERROR_NO_DATA as i32) => continue,
                Err(e) => return Err(e).context("accepting a client"),
            }
        }
    }
}

fn open(pipe: &str) -> io::Result<NamedPipeClient> {
    ClientOptions::new().open(pipe)
}

fn is_busy(e: &io::Error) -> bool {
    e.raw_os_error() == Some(ERROR_PIPE_BUSY as i32)
}

/// Refuses a pipe not owned by our SID: someone else got the name first.
fn verify_owner(client: &NamedPipeClient) -> anyhow::Result<()> {
    let mut owner: PSID = null_mut();
    let mut sd: PSECURITY_DESCRIPTOR = null_mut();
    // SAFETY: valid handle; `owner` points into `sd`, freed after use.
    let owner_sid = unsafe {
        let rc = GetSecurityInfo(
            client.as_raw_handle() as HANDLE,
            SE_KERNEL_OBJECT,
            OWNER_SECURITY_INFORMATION,
            &mut owner,
            null_mut(),
            null_mut(),
            null_mut(),
            &mut sd,
        );
        if rc != ERROR_SUCCESS {
            return Err(io::Error::from_raw_os_error(rc as i32))
                .context("reading the pipe's owner");
        }
        let s = sid_to_string(owner);
        LocalFree(sd);
        s?
    };
    let me = current_user_sid()?;
    if owner_sid != me {
        bail!("pipe is owned by {owner_sid}, not us ({me}); refusing to connect");
    }
    Ok(())
}

pub(crate) async fn connect(ep: &Endpoint) -> anyhow::Result<ClientEnd> {
    let deadline = Instant::now() + Duration::from_secs(1);
    let client = loop {
        match open(&ep.pipe) {
            Ok(c) => break c,
            // Every instance is mid-handoff; the server creates another
            // right after each accept, so this clears quickly.
            Err(e) if is_busy(&e) && Instant::now() < deadline => {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
            Err(e) => return Err(e).with_context(|| format!("connecting to {}", ep.pipe)),
        }
    };
    verify_owner(&client).with_context(|| format!("connecting to {}", ep.pipe))?;
    Ok(client_end(client))
}

pub(crate) async fn is_alive(ep: &Endpoint) -> bool {
    match open(&ep.pipe) {
        Ok(_) => true,
        Err(e) => is_busy(&e),
    }
}

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

#[cfg(test)]
mod tests {
    use super::*;
    use rmx_proto::{ClientMsg, ServerMsg};
    use std::sync::atomic::{AtomicU32, Ordering};

    fn unique_ep() -> Endpoint {
        static N: AtomicU32 = AtomicU32::new(0);
        endpoint_at(format!(
            r"\\.\pipe\rmx-ipc-test-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ))
    }

    #[test]
    fn endpoint_contains_sid_and_name() {
        let ep = crate::endpoint("work").unwrap();
        assert!(
            ep.display().starts_with(r"\\.\pipe\rmx-S-1-"),
            "{}",
            ep.display()
        );
        assert!(ep.display().ends_with("-work"));
    }

    #[test]
    fn breakaway_flag_is_optional() {
        assert_ne!(creation_flags(true) & CREATE_BREAKAWAY_FROM_JOB, 0);
        assert_eq!(creation_flags(false) & CREATE_BREAKAWAY_FROM_JOB, 0);
        let cmd = detached_command(Path::new(r"C:\rmx.exe"), &["__server"], true);
        assert_eq!(cmd.get_args().collect::<Vec<_>>(), ["__server"]);
    }

    #[tokio::test]
    async fn round_trips_messages_both_ways() {
        let ep = unique_ep();
        let mut l = Listener::bind(&ep).await.unwrap();
        let (client, server) = tokio::join!(connect(&ep), l.accept());
        let (mut client, mut server) = (client.unwrap(), server.unwrap());
        client.tx.send(ClientMsg::Detach).await.unwrap();
        client
            .tx
            .send(ClientMsg::Paste("x".repeat(1 << 20)))
            .await
            .unwrap();
        assert_eq!(server.rx.recv().await, Some(ClientMsg::Detach));
        assert_eq!(
            server.rx.recv().await,
            Some(ClientMsg::Paste("x".repeat(1 << 20)))
        );
        server.tx.send(ServerMsg::Bell).await.unwrap();
        assert_eq!(client.rx.recv().await, Some(ServerMsg::Bell));
        drop(server.tx);
        assert_eq!(client.rx.recv().await, None);
        drop(client);
        assert_eq!(server.rx.recv().await, None);
    }

    #[tokio::test]
    async fn second_bind_errors_and_is_alive_tracks_listener() {
        let ep = unique_ep();
        assert!(!is_alive(&ep).await);
        let l = Listener::bind(&ep).await.unwrap();
        assert!(is_alive(&ep).await);
        assert!(Listener::bind(&ep).await.is_err());
        // The probes above connected and left; accept must skip them.
        let mut l = l;
        let (client, server) = tokio::join!(connect(&ep), l.accept());
        client.unwrap().tx.send(ClientMsg::Detach).await.unwrap();
        assert_eq!(server.unwrap().rx.recv().await, Some(ClientMsg::Detach));
        drop(l);
        assert!(!is_alive(&ep).await);
    }
}
