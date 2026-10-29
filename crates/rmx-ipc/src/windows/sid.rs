//! The current user's SID, and the per-user pipe names derived from it.

use std::io;
use std::ptr::null_mut;

use anyhow::Context;
use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, LocalFree};
use windows_sys::Win32::Security::Authorization::ConvertSidToStringSidW;
use windows_sys::Win32::Security::{GetTokenInformation, PSID, TOKEN_QUERY, TOKEN_USER, TokenUser};
use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

use crate::Endpoint;

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

pub(super) fn current_user_sid() -> anyhow::Result<String> {
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
pub(super) unsafe fn sid_to_string(sid: PSID) -> anyhow::Result<String> {
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
