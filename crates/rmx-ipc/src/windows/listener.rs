//! Creating, accepting on and connecting to pipe instances.

use std::io;
use std::time::Duration;

use anyhow::{Context, bail};
use rmx_proto::{ClientEnd, ServerEnd};
use tokio::net::windows::named_pipe::{
    ClientOptions, NamedPipeClient, NamedPipeServer, ServerOptions,
};
use tokio::time::Instant;
use windows_sys::Win32::Foundation::{ERROR_ACCESS_DENIED, ERROR_NO_DATA, ERROR_PIPE_BUSY};
use windows_sys::Win32::Security::SECURITY_ATTRIBUTES;

use super::security::{OwnedSd, pipe_security, verify_owner};
use super::sid::current_user_sid;
use crate::Endpoint;
use crate::bridge::{client_end, server_end};

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
