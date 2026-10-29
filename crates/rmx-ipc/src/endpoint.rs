//! Naming and locating a server's endpoint.

use crate::sys;

/// Where a server listens: a socket path on Unix, a pipe name on Windows.
#[derive(Debug, Clone)]
pub struct Endpoint {
    #[cfg(unix)]
    pub(crate) path: std::path::PathBuf,
    #[cfg(windows)]
    pub(crate) pipe: String,
}

impl Endpoint {
    pub fn display(&self) -> String {
        #[cfg(unix)]
        return self.path.display().to_string();
        #[cfg(windows)]
        return self.pipe.clone();
    }

    #[cfg(unix)]
    pub fn path(&self) -> &std::path::Path {
        &self.path
    }
}

/// Where the server for `name` ("default" unless the user passes -L) listens.
///
/// Unix: `$XDG_RUNTIME_DIR/rmx/<name>.sock`, else `$TMPDIR/rmx-<uid>/<name>.sock`,
/// else `/tmp/rmx-<uid>/<name>.sock`. Windows: `\\.\pipe\rmx-<SID>-<name>`.
pub fn endpoint(name: &str) -> anyhow::Result<Endpoint> {
    validate_name(name)?;
    sys::endpoint(name)
}

/// `name` ends up in a path or pipe name, so only allow characters that can't
/// escape the directory or collide with our own suffixes.
pub(super) fn validate_name(name: &str) -> anyhow::Result<()> {
    let ok = !name.is_empty()
        && name.len() <= 64
        && !name.starts_with('.')
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'));
    anyhow::ensure!(
        ok,
        "invalid server name {name:?}: use 1-64 letters, digits, '-', '_' or '.', not starting with '.'"
    );
    Ok(())
}
