//! `rmx update`: replace this binary with the latest GitHub release.
//!
//! Downloads go through `curl` and archives through `tar`, which Linux,
//! macOS and Windows 10+ all ship, so rmx needs no HTTP or TLS crate and
//! stays a small binary. The archive's SHA-256 is checked against the
//! `.sha256` file published next to it before anything is replaced.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};
use sha2::{Digest, Sha256};

const REPO: &str = "Energma/rust-terminal-multiplexer";

/// The release target this build updates from. Linux x86_64 takes the static
/// musl build so it runs on any distro.
fn release_target() -> Option<&'static str> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("linux", "x86_64") => Some("x86_64-unknown-linux-musl"),
        ("macos", "aarch64") => Some("aarch64-apple-darwin"),
        ("macos", "x86_64") => Some("x86_64-apple-darwin"),
        ("windows", "x86_64") => Some("x86_64-pc-windows-msvc"),
        ("windows", "aarch64") => Some("aarch64-pc-windows-msvc"),
        _ => None,
    }
}

/// Archive name the release workflow publishes for `tag` on this platform.
pub fn asset_name(tag: &str, target: &str) -> String {
    let ext = if target.contains("windows") {
        "zip"
    } else {
        "tar.gz"
    };
    format!("rmx-{tag}-{target}.{ext}")
}

/// Pulls `tag_name` out of GitHub's release JSON.
pub fn latest_tag(json: &str) -> Result<String> {
    let v: serde_json::Value = serde_json::from_str(json).context("parse release JSON")?;
    match v["tag_name"].as_str() {
        Some(t) if !t.is_empty() => Ok(t.to_string()),
        _ => bail!("no release published yet for {REPO}"),
    }
}

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

fn curl(url: &str) -> Result<Vec<u8>> {
    let out = Command::new("curl")
        .args(["-fsSL", "-A", "rmx-update", url])
        .output()
        .context("run curl (is it installed?)")?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        if err.contains("404") {
            bail!("no release published yet for {REPO}");
        }
        bail!("curl {url}: {}", err.trim());
    }
    Ok(out.stdout)
}

/// Extracts the rmx executable from the archive at `path` into `dir`.
fn extract(path: &Path, dir: &Path) -> Result<PathBuf> {
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

/// Runs the update. With `check` it only reports.
pub fn run(check: bool) -> Result<i32> {
    let current = concat!("v", env!("CARGO_PKG_VERSION"));
    let json = String::from_utf8_lossy(&curl(&format!(
        "https://api.github.com/repos/{REPO}/releases/latest"
    ))?)
    .into_owned();
    let tag = latest_tag(&json)?;
    if tag == current {
        println!("rmx {current} is already the latest release");
        return Ok(0);
    }
    println!("current: {current}\nlatest:  {tag}");
    if check {
        return Ok(0);
    }

    let target = release_target().with_context(|| {
        format!(
            "no release build for {}/{}",
            std::env::consts::OS,
            std::env::consts::ARCH
        )
    })?;
    let name = asset_name(&tag, target);
    let base = format!("https://github.com/{REPO}/releases/download/{tag}/{name}");
    println!("downloading {name}...");
    let archive = curl(&base)?;
    let sum = String::from_utf8_lossy(&curl(&format!("{base}.sha256"))?).into_owned();
    verify(&archive, &sum)?;

    let work = std::env::temp_dir().join(format!("rmx-update-{}", std::process::id()));
    fs::create_dir_all(&work)?;
    let result = (|| {
        let archive_path = work.join(&name);
        fs::write(&archive_path, &archive)?;
        let exe = extract(&archive_path, &work)?;
        let payload = fs::read(&exe)?;
        let dest = std::env::current_exe().context("locate running binary")?;
        // Follow a symlinked install so the real file is replaced, not the link.
        let dest = fs::canonicalize(&dest).unwrap_or(dest);
        replace(&dest, &payload)?;
        println!("updated to {tag} at {}", dest.display());
        Ok(0)
    })();
    let _ = fs::remove_dir_all(&work);
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn asset_names_follow_the_release_workflow() {
        assert_eq!(
            asset_name("v0.2.0", "x86_64-unknown-linux-musl"),
            "rmx-v0.2.0-x86_64-unknown-linux-musl.tar.gz"
        );
        assert_eq!(
            asset_name("v0.2.0", "x86_64-pc-windows-msvc"),
            "rmx-v0.2.0-x86_64-pc-windows-msvc.zip"
        );
    }

    #[test]
    fn latest_tag_reads_github_json() {
        assert_eq!(
            latest_tag(r#"{"tag_name":"v0.3.1","assets":[]}"#).unwrap(),
            "v0.3.1"
        );
        assert!(latest_tag(r#"{"message":"Not Found"}"#).is_err());
        assert!(latest_tag("rmx: nope").is_err());
    }

    #[test]
    fn verify_matches_shasum_output() {
        let data = b"payload";
        let hex = Sha256::digest(data)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        assert!(verify(data, &format!("{hex}  rmx-v1.tar.gz\n")).is_ok());
        assert!(verify(b"tampered", &format!("{hex}  rmx-v1.tar.gz\n")).is_err());
        assert!(verify(data, "").is_err());
    }

    #[test]
    fn replace_swaps_the_file_and_leaves_no_temp() {
        let dir = std::env::temp_dir().join(format!("rmx-replace-test-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let dest = dir.join("rmx");
        fs::write(&dest, b"old").unwrap();
        replace(&dest, b"new").unwrap();
        assert_eq!(fs::read(&dest).unwrap(), b"new");
        assert!(!dir.join("rmx.new").exists());
        fs::remove_dir_all(&dir).unwrap();
    }
}
