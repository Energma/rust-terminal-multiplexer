//! Routes a parsed command line to its handler.

use anyhow::Result;
use rmx_config::Config;
use rmx_proto::{Attach, Request};

use super::ipc::{events, simple};
use super::{Cli, Command, config, doctor, server, session, workspace};
use crate::update;

pub(crate) fn run(cli: Cli) -> Result<i32> {
    let config_path = cli.config.clone().unwrap_or_else(Config::default_path);
    // Absolute, because the background server runs from another directory.
    let config_path = std::path::absolute(&config_path).unwrap_or(config_path);
    let server = cli.server.clone();
    let runtime = tokio::runtime::Runtime::new()?;
    match cli.command {
        Some(Command::Server { restore }) => {
            runtime.block_on(server::run_server(&server, config_path, restore))?;
            Ok(0)
        }
        Some(Command::Update { check }) => update::run(check),
        // `rmx -n [name]`: a new background workspace in this folder.
        None if cli.new.is_some() => {
            session::new_background(&runtime, &server, &config_path, cli.new)
        }
        // Plain `rmx` is the session manager: see every session, open one,
        // or start a new one in this folder.
        None => session::interactive(&runtime, &server, &config_path, Attach::Last, true),
        Some(Command::Attach { name }) => {
            let attach = name.map_or(Attach::Last, Attach::Named);
            session::interactive(&runtime, &server, &config_path, attach, false)
        }
        Some(Command::Manage) => {
            session::interactive(&runtime, &server, &config_path, Attach::Last, true)
        }
        Some(Command::New {
            name,
            cwd,
            detached,
            command,
        }) => session::new_workspace(
            &runtime,
            &server,
            &config_path,
            name,
            cwd,
            detached,
            command,
        ),
        Some(Command::List { json }) => workspace::list(&runtime, &server, json),
        Some(Command::Close { name }) => {
            simple(&runtime, &server, Request::CloseWorkspace { name })
        }
        Some(Command::Rename { from, to }) => {
            simple(&runtime, &server, Request::RenameWorkspace { from, to })
        }
        Some(Command::Send {
            target,
            enter,
            text,
        }) => workspace::send(&runtime, &server, target, enter, text),
        Some(Command::Capture { target }) => workspace::capture(&runtime, &server, target),
        Some(Command::Save) => workspace::save(&runtime, &server),
        Some(Command::Restore) => session::restore(&runtime, &server, &config_path),
        Some(Command::Events) => {
            runtime.block_on(events(&server))?;
            Ok(0)
        }
        Some(Command::Keys { mode }) => {
            config::print_keys(&config_path, mode.as_deref())?;
            Ok(0)
        }
        Some(Command::Config { action }) => {
            config::config_cmd(&runtime, &server, &config_path, action)
        }
        Some(Command::Doctor) => {
            doctor::doctor(&runtime, &server, &config_path);
            Ok(0)
        }
        Some(Command::Stop) => session::stop(&runtime, &server),
    }
}
