# rust-multiplexer (`rmx`) — a better tmux, native on Windows, Linux and macOS

Project: **rust-multiplexer** (command `rmx`) · Status: **Draft for review** · Date: 2026-10-24 · Owner: Energma

## 1. Problem

tmux is the standard terminal multiplexer, but:

- **No native Windows.** It only runs under WSL, Cygwin or MSYS2. Windows users have no first-class multiplexer for PowerShell or cmd.
- **Hard to learn.** Commands sit behind a hidden prefix key, the config syntax is cryptic, and there is no menu or command list on screen. You need a cheat sheet.
- **Rough everyday tasks.** Copy and paste needs clipboard plugins. Scrolling back means entering copy mode. Sessions don't survive a reboot without plugins. Reloading the config doesn't undo settings you removed.
- **No real API.** Scripts and plugins scrape `#{format}` strings with shell scripts (TPM).

The alternatives don't solve this:

- **Zellij** (Rust) is discoverable but has no Windows build, is heavy (about 50 MB) and its default keys clash with vim and Helix.
- **WezTerm**'s multiplexer only works with the WezTerm GUI.
- **psmux** is already "tmux for Windows in Rust" and copies tmux's `.tmux.conf` model.

**rmx's position:** the multiplexer that behaves the same on all three OSes and is easy to pick up for someone who knows Windows Terminal or VS Code. It has menus, a command palette, copy and paste that work out of the box, keybindings you can fully change, and built-in session restore. It does not try to be "tmux on Windows".

## 2. Goals and non-goals

### Goals (v1)
1. One `rmx` binary per OS. A session keeps running after you close the terminal (detach), and you can attach to it again.
2. Tabs and split panes, resize (keyboard and mouse), zoom, scrollback, select or copy mode with search, and the system clipboard.
3. **Familiar to Windows users:** Windows Terminal and VS Code vocabulary (Workspace, Tab, Pane), a menu bar (F10), right-click menu, command palette, and a hint bar on screen.
4. **Every keybinding can be changed** through `config.toml`: presets (`windows`, `tmux`, `vim`), chords, modes, unbinding, actions that take arguments, validation with line and column, and hot reload.
5. A scriptable CLI with `--json` output. tmux command names work as aliases.
6. Built-in save and restore of workspaces (layout, working folder, command), no plugins.

### Non-goals (v1)
- Not compatible with tmux's config language. A `.tmux.conf` importer may come later.
- No plugin runtime (WASM or Lua). Hooks and the JSON API are the extension points.
- No image protocols (sixel, kitty graphics). v2.
- No GUI or terminal-emulator front end. rmx runs *inside* an existing terminal.
- No remote attach over the network. Using it over SSH works the same as with tmux.
- No Windows older than Windows 10 1809, which is where ConPTY first shipped.

### Done (v1) checklist
- [ ] On Windows 11 (in Windows Terminal), Ubuntu 24.04 and macOS 14: `rmx` opens a workspace, and inside it vim/nvim, htop/btm, git log (pager) and pwsh run correctly.
- [ ] Close the terminal window, open a new one, run `rmx attach`: same tabs, panes and screen contents.
- [ ] A new user can split, switch tabs, copy text and detach **without reading docs**: the hint bar, F10 menu and palette show them how.
- [ ] Change any binding in `config.toml`, save, and it applies right away. A typo shows `config.toml:12:5 unknown action "split-pain" — did you mean "split-pane"?`.
- [ ] `rmx list --json` returns workspaces, tabs and panes. `rmx send <target> "ls\r"` works.
- [ ] After a reboot, `rmx restore` brings back the layout and working folders.
- [ ] CI is green on all three OSes (fmt, clippy `-D warnings`, tests, PTY smoke test).

## 3. Vocabulary

| tmux | rmx (user-facing) | Code type |
|---|---|---|
| server | Background service | `Server` |
| session | **Workspace** | `Workspace` |
| window | **Tab** | `Tab` |
| pane | **Pane** | `Pane` |
| prefix | **Leader key** | `KeyMode::Leader` |
| copy-mode | **Select mode** (scrolling uses the same mode) | `KeyMode::Select` |
| command-prompt | **Command palette** | `Overlay::Palette` |
| choose-tree | **Workspace switcher** | `Overlay::Switcher` |
| detach | **Detach**; menu label "Leave (keep running)" | `Action::Detach` |
| target `s:w.p` | `workspace:tab.pane` (same syntax, plus `%id`) | `Target` |

