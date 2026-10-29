//! Finding and fetching releases: which build fits this machine, what the
//! latest tag is, and downloads via `curl`.

use std::process::Command;

use anyhow::{Context, Result, bail};

use super::REPO;

/// The release target this build updates from. Linux x86_64 takes the static
/// musl build so it runs on any distro.
pub(super) fn release_target() -> Option<&'static str> {
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

pub(super) fn curl(url: &str) -> Result<Vec<u8>> {
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
