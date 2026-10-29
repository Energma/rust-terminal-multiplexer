//! File-based tracing setup shared by the client and the background server.

use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

use anyhow::{Context, Result};
use tracing_subscriber::EnvFilter;

pub(crate) fn log_dir() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("rmx")
}

/// Logs go to a file: the terminal belongs to the panes, and the server has
/// no terminal at all. Level via `RMX_LOG`.
pub(crate) fn init_logging(name: &str) -> Result<PathBuf> {
    let dir = log_dir();
    fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
    let path = dir.join(format!("{name}.log"));
    // Append: several clients may log at once, and a restart shouldn't
    // erase the evidence of what went wrong before it.
    let file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .with_context(|| format!("opening {}", path.display()))?;
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_env("RMX_LOG").unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .with_writer(Mutex::new(file))
        .with_ansi(false)
        .init();
    Ok(path)
}