## 4. Architecture

### 4.1 Process model (same idea as tmux, adapted for Windows)

```
 ┌────────────── terminal (Windows Terminal / iTerm2 / GNOME …) ──────────────┐
 │  rmx client  (thin)                                                         │
 │   crossterm input ──► KeyEvent/Mouse/Paste/Resize ──┐                         │
 │   ratatui draw   ◄── Frame diff (cells) ◄───────────┼──┐                      │
 └─────────────────────────────────────────────────────┼──┼──────────────────────┘
                                                       │  │  local socket / named pipe
                                  postcard frames, u32-length-prefixed
 ┌────────────── rmx server (detached, one per user) ─▼──┴──────────────────────┐
 │  Core (single owner of state, tokio select! loop)                              │
 │    Workspaces ─► Tabs ─► LayoutTree ─► Panes                                   │
 │    KeyEngine (per client: mode, pending chord)  ActionRegistry  Config(hot)    │
 │    Compositor: per-client ratatui Buffer → diff vs last sent                   │
 │  Pane = PTY (portable-pty) + reader thread + waiter thread + Emulator(vt100)   │
 └────────────────────────────────────────────────────────────────────────────────┘
```

Decisions and why:

- **The server owns all state; the client is thin.** This is what makes detach and multiple clients work. tmux passes the terminal's file descriptors to the server, which is impossible on Windows. So the server **composes the screen itself** and streams **cell diffs** to each client, which redraws through ratatui. That also handles clients of different sizes correctly.
- **The client sends logical events, not raw bytes.** It sends `KeyEvent`, mouse and paste events. Keybindings are resolved on the server, so behaviour is the same on every OS, and the server encodes keys into the pane's VT bytes.
- **The server core runs in one task and owns all state.** Other parts talk to it over channels, with no shared locks. portable-pty's I/O blocks, so each pane gets a dedicated reader thread and child-waiter thread that feed the core over a tokio mpsc channel.
- **Ship the server first, but in the same process.** Phase 1 runs the server core on a thread inside the `rmx` process with an in-memory transport. Phase 5 replaces that with a real socket and a detached process. This is nearly as fast as a single-process prototype, and it avoids rewriting the code when we split it.

### 4.2 Crate layout (cargo workspace, one shipped binary)

```
rust-multiplexer/
  src/             bin `rmx`: clap CLI (src/cli/), self-update (src/update/), logging.
  crates/
    rmx-core/      Pure logic: LayoutTree, Emulator trait + vt100 impl, key→VT encoder. No I/O.
    rmx-config/    Action enum + registry, KeyChord parser, KeyMap/KeyEngine, presets,
                    Config (serde/toml) with located errors.
    rmx-proto/     ClientMsg / ServerMsg, codec (postcard + length prefix), version handshake.
    rmx-ipc/       Unix sockets / Windows named pipes, detached spawn, socket security.
    rmx-server/    Core loop, PTY panes, compositor, menus/palette/manager, select mode,
                    persistence, config watcher, control API.
    rmx-client/    Terminal setup/teardown, input capture, frame apply + ratatui draw, clipboard.
```

Inside each crate, modules are folders split by responsibility, with no file over 200 lines. See [DEVELOPMENT.md](DEVELOPMENT.md) for the module map and the code rules.

### 4.3 Stack

