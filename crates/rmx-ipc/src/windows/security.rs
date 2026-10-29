//! The pipe's security descriptor (who may open it) and the owner check
//! clients make before trusting a pipe.

use std::io;
use std::os::windows::io::AsRawHandle;
use std::ptr::null_mut;

use anyhow::{Context, bail};
use tokio::net::windows::named_pipe::NamedPipeClient;
use windows_sys::Win32::Foundation::{ERROR_SUCCESS, HANDLE, LocalFree};
use windows_sys::Win32::Security::Authorization::{
    ConvertStringSecurityDescriptorToSecurityDescriptorW, GetSecurityInfo, SDDL_REVISION_1,
    SE_KERNEL_OBJECT,
};
use windows_sys::Win32::Security::{OWNER_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR, PSID};

use super::sid::{current_user_sid, sid_to_string};

/// A security descriptor allocated by `ConvertStringSecurityDescriptor...`.
pub(super) struct OwnedSd(pub(super) PSECURITY_DESCRIPTOR);

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

pub(super) fn pipe_security(sid: &str) -> anyhow::Result<OwnedSd> {
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

/// Refuses a pipe not owned by our SID: someone else got the name first.
pub(super) fn verify_owner(client: &NamedPipeClient) -> anyhow::Result<()> {
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
