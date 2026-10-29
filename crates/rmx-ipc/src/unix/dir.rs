//! Where the socket lives, and the checks that make its directory safe to trust.

use std::fs;
use std::io;
use std::os::unix::fs::{DirBuilderExt, MetadataExt};
use std::path::{Path, PathBuf};

use anyhow::{Context, bail};

use crate::Endpoint;

/// Builds an endpoint at an explicit socket path (tests, `--socket` style overrides).
pub fn endpoint_at(path: impl Into<PathBuf>) -> Endpoint {
    Endpoint { path: path.into() }
}

pub(super) fn euid() -> u32 {
    // SAFETY: geteuid has no preconditions and cannot fail.
    unsafe { libc::geteuid() }
}

pub(crate) fn endpoint(name: &str) -> anyhow::Result<Endpoint> {
    let dir = socket_dir(
        std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from),
        std::env::var_os("TMPDIR").map(PathBuf::from),
        euid(),
    );
    Ok(endpoint_at(dir.join(format!("{name}.sock"))))
}

/// Relative env values are ignored: they'd resolve against whatever folder
/// the client happened to start in, so two clients wouldn't find each other.
pub(super) fn socket_dir(xdg: Option<PathBuf>, tmp: Option<PathBuf>, uid: u32) -> PathBuf {
    if let Some(x) = xdg.filter(|p| p.is_absolute()) {
        return x.join("rmx");
    }
    let base = tmp
        .filter(|p| p.is_absolute())
        .unwrap_or_else(|| PathBuf::from("/tmp"));
    base.join(format!("rmx-{uid}"))
}

pub(super) fn parent(ep: &Endpoint) -> anyhow::Result<&Path> {
    ep.path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .with_context(|| format!("socket path {} has no directory", ep.path.display()))
}

/// Creates `dir` (one level, 0700) if missing, then insists it is safe.
pub(super) fn ensure_dir(dir: &Path) -> anyhow::Result<()> {
    match fs::DirBuilder::new().mode(0o700).create(dir) {
        Ok(()) => {}
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
        Err(e) => return Err(e).with_context(|| format!("creating {}", dir.display())),
    }
    check_dir(dir)
}

/// The directory must be a real directory (not a symlink), owned by us, with
/// no group/other access. Anything else means someone else could plant or
/// hijack our socket, so refuse rather than "fix" it.
pub(crate) fn check_dir(dir: &Path) -> anyhow::Result<()> {
    let meta =
        fs::symlink_metadata(dir).with_context(|| format!("inspecting {}", dir.display()))?;
    if meta.file_type().is_symlink() {
        bail!(
            "{} is a symlink; refusing to use it for the rmx socket",
            dir.display()
        );
    }
    if !meta.is_dir() {
        bail!("{} is not a directory", dir.display());
    }
    if meta.uid() != euid() {
        bail!(
            "{} is owned by uid {}, not us (uid {}); refusing to use it",
            dir.display(),
            meta.uid(),
            euid()
        );
    }
    let mode = meta.mode() & 0o777;
    if mode & 0o077 != 0 {
        bail!(
            "{} has mode {mode:o}; it must be 0700 (run `chmod 700 {}`)",
            dir.display(),
            dir.display()
        );
    }
    Ok(())
}