| Concern | Choice | Why / notes |
|---|---|---|
| PTY | `portable-pty` 0.9 | WezTerm's PTY layer and the only mature cross-platform one (ConPTY plus Unix). `take_writer()` works only once; `MasterPty` is not `Sync`, so share it through a `Mutex`. |
| Emulation | `vt100` 0.18 behind our own `Emulator` trait | Clean read API, truecolor, alt screen, mouse modes, bracketed paste, wide chars, OSC title and OSC 52 callbacks. **Gaps:** no reflow, no DSR/DA replies, no strikethrough, no OSC 8, no mode 2026. The upgrade path is `alacritty_terminal`, which the trait keeps cheap to switch to. |
| TUI | `ratatui` 0.30 + `crossterm` 0.29 | They share one crossterm version (`ratatui::crossterm` re-export). We write our own vt100→Buffer widget (about 60 lines), because `tui-term` pins vt100 0.16. |
| Async | `tokio` (multi-thread on the server, current-thread on the client) | One codec, timers, signals. |
| IPC | `interprocess` 2.4 (`tokio` feature) behind a `Transport` trait | Unix domain sockets and Windows named pipes. Before committing, check whether it supports `FIRST_PIPE_INSTANCE` and `REJECT_REMOTE_CLIENTS`. If not, use `tokio::net::windows::named_pipe` directly. |
| Wire format | `postcard` with serde, u32 LE length prefix, 16 MiB cap | Compact and stable. A JSON codec behind `--debug-proto`. |
| CLI | `clap` 4 (derive) with `visible_alias` for tmux names | |
| Config | `toml` + `serde` with spans; `miette` for error reports; `strsim` for "did you mean" | |
| Hot reload | `notify` watching the config **folder**, 200 ms debounce | Editors save by renaming the file, so watching the file itself misses saves. |
| Fuzzy search | `nucleo-matcher` | Command palette and workspace switcher. |
| Clipboard | `arboard` (native) with OSC 52 as fallback (SSH or headless) | |
| Errors / logging | `anyhow` (bins), `thiserror` (libs), `tracing` written to a log file | The server has no visible terminal, so it logs to a file. |

## 5. Key subsystems

### 5.1 Pane lifecycle (where the Windows risks are)
1. Check the OS. On Windows, require build ≥ 17763 (1809) **before** calling portable-pty, because it panics on older builds.
2. `openpty(size)` → `spawn_command`, then drop the slave. Pick the shell: on Windows, look for `pwsh.exe`, then `powershell.exe`, then `%ComSpec%`. On Unix, use `$SHELL`, then the shell in passwd. The config can override it.
3. **Answer DSR/DA queries ourselves.** ConPTY's `INHERIT_CURSOR` sends `ESC[6n` at startup and blocks until it gets a reply. We catch `CSI 6n` and `CSI c` through vt100's `unhandled_csi` callback and write `ESC[{r};{c}R` and a DA reply back to the PTY. **Without this the shell hangs on Windows.**
4. The reader thread loops `read` → `core_tx.send(PaneOutput)`. EOF (or EIO on Unix) means the pane has exited. A waiter thread calls `child.wait()` to get the exit code.
5. Close in this order: kill the child, keep draining output until EOF, drop the writer, then drop the master. Getting this wrong deadlocks `ClosePseudoConsole`.
6. Ship a bundled `conpty.dll` and `OpenConsole.exe` next to the Windows build, because the built-in ConPTY on older Windows 10 is buggy. portable-pty loads the bundled one automatically.

### 5.2 Layout tree (borrowed from tmux `layout.c`)
- `enum Node { Split { dir: H|V, children: Vec<(Node, weight)> }, Leaf(PaneId) }`.
- **Split** inserts a sibling if the direction matches, otherwise it wraps the leaf in a new split. **Resize** moves a boundary and takes space from neighbours. **Close** gives the space to the neighbours and collapses splits left with one child.
- **Zoom** is a per-tab flag: the zoomed pane gets the whole area, and the tree stays unchanged.
- **Presets** rebuild the tree: `even-h`, `even-v`, `main-left`, `main-top`, `tiled`.
- **Focus by direction** picks the nearest pane whose edge is next to the current pane's edge in that direction.
- The tree is serialisable with serde for persistence. It is pure logic, so it gets heavy unit and property tests.

