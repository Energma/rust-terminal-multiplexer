//! `rmx doctor`: a system report with setup tips.

use std::path::Path;

use rmx_config::Config;
use tokio::runtime::Runtime;

use crate::logging::log_dir;

pub(super) fn doctor(runtime: &Runtime, server: &str, config: &Path) {
    println!(
        "rmx {}  ({} {})",
        env!("CARGO_PKG_VERSION"),
        std::env::consts::OS,
        std::env::consts::ARCH
    );
    match Config::load(config) {
        Ok((c, w)) => println!(
            "config   {}  ok, preset \"{}\", {} warning(s)",
            config.display(),
            c.preset,
            w.len()
        ),
        Err(e) => println!(
            "config   {}  ERROR: {}",
            config.display(),
            e.to_string().lines().next().unwrap_or("")
        ),
    }
    match rmx_ipc::endpoint(server) {
        Ok(ep) => {
            let alive = runtime.block_on(rmx_ipc::is_alive(&ep));
            println!(
                "server   {}  {}",
                ep.display(),
                if alive { "running" } else { "not running" }
            );
        }
        Err(e) => println!("server   ERROR: {e:#}"),
    }
    println!("logs     {}", log_dir().display());
    let term = std::env::var("TERM_PROGRAM")
        .or_else(|_| std::env::var("TERM"))
        .unwrap_or_else(|_| "unknown".into());
    println!("terminal {term}");
    if cfg!(windows) || std::env::var_os("WT_SESSION").is_some() {
        println!(
            "\nWindows Terminal takes some Ctrl+Shift keys before rmx sees them. To use rmx's\n\
             direct keys there, add this to the \"actions\"/\"keybindings\" list in WT's settings.json:\n\n  \
             {{ \"command\": \"unbound\", \"keys\": \"ctrl+shift+t\" }},\n  \
             {{ \"command\": \"unbound\", \"keys\": \"ctrl+shift+w\" }},\n  \
             {{ \"command\": \"unbound\", \"keys\": \"ctrl+shift+p\" }},\n  \
             {{ \"command\": \"unbound\", \"keys\": \"ctrl+shift+f\" }},\n  \
             {{ \"command\": \"unbound\", \"keys\": \"ctrl+tab\" }},\n  \
             {{ \"command\": \"unbound\", \"keys\": \"ctrl+shift+tab\" }},\n  \
             {{ \"command\": \"unbound\", \"keys\": \"alt+left\" }}, (and alt+right/up/down)\n\n\
             The leader key (Ctrl+Space then a letter) works everywhere without changes."
        );
    }
}
