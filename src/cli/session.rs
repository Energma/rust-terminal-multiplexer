//! Workspace lifecycle commands: attaching the interactive client, creating
//! and restoring workspaces, and stopping the server.

use std::path::Path;

use anyhow::{Result, bail};
use rmx_proto::{Attach, Request};
use tokio::runtime::Runtime;

use super::ipc::{request, request_existing};
use super::server::server_args;
use crate::logging::init_logging;

pub(super) fn interactive(
    runtime: &Runtime,
    server: &str,
    config: &Path,
    attach: Attach,
    open_manager: bool,
) -> Result<i32> {
    if std::env::var_os("RMX").is_some() {
        bail!("already inside rmx (unset RMX to nest anyway)");
    }
    init_logging("client")?;
    let args = server_args(server, config)?;
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let reason = runtime.block_on(async {
        let ep = rmx_ipc::endpoint(server)?;
        let (conn, spawned) = rmx_ipc::connect_or_spawn(&ep, &args).await?;
        if spawned.is_some_and(|s| !s.breakaway) {
            eprintln!(
                "rmx: note: this terminal does not let programs outlive it, so closing it will also close your workspaces"
            );
        }
        let cwd = std::env::current_dir().ok().map(|d| d.display().to_string());
        rmx_client::run(conn, rmx_client::Options { attach, open_manager, cwd }).await
    })?;
    println!("[rmx: {reason}]");
    // The input thread may still be blocked reading the terminal; don't wait for it.
    Ok(0)
}

/// `rmx -n [name]`: a new background workspace in this folder.
pub(super) fn new_background(
    runtime: &Runtime,
    server: &str,
    config: &Path,
    new: Option<String>,
) -> Result<i32> {
    let name = new.filter(|n| !n.is_empty());
    let cwd = std::env::current_dir()
        .ok()
        .map(|d| d.display().to_string());
    let body = runtime.block_on(request(
        server,
        config,
        Request::NewWorkspace {
            name,
            cwd,
            command: None,
        },
    ))?;
    let v: serde_json::Value = serde_json::from_str(&body)?;
    println!(
        "created workspace \"{}\" (open it with `rmx`)",
        v["name"].as_str().unwrap_or("?")
    );
    Ok(0)
}

/// `rmx new`: attach to a fresh workspace, or with `--detached` create it
/// in the background (optionally running `command`) and print its details.
pub(super) fn new_workspace(
    runtime: &Runtime,
    server: &str,
    config: &Path,
    name: Option<String>,
    cwd: Option<String>,
    detached: bool,
    command: Vec<String>,
) -> Result<i32> {
    let cwd = cwd.or_else(|| {
        std::env::current_dir()
            .ok()
            .map(|d| d.display().to_string())
    });
    if detached {
        let command = (!command.is_empty()).then(|| shell_join(&command));
        let body = runtime.block_on(request(
            server,
            config,
            Request::NewWorkspace { name, cwd, command },
        ))?;
        println!("{body}");
        Ok(0)
    } else {
        anyhow::ensure!(
            command.is_empty(),
            "a command can only be given with --detached"
        );
        interactive(runtime, server, config, Attach::New { name, cwd }, false)
    }
}

pub(super) fn restore(runtime: &Runtime, server: &str, config: &Path) -> Result<i32> {
    let body = runtime.block_on(request(server, config, Request::Restore))?;
    let v: serde_json::Value = serde_json::from_str(&body)?;
    println!(
        "restored {} workspace(s); run `rmx` to attach",
        v["restored_workspaces"]
    );
    Ok(0)
}

pub(super) fn stop(runtime: &Runtime, server: &str) -> Result<i32> {
    let ep = rmx_ipc::endpoint(server)?;
    if !runtime.block_on(rmx_ipc::is_alive(&ep)) {
        println!("no server running");
        return Ok(0);
    }
    runtime.block_on(request_existing(server, Request::Stop))?;
    // Wait until it's really gone so a following command doesn't
    // race the shutdown.
    runtime.block_on(async {
        for _ in 0..40 {
            if !rmx_ipc::is_alive(&ep).await {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
    });
    if runtime.block_on(rmx_ipc::is_alive(&ep)) {
        println!("server is still shutting down");
    } else {
        println!("server stopped");
    }
    Ok(0)
}

/// Joins `rmx new -d -- prog args…` into one command line for the pane's
/// shell, quoting each argument so `"a b"` stays one argument. A single
/// argument is taken as a complete command line (`rmx new -d -- "make && run"`).
pub(super) fn shell_join(args: &[String]) -> String {
    if let [only] = args {
        return only.clone();
    }
    args.iter()
        .map(|a| shell_quote(a))
        .collect::<Vec<_>>()
        .join(" ")
}

fn shell_quote(arg: &str) -> String {
    let safe = |c: char| c.is_ascii_alphanumeric() || "_-./=:,@%+".contains(c);
    if !arg.is_empty() && arg.chars().all(safe) {
        return arg.to_owned();
    }
    if cfg!(windows) {
        // cmd.exe: double quotes, with embedded quotes doubled.
        format!("\"{}\"", arg.replace('"', "\"\""))
    } else {
        format!("'{}'", arg.replace('\'', "'\\''"))
    }
}