### 5.3 Input pipeline and key engine
```
crossterm Event ─► (client) filter Press only ─► ClientMsg::Input
   ─► (server) KeyEngine.feed(client_mode, key)
        ├─ matches binding in current mode/chord → Action → Core.dispatch
        ├─ partial chord → wait (timeout_ms) → show which-key hints
        └─ no match in Normal mode → encoder.encode(key, pane.app_cursor) → PTY
```
- **Modes** are separate key tables, like tmux's: `normal`, `leader`, `select`, `resize`, `locked`. In `locked` mode every key goes to the pane except the unlock key. That is the escape hatch for vim and for nested rmx, and it fixes the key clashes Zellij is criticised for.
- **The encoder is our own code.** It handles Ctrl+letter (`&0x1f`), Alt as an Esc prefix, CSI `1;mod` modifiers, app-cursor mode (`ESC O A`), F-keys, and bracketed paste only when the pane has asked for it.
- **Platform facts the design has to respect:**
  - Crossterm's kitty keyboard protocol is **Unix only**. On Windows, the console API already reports modifiers fully.
  - Bracketed paste is not supported on Windows input, so a paste arrives as a fast run of key events. We detect that and still wrap it.
  - Toward the child process, Ctrl+Shift+X is always the same as Ctrl+X.
  - Avoid Ctrl+Alt+key bindings by default: on German, Polish and similar keyboards Ctrl+Alt is AltGr, which types `@ { \`.

### 5.4 Actions (one registry used by keys, menus, palette, CLI and the API)
- Each action has: `id` (kebab-case), label, category (File/Edit/View/Tab/Pane/Help), typed arguments, `enabled(ctx)` and a doc string.
- The menu bar, right-click menu, palette, which-key popup, `rmx keys`, the CLI and the JSON API all **read from this one registry**. That keeps them consistent and means each new action is one entry.
- Initial set:
  - **Tabs:** `new-tab`, `close-tab`, `rename-tab`, `next-tab`, `prev-tab`, `go-to-tab{index}`, `move-tab{delta}`.
  - **Panes:** `split-pane{direction,size,command}`, `close-pane`, `move-focus{direction}`, `resize-pane{direction,amount}`, `toggle-pane-zoom`, `swap-pane`, `apply-layout{name}`, `toggle-sync-input`.
  - **Select mode and clipboard:** `select-mode`, `find`, `copy`, `paste`, `scroll{lines|page}`.
  - **UI:** `command-palette`, `open-menu`, `show-help`, `workspace-switcher`.
  - **Workspace and config:** `rename-workspace`, `detach`, `reload-config`, `open-config`, `open-editor`.
  - **Mode switching:** `enter-mode{mode,sticky}`, `exit-mode`.
  - **Commands:** `run{command,in}`.

### 5.5 Default keybindings (`windows` preset)

Three layers, chosen so that bindings keep working **inside Windows Terminal** (which takes its own Ctrl+Shift keys before rmx sees them) and in older terminals:

| Action | Leader layer (always works) | Direct keys | Ctrl+Shift layer (if the terminal passes it through) |
|---|---|---|---|
| Leader key | **Ctrl+Space** (tmux preset: Ctrl+B) | — | — |
| Menu bar / Help | — | **F10** / **F1** | — |
| Command palette | Leader P | — | Ctrl+Shift+P |
| New tab | Leader T | — | Ctrl+Shift+T |
| Next / prev tab | Leader N / Leader B | Alt+] / Alt+[ | Ctrl+Tab / Ctrl+Shift+Tab |
| Go to tab N | Leader 1–9 | Alt+1–9 | — |
| Split right / down | Leader \| / Leader - | — | Alt+Shift+Plus / Alt+Shift+Minus |
| Move focus | Leader Arrows | Alt+Shift+H/J/K/L | Alt+Arrow |
| Resize | Leader R → arrows (resize mode stays on until Esc) | — | Alt+Shift+Arrow |
| Zoom pane | Leader Z | — | — |
| Close pane | Leader W | — | Ctrl+Shift+W |
| Select/scroll mode | Leader [ , mouse wheel | — | Ctrl+Shift+M |
| Find | Leader / | — | Ctrl+Shift+F |
| Copy / paste | Enter or Ctrl+C **in Select mode**; mouse drag copies | — | Ctrl+Shift+C / Ctrl+Shift+V |
| Detach | Leader D | — | — |
| Lock (all keys go to the pane) | Leader L | — | — |

- **When the Ctrl+Shift layer is on:** `direct_keys = "auto"` turns it on when the terminal can report these keys: on Windows always, on Unix when the kitty keyboard protocol is available.
- **Making it work inside Windows Terminal:** `rmx doctor` prints a Windows Terminal `settings.json` snippet that unbinds those keys there, so they reach rmx.
- **Ctrl+C is never taken** outside Select mode.
- **Other presets:** `tmux` (Ctrl+B and tmux letters such as `%`, `"`, `c` and `x`), `vim` (hjkl everywhere), and `none` (start from empty).

