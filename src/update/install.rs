//! Installing a downloaded archive: checksum verification, extraction via
//! `tar`, and the crash-safe swap of the running binary.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};
use sha2::{Digest, Sha256};

/// Checks `data` against a `<hex>  <name>` line as `shasum -a 256` prints.
pub fn verify(data: &[u8], sum_line: &str) -> Result<()> {
    let want = sum_line
        .split_whitespace()
        .next()
        .context("empty checksum file")?
        .to_ascii_lowercase();
    let got = Sha256::digest(data)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    if got != want {
        bail!("checksum mismatch: archive is corrupt or tampered");
    }
    Ok(())
}

/// Writes `payload` next to `dest` and renames it over `dest`, so a crash
/// mid-write never leaves a half binary. Windows won't overwrite a running
/// exe, so there the old one is moved aside and removed on the next update.
pub fn replace(dest: &Path, payload: &[u8]) -> Result<()> {
    let tmp = dest.with_extension("new");
    fs::write(&tmp, payload)
        .with_context(|| format!("write {} (try with sudo?)", tmp.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&tmp, fs::Permissions::from_mode(0o755))?;
    }
    if cfg!(windows) {
        let old = dest.with_extension("old");
        let _ = fs::remove_file(&old);
        if dest.exists() {
            fs::rename(dest, &old).context("move old binary aside")?;
        }
    }
    fs::rename(&tmp, dest).with_context(|| format!("replace {}", dest.display()))?;
    Ok(())
}

/// Extracts the rmx executable from the archive at `path` into `dir`.
pub(super) fn extract(path: &Path, dir: &Path) -> Result<PathBuf> {
    let status = Command::new("tar")
        .arg("-xf")
        .arg(path)
        .arg("-C")
        .arg(dir)
        .status()
        .context("run tar (is it installed?)")?;
    if !status.success() {
        bail!("tar failed to extract {}", path.display());
    }
    let exe = if cfg!(windows) { "rmx.exe" } else { "rmx" };
    for entry in fs::read_dir(dir)? {
        let candidate = entry?.path().join(exe);
        if candidate.is_file() {
            return Ok(candidate);
        }
    }
    bail!("{exe} not found in archive")
}
