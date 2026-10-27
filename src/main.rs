//! `rmx` — the rust-multiplexer command.
//!
//! Plain `rmx` attaches to the last workspace (starting the background
//! server if needed). Subcommands use Windows-friendly names, and tmux's
//! names work as aliases so muscle memory carries over.

use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use rmx_config::Config;
use rmx_proto::{Attach, ClientMsg, Request, ServerMsg};
use tracing_subscriber::EnvFilter;

mod update;

#[derive(Parser)]
#[command(
    name = "rmx",
    version,
    about = "A cross-platform terminal multiplexer with menus and custom keys"
)]
struct Cli {
    /// Use a separate server with this name (like tmux -L).
    /// Inside an rmx pane this defaults to that pane's server.
    #[arg(
        short = 'L',
        long = "server",
        global = true,
        env = "RMX_SERVER",
        default_value = "default"
    )]
    server: String,

    /// Config file (default: the platform config dir, or $RMX_CONFIG).
    #[arg(long, global = true)]
    config: Option<PathBuf>,

    /// Create a new workspace in the background, in the current folder
    /// (named after it unless NAME is given), and return to the shell.
    #[arg(short = 'n', long = "new", value_name = "NAME", num_args = 0..=1, default_missing_value = "")]
    new: Option<String>,

    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Start a new workspace and attach to it.
    #[command(visible_alias = "new-session")]
    New {
        /// Workspace name.
        #[arg(short, long)]
        name: Option<String>,
        /// Starting directory.
        #[arg(short = 'c', long)]
        cwd: Option<String>,
        /// Create it in the background without attaching.
        #[arg(short, long)]
        detached: bool,
        /// Run this command instead of a shell (detached mode only).
        #[arg(last = true)]
        command: Vec<String>,
    },
    /// Attach straight to a workspace (default: the most recently used),
    /// skipping the session manager that plain `rmx` shows.
    #[command(visible_aliases = ["a", "attach-session"])]
    Attach { name: Option<String> },
    /// Open the session manager to browse, open, split and close workspaces.
    #[command(visible_aliases = ["sessions", "choose-tree"])]
    Manage,
    /// List workspaces, tabs and panes.
    #[command(visible_aliases = ["ls", "list-sessions"])]
    List {
        #[arg(long)]
        json: bool,
    },
    /// Close a workspace and everything in it.
    #[command(visible_alias = "kill-session")]
    Close { name: String },
    /// Rename a workspace.
    #[command(visible_alias = "rename-session")]
    Rename { from: String, to: String },
    /// Type text into a pane. Escapes: \n \r \t \e. TARGET is
    /// workspace[:tab[.pane]] (1-based) or %<pane id>.
    #[command(visible_alias = "send-keys")]
    Send {
        #[arg(short, long)]
        target: Option<String>,
        /// Press Enter after the text.
        #[arg(short, long)]
        enter: bool,
        text: Vec<String>,
    },
    /// Print a pane's screen contents.
    #[command(visible_alias = "capture-pane")]
    Capture {
        #[arg(short, long)]
        target: Option<String>,
    },
    /// Save workspaces (layouts, folders, commands) for `rmx restore`.
    Save,
    /// Recreate saved workspaces, e.g. after a reboot.
    Restore,
    /// Stream events (JSON lines) for scripts and integrations.
    Events,
    /// Show the effective key bindings.
    #[command(visible_alias = "list-keys")]
    Keys {
        /// Only this mode (normal, leader, select, resize, locked).
        #[arg(long)]
        mode: Option<String>,
    },
    /// Manage the config file.
    Config {
        #[command(subcommand)]
        action: ConfigCmd,
    },
    /// Check this system and print setup tips.
    Doctor,
    /// Stop the background server and close everything.
    #[command(visible_alias = "kill-server")]
    Stop,
    /// Replace this binary with the latest GitHub release.
    Update {
        /// Only report whether a newer release exists.
        #[arg(long)]
        check: bool,
    },
    #[command(name = "__server", hide = true)]
    Server {
        #[arg(long)]
        restore: bool,
    },
}

#[derive(Subcommand)]
enum ConfigCmd {
    /// Print the config file path.
    Path,
    /// Validate the config; exits non-zero on errors (for CI).
    Check,
    /// Write a commented starter config if none exists.
    Init,
    /// Apply the config to the running server now.
    Reload,
}

fn main() {
    let cli = Cli::parse();
    // CLI output is often piped (`rmx keys | head`); exit quietly when the
    // reader goes away instead of panicking. The server keeps ignoring
    // SIGPIPE, since a vanished client must not kill it.
    #[cfg(unix)]
    if !matches!(cli.command, Some(Command::Server { .. })) {
        unsafe {
            libc::signal(libc::SIGPIPE, libc::SIG_DFL);
        }
    }
    match real_main(cli) {
        Ok(code) => std::process::exit(code),
        Err(err) => {
            eprintln!("rmx: {err:#}");
            std::process::exit(1);
        }
    }
}

