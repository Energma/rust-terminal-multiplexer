//! The background server: how it is launched and its accept loop.

use std::path::PathBuf;

use anyhow::Result;

use crate::logging::init_logging;

/// Runs the server: accepts connections and feeds them to the core.
pub(super) async fn run_server(name: &str, config_path: PathBuf, restore: bool) -> Result<()> {
    let ep = rmx_ipc::endpoint(name)?;
    let mut listener = rmx_ipc::Listener::bind(&ep).await?;
    // Only after winning the bind, so a losing duplicate can't touch the
    // live server's log.
    init_logging(&format!("server-{name}"))?;
    tracing::info!(endpoint = %ep.display(), "server listening");
    let (tx, rx) = tokio::sync::mpsc::channel(16);
    tokio::spawn(async move {
        loop {
            match listener.accept().await {
                Ok(conn) => {
                    if tx.send(conn).await.is_err() {
                        break;
                    }
                }
                Err(err) => tracing::warn!("accept failed: {err:#}"),
            }
        }
    });
    let mut opts = rmx_server::ServerOptions::new(name, config_path);
    opts.restore = restore;
    rmx_server::run(rx, opts).await
}

/// Arguments for the background server. `--opt=value` form so values that
/// start with `-` aren't mistaken for flags.
pub(super) fn server_args(name: &str, config: &std::path::Path) -> Result<Vec<String>> {
    let config = config.to_str().ok_or_else(|| {
        anyhow::anyhow!("the config path {} is not valid UTF-8", config.display())
    })?;
    Ok(vec![
        format!("--server={name}"),
        format!("--config={config}"),
        "__server".into(),
    ])
}
