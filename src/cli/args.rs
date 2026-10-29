//! The clap definitions: global flags, subcommands and their help text.

use std::path::PathBuf;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "rmx",
    version,
    about = "A cross-platform terminal multiplexer with menus and custom keys"
)]
pub(crate) struct Cli {
    /// Use a separate server with this name (like tmux -L).
    /// Inside an rmx pane this defaults to that pane's server.
    #[arg(
        short = 'L',
        long = "server",
        global = true,
        env = "RMX_SERVER",
        default_value = "default"
    )]
    pub(crate) server: String,

    /// Config file (default: the platform config dir, or $RMX_CONFIG).
    #[arg(long, global = true)]
    pub(crate) config: Option<PathBuf>,

    /// Create a new workspace in the background, in the current folder
    /// (named after it unless NAME is given), and return to the shell.
    #[arg(short = 'n', long = "new", value_name = "NAME", num_args = 0..=1, default_missing_value = "")]
    pub(crate) new: Option<String>,

    #[command(subcommand)]
    pub(crate) command: Option<Command>,
}

#[derive(Subcommand)]
pub(crate) enum Command {
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
pub(crate) enum ConfigCmd {
    /// Print the config file path.
    Path,
    /// Validate the config; exits non-zero on errors (for CI).
    Check,
    /// Write a commented starter config if none exists.
    Init,
    /// Apply the config to the running server now.
    Reload,
}