### 5.6 Config (`config.toml`)
- **Location:**
  - Windows: `%APPDATA%\rmx\config.toml`.
  - Linux: `$XDG_CONFIG_HOME/rmx/config.toml`.
  - macOS: `~/.config/rmx` first, then `~/Library/Application Support/rmx`.
  - `RMX_CONFIG` or `--config` overrides all of these.
- **Effective config** = preset defaults + the user file, merged fresh on every load. Removing a line therefore reverts the setting, unlike tmux.
- **Validation:**
  - Every error shows file, line and column, with a "did you mean" suggestion for unknown actions.
  - It also flags duplicate keys after normalising (`Shift+Ctrl+T` is the same as `ctrl+shift+t`).
  - It flags a key that is both a binding and the start of a chord.
  - It warns about keys the terminal can't deliver, such as `ctrl+shift+a` when the terminal can't tell it from `ctrl+a`.
  - It warns about keys Windows Terminal is known to take.
- **Hot reload:** parse and validate first, then swap atomically. On error, keep the old config and show a message in the status bar.

```toml
preset = "windows"           # windows | tmux | vim | none
direct_keys = "auto"         # auto | on | off

[general]
shell = "auto"               # or "pwsh", "/bin/zsh", ...
scrollback = 10000
mouse = true
copy_on_select = true

[leader]
key = "ctrl+space"
timeout_ms = 2000
show_hints = true

[theme]
status_bg = "#1e1e2e"
active_border = "#89b4fa"

[keys.normal]
"f10"          = "open-menu"
"alt+1"        = { action = "go-to-tab", index = 1 }
"ctrl+shift+t" = "none"                       # unbind a preset key
"ctrl+space z" = "toggle-pane-zoom"           # chord form

[keys.leader]
"|" = { action = "split-pane", direction = "right" }
"-" = { action = "split-pane", direction = "down", size = "30%" }
"r" = { action = "enter-mode", mode = "resize", sticky = true }
"g" = { action = "run", command = "lazygit", in = "new-tab" }

[keys.resize]
"left" = { action = "resize-pane", direction = "left", amount = 2 }
"esc"  = "exit-mode"

[[commands]]                                  # extra palette entries
name = "Open htop"
action = { action = "run", command = "htop", in = "split-right" }
```

### 5.7 UI surfaces
- **Status bar:**
  - Left: workspace name and tabs (clickable).
  - Right: mode indicator, hints such as `^Space Leader · F10 Menu · F1 Help`, and the clock.
  - In Select mode the hints change to show that mode's keys.
- **Which-key popup:** appears after the leader key and lists the next keys available in that mode.
- **Command palette:** fuzzy search over every action and `[[commands]]` entry, showing each one's binding. This is also how users discover keys.
- **Right-click context menu** on a pane: Split, Close, Zoom, Copy, Paste, Rename. Shift-click bypasses rmx so the outer terminal handles it.
- **Menu bar**, opened with F10 or a click: File (New tab, New workspace, Detach, Quit), Edit (Copy, Paste, Find, Select mode), View (Zoom, Layouts, Sync input), Tab, Pane, Help (Keys, Open config, About). Arrow keys and accelerator letters work.
- **Mouse:** click to focus, drag borders to resize, wheel scrolls back (no special mode needed), drag to select and copy.

### 5.8 Wire protocol (rmx-proto)
- **Client → server:**
  - `Hello{proto_ver, term, size, env, cwd}`
  - `Input(Key|Mouse|Paste|Focus)`
  - `Resize{cols,rows}`
  - `Command(Vec<String>)`
  - `Detach`
- **Server → client:**
  - `Welcome{client_id}`
  - `Frame{full, cells:[(x,y,Cell)], cursor, cursor_style, title}`
  - `Bell`
  - `SetClipboard(String)`, which the client writes to the native clipboard or sends as OSC 52.
  - `Toast(String)`
  - `Exit{reason}`
