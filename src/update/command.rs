//! The `rmx update` flow: compare versions, download, verify, install.

use std::fs;

use anyhow::{Context, Result};

use super::REPO;
use super::install::{extract, replace, verify};
use super::release::{asset_name, curl, latest_tag, release_target};

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