fn real_main(cli: Cli) -> Result<i32> {
    let config_path = cli.config.clone().unwrap_or_else(Config::default_path);
    // Absolute, because the background server runs from another directory.
    let config_path = std::path::absolute(&config_path).unwrap_or(config_path);
    let server = cli.server.clone();
    let runtime = tokio::runtime::Runtime::new()?;
    match cli.command {
        Some(Command::Server { restore }) => {
            runtime.block_on(run_server(&server, config_path, restore))?;
            Ok(0)
        }
        Some(Command::Update { check }) => update::run(check),
        // `rmx -n [name]`: a new background workspace in this folder.
        None if cli.new.is_some() => {
            let name = cli.new.filter(|n| !n.is_empty());
            let cwd = std::env::current_dir()
                .ok()
                .map(|d| d.display().to_string());
            let body = runtime.block_on(request(
                &server,
                &config_path,
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
        // Plain `rmx` is the session manager: see every session, open one,
        // or start a new one in this folder.
        None => interactive(&runtime, &server, &config_path, Attach::Last, true),
        Some(Command::Attach { name }) => {
            let attach = name.map_or(Attach::Last, Attach::Named);
            interactive(&runtime, &server, &config_path, attach, false)
        }
        Some(Command::Manage) => interactive(&runtime, &server, &config_path, Attach::Last, true),
        Some(Command::New {
            name,
            cwd,
            detached,
            command,
        }) => {
            let cwd = cwd.or_else(|| {
                std::env::current_dir()
                    .ok()
                    .map(|d| d.display().to_string())
            });
            if detached {
                let command = (!command.is_empty()).then(|| shell_join(&command));
                let body = runtime.block_on(request(
                    &server,
                    &config_path,
                    Request::NewWorkspace { name, cwd, command },
                ))?;
                println!("{body}");
                Ok(0)
            } else {
                anyhow::ensure!(
                    command.is_empty(),
                    "a command can only be given with --detached"
                );
                interactive(
                    &runtime,
                    &server,
                    &config_path,
                    Attach::New { name, cwd },
                    false,
                )
            }
        }
        Some(Command::List { json }) => {
            let body = runtime.block_on(request_existing(&server, Request::List))?;
            if json {
                println!("{body}");
            } else {
                print_list(&body)?;
            }
            Ok(0)
        }
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
        }) => {
            let mut text = text.join(" ");
            if enter {
                text.push('\r');
            }
            simple(&runtime, &server, Request::SendKeys { target, text })
        }
        Some(Command::Capture { target }) => {
            let body = runtime.block_on(request_existing(&server, Request::Capture { target }))?;
            let v: serde_json::Value = serde_json::from_str(&body)?;
            print!("{}", v["text"].as_str().unwrap_or_default());
            Ok(0)
        }
        Some(Command::Save) => {
            let body = runtime.block_on(request_existing(&server, Request::Save))?;
            let v: serde_json::Value = serde_json::from_str(&body)?;
            println!("saved to {}", v["path"].as_str().unwrap_or("?"));
            Ok(0)
        }
        Some(Command::Restore) => {
            let body = runtime.block_on(request(&server, &config_path, Request::Restore))?;
            let v: serde_json::Value = serde_json::from_str(&body)?;
            println!(
                "restored {} workspace(s); run `rmx` to attach",
                v["restored_workspaces"]
            );
            Ok(0)
        }
        Some(Command::Events) => {
            runtime.block_on(events(&server))?;
            Ok(0)
        }
        Some(Command::Keys { mode }) => {
            print_keys(&config_path, mode.as_deref())?;
            Ok(0)
        }
        Some(Command::Config { action }) => config_cmd(&runtime, &server, &config_path, action),
        Some(Command::Doctor) => {
            doctor(&runtime, &server, &config_path);
            Ok(0)
        }
        Some(Command::Stop) => {
            let ep = rmx_ipc::endpoint(&server)?;
            if !runtime.block_on(rmx_ipc::is_alive(&ep)) {
                println!("no server running");
                return Ok(0);
            }
            runtime.block_on(request_existing(&server, Request::Stop))?;
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
    }
}

/// Joins `rmx new -d -- prog args…` into one command line for the pane's
/// shell, quoting each argument so `"a b"` stays one argument. A single
/// argument is taken as a complete command line (`rmx new -d -- "make && run"`).
fn shell_join(args: &[String]) -> String {
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

/// Runs the server: accepts connections and feeds them to the core.
async fn run_server(name: &str, config_path: PathBuf, restore: bool) -> Result<()> {
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
fn server_args(name: &str, config: &std::path::Path) -> Result<Vec<String>> {
    let config = config.to_str().ok_or_else(|| {
        anyhow::anyhow!("the config path {} is not valid UTF-8", config.display())
    })?;
    Ok(vec![
        format!("--server={name}"),
        format!("--config={config}"),
        "__server".into(),
    ])
}

fn interactive(
    runtime: &tokio::runtime::Runtime,
    server: &str,
    config: &std::path::Path,
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

/// Sends one request to a server, starting one if needed.
async fn request(server: &str, config: &std::path::Path, req: Request) -> Result<String> {
    let ep = rmx_ipc::endpoint(server)?;
    let args = server_args(server, config)?;
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let (conn, _) = rmx_ipc::connect_or_spawn(&ep, &args).await?;
    roundtrip(conn, req).await
}

/// Sends one request to a running server; errors helpfully if there is none.
async fn request_existing(server: &str, req: Request) -> Result<String> {
    let ep = rmx_ipc::endpoint(server)?;
    if !rmx_ipc::is_alive(&ep).await {
        bail!("no rmx server is running (start one with `rmx`)");
    }
    roundtrip(rmx_ipc::connect(&ep).await?, req).await
}

async fn roundtrip(mut conn: rmx_proto::ClientEnd, req: Request) -> Result<String> {
    conn.tx.send(ClientMsg::request(req)).await?;
    loop {
        match conn.rx.recv().await {
            Some(ServerMsg::Response { ok: true, body }) => return Ok(body),
            Some(ServerMsg::Response { ok: false, body }) => bail!("{body}"),
            Some(ServerMsg::Exit { reason }) => bail!("{reason}"),
            Some(_) => continue,
            None => bail!(
                "the server closed the connection; it may be shutting down, or be a different \
                 rmx version (run `rmx stop`, then retry)"
            ),
        }
    }
}

fn simple(runtime: &tokio::runtime::Runtime, server: &str, req: Request) -> Result<i32> {
    runtime.block_on(request_existing(server, req))?;
    Ok(0)
}

async fn events(server: &str) -> Result<()> {
    let ep = rmx_ipc::endpoint(server)?;
    let mut conn = rmx_ipc::connect(&ep)
        .await
        .context("no rmx server is running")?;
    conn.tx.send(ClientMsg::request(Request::Subscribe)).await?;
    while let Some(msg) = conn.rx.recv().await {
        if let ServerMsg::Event(line) = msg {
            println!("{line}");
        }
    }
    Ok(())
}

fn print_list(body: &str) -> Result<()> {
    let v: serde_json::Value = serde_json::from_str(body)?;
    let workspaces = v["workspaces"].as_array().cloned().unwrap_or_default();
    if workspaces.is_empty() {
        println!("no workspaces");
    }
    for w in workspaces {
        let tabs = w["tabs"].as_array().cloned().unwrap_or_default();
        let attached = w["attached"].as_u64().unwrap_or(0);
        println!(
            "{}: {} tab{}{}",
            w["name"].as_str().unwrap_or("?"),
            tabs.len(),
            if tabs.len() == 1 { "" } else { "s" },
            if attached > 0 {
                format!(" (attached: {attached})")
            } else {
                String::new()
            }
        );
        for t in tabs {
            let panes = t["panes"].as_array().map_or(0, Vec::len);
            let mark = if t["active"].as_bool() == Some(true) {
                "*"
            } else {
                " "
            };
            println!(
                "  {}{mark} {}  [{} pane{}]",
                t["index"],
                t["name"].as_str().unwrap_or("?"),
                panes,
                if panes == 1 { "" } else { "s" }
            );
        }
    }
    Ok(())
}

fn load_config(path: &std::path::Path) -> Result<Config> {
    let (config, warnings) = Config::load(path).map_err(|e| anyhow::anyhow!("\n{e}"))?;
    for w in warnings {
        eprintln!("{w}");
    }
    Ok(config)
}

fn print_keys(path: &std::path::Path, only: Option<&str>) -> Result<()> {
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

fn config_cmd(
    runtime: &tokio::runtime::Runtime,
    server: &str,
    path: &std::path::Path,
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

fn doctor(runtime: &tokio::runtime::Runtime, server: &str, config: &std::path::Path) {
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

fn log_dir() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("rmx")
}

/// Logs go to a file: the terminal belongs to the panes, and the server has
/// no terminal at all. Level via `RMX_LOG`.
fn init_logging(name: &str) -> Result<PathBuf> {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_join_keeps_argument_boundaries() {
        let args = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(shell_join(&args(&["make && run"])), "make && run");
        if cfg!(windows) {
            assert_eq!(
                shell_join(&args(&["vim", "my file.txt"])),
                "vim \"my file.txt\""
            );
        } else {
            assert_eq!(
                shell_join(&args(&["sh", "-c", "echo \"a b\""])),
                "sh -c 'echo \"a b\"'"
            );
            assert_eq!(shell_join(&args(&["echo", "it's"])), "echo 'it'\\''s'");
        }
    }

    #[test]
    fn server_args_survive_leading_dashes() {
        let cli = Cli::try_parse_from(
            std::iter::once("rmx".to_owned())
                .chain(server_args("-odd", std::path::Path::new("/tmp/-x.toml")).unwrap()),
        )
        .unwrap();
        assert_eq!(cli.server, "-odd");
        assert_eq!(
            cli.config.unwrap(),
            std::path::PathBuf::from("/tmp/-x.toml")
        );
        assert!(matches!(cli.command, Some(Command::Server { .. })));
    }
}