- **Frame rate:** the compositor redraws only when something changed, capped at about 60 fps per client. A slow client gets the latest frame, not a backlog (the queue has a fixed size).
- **Version check:** a version mismatch is refused with a clear message, for example "server is v0.3, client v0.4 — run `rmx stop`".

### 5.9 Server discovery, spawn and security
- **Socket or pipe location:**
  - Unix: `$XDG_RUNTIME_DIR/rmx/default.sock`, falling back to `$TMPDIR/rmx-$UID/` or `/tmp/rmx-$UID/`.
  - Windows: `\\.\pipe\rmx-<user-SID>-default`.
  - `-L name` selects a different server, as in tmux.
- **Starting the server:** the client re-runs its own binary as `rmx __server`, waits up to 2 s for the socket, and uses a lock file so two clients don't start two servers.
  - Unix: `setsid` via `pre_exec`, all standard streams to null, ignore SIGHUP and SIGPIPE.
  - Windows: `DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP | CREATE_BREAKAWAY_FROM_JOB`. The breakaway flag stops the server being killed when the terminal or VS Code closes. If the host refuses it, retry without it and warn that detach won't survive closing that terminal. Helper processes get `CREATE_NO_WINDOW`.
- **Security:**
  - Unix: the folder must be 0700 and owned by the user, and must not be a symlink. The socket is 0600, and a connecting peer's uid must match ours. Never use the Linux abstract namespace, which has no permissions.
  - Windows: the pipe has an explicit DACL allowing only the current user and SYSTEM. It is created with `FILE_FLAG_FIRST_PIPE_INSTANCE`, so another process can't create it first and pose as the server, and with `PIPE_REJECT_REMOTE_CLIENTS`. By default a named pipe is readable by Everyone, so this is required.

### 5.10 Persistence and API (v1)
- **Saving:**
  - The server saves workspaces as `state.json` every 30 s, and again on every layout change: layout tree, tab and pane names, working folder, and the foreground command if it is on an allowlist (vim, htop, ssh, and so on).
  - Working folder comes from OSC 7 when available, otherwise from `/proc` or `proc_pidinfo`. On Windows, the shell integration snippet emits OSC 7.
- **Restoring:** `rmx restore`, or automatically on first start if `restore_on_start = true`. Restoring scrollback is opt-in.
- **JSON API:** every CLI subcommand supports `--json`. `rmx events --json` streams changes (pane-created, tab-renamed, exited). It is the same protocol, with a "control" client role. This replaces tmux's control mode and TPM-style shell scripting.

## 6. Phases and checks

**Progress (2026-10-24): phases 0–8 implemented and revalidated.** 133 tests pass on Linux (stable across repeated runs). Clippy `-D warnings` is clean for Linux, Windows (msvc) and macOS (aarch64). Release binary: 4.3 MB.

| Phase | Status | Verified how |
|---|---|---|
| 0 Foundation | Done | CI workflow for 3 OSes, fmt/clippy clean |
| 1 One pane end to end | Done | Real-PTY tests; by hand: bash, vim, htop, resize |
| 2 Tabs, splits, status bar | Done | Layout unit tests; split and exit tested through the core; by hand in a real terminal |
| 3 Keys + config | Done | 56 config tests; hot reload with a bad config shows `file:line:col` + did-you-mean and keeps the old settings |
| 4 Menus, palette, hints, session manager | Done | Session-manager test through the core; by hand: F10 menu, palette, manager |
| 5 Client/server + detach | Done | 19 IPC tests; by hand: detach, `ls`, reattach by name, real detached server |
| 6 Select mode, scrollback, clipboard | Done | Select unit tests; by hand: select, copy to system clipboard, paste back |
| 7 Session restore, JSON API, layouts, sync input, locked mode, doctor | Done | Control API test; by hand: save → stop → restore brought back 3 workspaces with their layouts |
| 8 Release + packaging | Partial | Release workflow, Scoop and Homebrew templates; no release has been cut yet |

