# rust-multiplexer (`rmx`)

A terminal multiplexer, like tmux, that runs natively on **Windows, Linux and macOS**. It has the menus, command palette and keys of Windows Terminal, and every keybinding can be changed.

- **Workspaces, tabs and split panes**, kept in a background server. Close the terminal and your shells keep running; `rmx` brings them back.
- **Session manager** (plain `rmx`, or Leader `s` inside rmx):
  - a full-screen tree of workspaces, tabs and panes with a live preview, each showing its folder and whether it's attached elsewhere
  - a top row, **+ New workspace in <this folder>**, for starting a session where you ran `rmx`
  - **Enter** or **1–9** opens a workspace
  - from the same screen you can create (`n`, `t`), rename (`r`), split (`|`, `-`) and close (`x`) anything
- **Easy to discover:**
  - an F10 menu bar (File · Edit · View · Tab · Pane · Workspace · Help)
  - a right-click menu on panes
  - a fuzzy command palette that shows each command's key
  - a hint popup after the leader key
  - an F1 list of every binding
- **Copy and paste that just work:**
  - drag to select, and the text goes to the system clipboard (OSC 52 when you're on SSH)
  - the mouse wheel scrolls back through history
  - a keyboard select mode with search
- **Custom keybindings:**
  - presets (`windows`, `tmux`, `vim`, `none`), key sequences and your own modes
  - settings apply as soon as you save the file
  - a mistake gets an error with line, column and a suggested fix
- **Built-in session restore** after a reboot, with no plugins.
- **Scriptable:** `rmx list --json`, `rmx send`, `rmx capture`, and `rmx events` (a stream of JSON lines).
- **Small:** one binary of about 4 MB.

## Install

```sh
cargo install --path .          # from a checkout
```

Release archives for Windows (x64/ARM64), macOS (Apple silicon/Intel) and Linux (glibc and static musl) are built by `.github/workflows/release.yml` when you push a `v*` tag. Scoop and Homebrew templates are in `packaging/`.

## Use

```sh
rmx                    # session manager: see all sessions, open one, or start one in this folder
rmx -n [name]          # new session in this folder, in the background (named after the folder)
rmx attach             # skip the manager: straight into the last session
rmx new -n api         # new workspace "api"
rmx manage             # session manager
rmx list               # workspaces, tabs, panes   (--json for scripts)
rmx attach api         # attach to a named workspace
rmx send -t api -e "cargo test"     # type into a pane (target: ws[:tab[.pane]] or %id)
rmx save / rmx restore # keep workspaces across reboots
rmx doctor             # check setup; prints the Windows Terminal key snippet
rmx stop               # stop the server and close everything
```

tmux command names work as aliases: `new-session`, `attach-session`/`a`, `ls`, `kill-session`, `send-keys`, `capture-pane`, `list-keys`, `kill-server`.

### Default keys (`windows` preset)

The **leader key** is `Ctrl+Space`: press it, then a letter. A popup shows what each next key does.

| | Leader key | Direct key |
|---|---|---|
| Menu bar / help | | `F10` / `F1` |
| Command palette | `p` | `Ctrl+Shift+P`* |
| Session manager | `s` | |
| New tab / close tab | `t` / `x` | `Ctrl+Shift+T`* |
| Next / previous tab | `n` / `b` | `Alt+]` / `Alt+[`, `Ctrl+Tab`* |
| Go to tab N | `1`–`9` | `Alt+1`–`9` |
| Split right / down | `\|` / `-` | `Alt+Shift+Plus`* / `Alt+Shift+Minus`* |
| Move focus | arrows | `Alt+Shift+H/J/K/L`, `Alt+Arrows`* |
| Resize | `r`, then arrows, `Esc` when done | `Alt+Shift+Arrows`* |
| Zoom pane / close pane | `z` / `w` | `Ctrl+Shift+W`* |
| Select / scroll mode | `[` (then `v` to select, `Enter` to copy) | mouse wheel, drag |
| Paste / find | `]` / `/` | `Ctrl+Shift+V`* / `Ctrl+Shift+F`* |
| Next layout | `Space` | |
| Leave (detach) | `d` | |
| Lock keys to the pane | `l` (press the leader to unlock) | |

\* These keys work when the terminal passes them through: always on Windows, and in terminals with the kitty keyboard protocol. Windows Terminal handles some of them itself; `rmx doctor` shows how to free them. Everything in the leader column works in every terminal.

To use tmux's keys instead (`Ctrl+B`, `%`, `"`, `c`, …), put `preset = "tmux"` in your config.

## Configure

`rmx config init` writes a commented starter file; `rmx config path` shows where it is. See [docs/CONFIG.md](docs/CONFIG.md) for every option and action.

```toml
preset = "windows"

[leader]
key = "ctrl+a"

[keys.leader]
"g" = { action = "run", command = "lazygit", in = "new-tab" }
"-" = { action = "split-pane", direction = "down", size = 30 }
"x" = "none"                    # remove a preset key
```

## How it works

```
rmx (client) ──local socket / named pipe──▶ rmx server (background, one per user)
  keys, mouse, paste ─────────────────────▶   workspaces ▸ tabs ▸ layout tree ▸ panes (PTY + vt100)
  ◀─────────────────── changed cells only ─   draws menus, panes, status bar for each client
```

- **Server:** owns every shell. It draws each client's screen, including all menus and overlays, and sends only the cells that changed. That works the same on Windows, where tmux's way of handing the terminal itself to the server isn't possible.
- **Client:** sends key and mouse events, not raw bytes, so bindings resolve the same on every OS.
- **Socket security:**
  - Unix: the socket folder must be mode 0700 and owned by you, the socket is 0600, and the server checks each connecting user's ID.
  - Windows: the named pipe allows only you and SYSTEM, and refuses connections from other machines.

| Crate | Role |
|---|---|
| `rmx-core` | Pure logic: terminal emulation, key encoding, the layout tree |
| `rmx-config` | Keys, actions, presets, config parsing and validation, the key engine |
| `rmx-proto` | Client/server messages and how they're encoded |
| `rmx-ipc` | Sockets and pipes, starting the detached server, security |
| `rmx-server` | Workspaces, panes, drawing the screen, menus, session manager, select mode, persistence, control API |
| `rmx-client` | Thin terminal front end and clipboard |
| `rust-multiplexer` (root) | The `rmx` command |

The design and roadmap are in [docs/PLAN.md](docs/PLAN.md).

## Development

```sh
cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
```

CI runs this on Ubuntu, Windows and macOS, including tests that drive real shells through the whole server. Logs go to `<local data dir>/rmx/` (`client.log`, `server-<name>.log`); set the level with `RMX_LOG=debug`.
