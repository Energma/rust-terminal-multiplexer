//! Config commands: `rmx keys` and the `rmx config` subcommands.

use std::fs;
use std::path::Path;

use anyhow::Result;
use rmx_config::Config;
use rmx_proto::Request;
use tokio::runtime::Runtime;

use super::args::ConfigCmd;
use super::ipc::request_existing;

fn load_config(path: &Path) -> Result<Config> {
    let (config, warnings) = Config::load(path).map_err(|e| anyhow::anyhow!("\n{e}"))?;
    for w in warnings {
        eprintln!("{w}");
    }
    Ok(config)
}

pub(super) fn print_keys(path: &Path, only: Option<&str>) -> Result<()> {
    let config = load_config(path)?;
    let direct = config.direct_enabled(false);
    let keymap = config.keymap(direct);
    println!(
        "leader: {}   (preset: {})",
        keymap.leader().display_short(),
        config.preset
    );
    for mode in keymap.modes() {
        if only.is_some_and(|m| m != mode) {
            continue;
        }
        let bindings = keymap.bindings(mode);
        if bindings.is_empty() {
            continue;
        }
        println!("\n[{mode}]");
        for b in bindings {
            let source = match b.source {
                rmx_config::Source::User => "  (yours)",
                rmx_config::Source::Preset => "",
            };
            println!(
                "  {:<22} {}{source}",
                rmx_config::display_sequence(&b.keys),
                b.action.label()
            );
        }
    }
    Ok(())
}

pub(super) fn config_cmd(
    runtime: &Runtime,
    server: &str,
    path: &Path,
    action: ConfigCmd,
) -> Result<i32> {
    match action {
        ConfigCmd::Path => println!("{}", path.display()),
        ConfigCmd::Check => match Config::load(path) {
            Ok((_, warnings)) => {
                for w in &warnings {
                    eprintln!("{w}");
                }
                if !path.exists() {
                    println!("{} does not exist; defaults are in use", path.display());
                } else {
                    println!(
                        "{} is valid ({} warning{})",
                        path.display(),
                        warnings.len(),
                        if warnings.len() == 1 { "" } else { "s" }
                    );
                }
            }
            Err(err) => {
                eprintln!("{err}");
                return Ok(1);
            }
        },
        ConfigCmd::Init => {
            if path.exists() {
                println!("{} already exists", path.display());
            } else {
                if let Some(dir) = path.parent() {
                    fs::create_dir_all(dir)?;
                }
                fs::write(path, rmx_config::DEFAULT_CONFIG_TOML)?;
                println!("wrote {}", path.display());
            }
        }
        ConfigCmd::Reload => {
            runtime.block_on(request_existing(server, Request::ReloadConfig))?;
            println!("settings applied");
        }
    }
    Ok(0)
}