**Not yet verified or not done:**
- **Nothing has run on Windows or macOS yet.** The code compiles and lints clean for both; the first CI run on those runners is the real test (ConPTY, named-pipe security, detaching from the terminal).
- **Revalidation:** three review subagents (server core; UI and input; IPC, CLI and config) found 29 confirmed bugs, all fixed.
  - The serious ones were server-wide crashes on tiny terminals and tiny panes. They are now covered by a regression test, a 40×10 minimum layout size, and a guard that drops a single frame instead of losing every session.
  - The Windows-only ones were Caps Lock + Ctrl, AltGr characters, and Alt+numpad codes; those fixes are not verified on Windows yet.
  - The rest: dialogs acting on the wrong tab, how `rmx new -d -- …` passes arguments, `-L` names starting with a dash, `rmx` inside a pane reaching the wrong server, refused attaches exiting 0, missing version checks on CLI requests, the server holding onto the client's folder, log truncation, very large pastes dropping the session, drift in select positions, and safety of restoring the session file.
- **Mouse:** now tried by hand by sending mouse sequences to the real client: click focus, border drag, drag-to-select with copy, middle-click paste, wheel scroll, right-click menu and clicking its items. Forwarding mouse events to programs (vim, htop) is still covered only by encoding tests.
- **Deferred to v1.x:** floating panes and popups; OSC 8 hyperlinks and image protocols (vt100 limits); reflow on resize (would need `alacritty_terminal`); bundling `conpty.dll`; winget manifest; Shift+Home/End in select mode; restoring programs other than an explicit pane command.

Every phase leaves `main` green on CI for all three OSes and ends with a demo on each OS.

| # | Phase | Scope | Done when |
|---|---|---|---|
| 0 | **Foundation** | Workspace crates, CI matrix (windows-latest, ubuntu-latest, macos-latest), rustfmt, clippy `-D warnings`, `tracing` log file, `git init` | CI is green on an empty skeleton on all three OSes |
| 1 | **One pane end to end** (server in the same process) | PTY spawn, reader and waiter threads, vt100 emulator with DSR/DA replies, vt100→ratatui widget, key encoder, resize, in-memory transport, proto types | `rmx` opens pwsh (Windows) or zsh/bash; vim, htop and `git log` render; resize works; exiting the shell exits rmx. Golden tests: emulator bytes → Buffer |
| 2 | **Tabs, splits, status bar** | LayoutTree, split, close, focus by direction, resize, zoom, tabs, status bar with tab list, borders | Make 4 panes across 2 tabs, resize and zoom them, and everything redraws correctly. LayoutTree unit and property tests |
| 3 | **Actions + keybinding engine + config** | Action registry, key parser, modes, chords, leader, presets, TOML with spans and errors, validation, hot reload, `rmx keys`, `rmx config check` | Edit the config while rmx runs and the change applies; a bad config shows file:line:col with a suggestion; the `tmux` preset works |
| 4 | **Easy to discover** | Hint bar, which-key popup, command palette, right-click menu, F10 menu bar, mouse focus and border resize | A new user splits a pane, switches tabs and finds Detach with no docs. Snapshot tests with ratatui `TestBackend` |
| 5 | **Real client/server** | `Transport` over `interprocess`, detached server spawn on each OS, socket and pipe security, version handshake, frame diffs, multiple clients (smallest-size policy), CLI `new/attach/list/detach/close/rename/stop` | Close the terminal, reattach from a new one, everything is still there. Two clients at once work. Another user can't connect (checked by a test on Unix and a manual check on Windows) |
| 6 | **Select mode, scrollback, clipboard** | Wheel scrollback, Select mode (keyboard and mouse), search with highlight and match count, `arboard` plus OSC 52, paste (wrapped only when the pane asks; Windows paste detection) | Copy from a pane, paste into Notepad or TextEdit; search finds a match 5,000 lines back |
| — | **MVP = phases 0–6** | | |
| 7 | **Better than tmux** | Save and restore, `--json` and `rmx events`, layout presets, sync input, `rmx send`/`capture`, `locked` mode, `rmx doctor` (prints the Windows Terminal unbind snippet, checks ConPTY) | Restore after a reboot works; a script drives rmx using only `--json` |
| 8 | **Polish + release** | Floating panes and popups, OSC 8 links, synchronized output, theming, packaging (cargo, winget, scoop, Homebrew, AUR), bundled `conpty.dll` | v1.0 tagged and installable through each package manager |

**Order and why:** phases 1 and 5 carry the most risk (ConPTY behaviour, detaching on Windows). Phase 1 runs straight away. As soon as phase 1 runs, a spike tests a detached ConPTY server on Windows, ahead of the rest of phase 5, so a surprise there can't derail phases 2–4.

## 7. Testing strategy
- **Pure logic in `rmx-core`** (unit tests plus `proptest`): layout tree, key and chord parser, key normalisation, encoder byte tables, config merging and validation, target parsing.
- **Emulator golden tests:** feed recorded byte streams (vim startup, htop, a ConPTY repaint) and compare the resulting `Buffer` with stored snapshots (`insta`).
- **UI snapshots:** ratatui `TestBackend` renders of the status bar, palette, menus and which-key popup.
- **Integration:** the server core with the in-memory transport and a fake PTY, driven by scripted `ClientMsg` messages, with assertions on the frames that come back.
- **PTY smoke test in CI on every OS:** spawn a real shell, send `echo rmx-ok\r`, and check the screen contains `rmx-ok`. On Windows this also proves the DSR reply works.
- **Regression rule:** every bug fix gets a test, usually a recorded byte stream.
- **Manual matrix before each release:** Windows Terminal, conhost, VS Code terminal; GNOME Terminal, kitty, Alacritty; iTerm2, Terminal.app; over SSH.

## 8. Risks and open questions

| Risk | Impact | Mitigation |
|---|---|---|
| ConPTY hangs waiting for a DSR reply | The shell hangs on Windows | Phase-1 DSR/DA replies, covered by the CI smoke test |
| ConPTY repaints and resize artifacts; differences between ConPTY versions | Redraw glitches | Bundle `conpty.dll`/`OpenConsole.exe`; treat ConPTY output as the true screen state |
| `ClosePseudoConsole` deadlocks | Hang when closing a pane | Fixed close order, keep draining output until EOF, test that close finishes within a timeout |
| Windows host refuses job breakaway | Server dies when the terminal closes | Retry without the flag and warn the user; document it in `rmx doctor` |
| `interprocess` lacks pipe flags | Security gap on Windows | Verify in phase 5; fall back to `tokio::net::windows::named_pipe` behind the `Transport` trait |
| vt100 gaps (no reflow, OSC 8, mode 2026, strikethrough) | Less accurate rendering than tmux | `Emulator` trait; plan for `alacritty_terminal` if the gaps hurt |
| Outer terminal takes keys | Bindings "don't work" | Three-layer defaults, `rmx doctor` snippet, hint bar shows the leader |
| Cell-diff bandwidth when output floods | Lag with huge output | Frame coalescing, only the latest frame for slow clients, optional lz4 later |
| psmux already exists | Unclear positioning | Compete on UX, behaving the same on every OS, persistence and the API, not tmux compatibility |

**Open questions for you:**
1. **Leader key default:** Ctrl+Space (easy to type, but switches input language on macOS and toggles the IME for Chinese/Japanese/Korean input on Windows) or Ctrl+B (tmux muscle memory, but conflicts with readline)? The plan assumes **Ctrl+Space**, with a first-run notice showing how to change it.
2. **Name:** decided — package **`rust-multiplexer`**, free on crates.io as of 2026-10-24. `rmux` and `rust-terminal` were already taken. The command you type is the short **`rmx`** (the `[[bin]]` name). Internal crates are named `rmx-core`, `rmx-proto` and so on, and are not published separately.
3. **Licence:** MIT/Apache-2.0 dual (Rust convention) or proprietary Energma?
4. **Minimum macOS / Linux targets:** proposed macOS 12+, glibc 2.31+ plus a static musl build.

## 9. Immediate next steps (after you approve)
1. `git init`, convert to the workspace layout, and add CI (phase 0).
2. Phase 1: PTY, emulator, renderer and encoder for one pane, then a demo on Linux here. Windows and macOS are checked through CI and you.
3. Spike: detached ConPTY server on Windows.
