# Configuring rmx

rmx reads one TOML file:

| OS | Default location |
|---|---|
| Windows | `%APPDATA%\rmx\config.toml` |
| Linux | `$XDG_CONFIG_HOME/rmx/config.toml` (usually `~/.config/rmx/config.toml`) |
| macOS | `~/.config/rmx/config.toml` if that folder exists, else `~/Library/Application Support/rmx/config.toml` |

`RMX_CONFIG=/path/to/file` or `rmx --config /path/to/file` overrides it, and `rmx config path` prints the one in use.

- `rmx config init` writes a commented starter file. The same file is also at `crates/rmx-config/src/default_config.toml` in this repo.
- Changes apply as soon as you save. Panes keep running. If the file has an error, rmx keeps the previous settings and shows a message with the line and column.
- `rmx config check` validates the file and exits non-zero on errors, so it can run in CI or a dotfiles hook.
- `rmx keys` (or `rmx keys --mode leader`) prints the bindings in effect, and marks the ones that come from your file.

## Keys

Key syntax is `modifier+modifier+key`.

- **Modifiers:** `ctrl`, `alt`, `shift`, `super`.
- **Keys:** letters, digits, symbols, `f1`–`f12`, `enter`, `esc`, `tab`, `space`, `backspace`, `delete`, `insert`, `home`, `end`, `pgup`, `pgdn`, the arrow keys, `plus`, `minus`.
- **Case:** names are case-insensitive, so `Ctrl+T` is the same key as `ctrl+t`. Write `shift+t` when you mean Shift.
- **Sequences:** separate keys with spaces, as in `"ctrl+space z"`.

Bindings live in key tables (modes):

| Mode | When it is active |
|---|---|
| `normal` | Always. Keys that aren't bound here go to the program in the pane. |
| `leader` | For one key after the leader key (default `Ctrl+Space`). Unbound keys are ignored. Pressing the leader twice sends it to the pane. |
| `select` | Select/scroll mode (Leader `[`, the mouse wheel, or dragging to select). |
| `resize` | Leader `r`. Arrow keys resize the pane until you press Esc. |
| `locked` | Leader `l`. Every key goes to the pane except the unlock key, which is useful for nested rmx/tmux or for apps whose keys clash. |
| *your own* | Any `[keys.<name>]` table. Enter it with `{ action = "enter-mode", mode = "<name>", sticky = true }`. |

A binding's value is either an action id (`"new-tab"`), or a table with arguments (`{ action = "split-pane", direction = "down", size = 30 }`), or `"none"` to remove a preset binding.

## Presets

`preset = "windows"` (the default), `"tmux"`, `"vim"` or `"none"`. Your bindings are applied on top of the preset. Remove a line from your file and the preset's binding comes back.

`direct_keys = "auto" | "on" | "off"` controls the Windows Terminal style layer: Ctrl+Shift+T new tab, Ctrl+Shift+P palette, Alt+Arrow focus, Alt+Shift+Arrow resize, and so on.

- With `auto` (the default), the layer is on whenever the terminal can report those keys: always on Windows, and on other systems in terminals that speak the kitty keyboard protocol (kitty, WezTerm, Ghostty, foot, recent Alacritty and iTerm2).
- Windows Terminal handles some of these keys itself before rmx sees them. `rmx doctor` prints a snippet that frees them.

## Action arguments

| Argument | Used by | Values |
|---|---|---|
| `direction` | `split-pane`, `move-focus`, `resize-pane`, `swap-pane`, `select-move`, `select-extend` | `left`, `right`, `up`, `down` |
| `size` | `split-pane` | percent of the current pane: `30` or `"30%"` |
| `command` | `split-pane`, `run` | a command line, run through the system shell |
| `in` | `run` | `new-tab` (default), `split-right`, `split-down`, `background` |
| `index` | `go-to-tab` | 1-based tab number |
| `delta` | `move-tab` | e.g. `-1`, `1` |
| `amount` | `resize-pane` | cells (default 2) |
| `lines` | `scroll-up`, `scroll-down` | default 3 |
| `name` | `apply-layout` | `even-horizontal`, `even-vertical`, `main-left`, `main-top`, `tiled`, or `next` |
| `mode`, `sticky` | `enter-mode` | mode name, and whether it stays active after one key |
| `keys` | `send-keys` | text with `\r` `\n` `\t` `\e` escapes |

## All actions

Every action is also listed in the command palette (Leader `p`), together with its current key.

| Action | Menu | What it does |
|---|---|---|
| `new-tab` | Tab | Open a new tab with the default shell |
| `close-tab` | Tab | Close the current tab and all its panes |
| `rename-tab` | Tab | Give the current tab a name |
| `next-tab` | Tab | Switch to the tab on the right |
| `prev-tab` | Tab | Switch to the tab on the left |
| `go-to-tab` | Tab | Switch to tab number `index` (1-based) |
| `move-tab` | Tab | Move the current tab `delta` places |
| `split-pane` | Pane | Split the focused pane in `direction` |
| `close-pane` | Pane | Close the focused pane |
| `move-focus` | Pane | Focus the neighbouring pane |
| `resize-pane` | Pane | Move the focused pane's border |
| `toggle-pane-zoom` | Pane | Toggle the focused pane filling the tab |
| `swap-pane` | Pane | Swap the focused pane with its neighbour |
| `apply-layout` | Pane | Rearrange the tab's panes |
| `toggle-sync-input` | Pane | Type into every pane of the tab at once |
| `select-mode` | Edit | Scroll back and select text with the keyboard |
| `find` | Edit | Search the scrollback |
| `find-next` | Edit | Jump to the next search match |
| `find-prev` | Edit | Jump to the previous search match |
| `copy` | Edit | Copy the selection to the clipboard |
| `paste` | Edit | Paste the clipboard into the focused pane |
| `copy-and-exit` | Edit | Copy the selection and leave select mode |
| `select-move` | Edit | Move the select-mode cursor |
| `select-extend` | Edit | Move the cursor, starting a selection if there is none (Shift+arrows) |
| `select-line-start` | Edit | Move the cursor to the start of the line |
| `select-line-end` | Edit | Move the cursor to the end of the line |
| `select-start` | Edit | Start or clear a selection at the cursor |
| `select-line` | Edit | Select whole lines |
| `select-word-left` | Edit | Move the cursor one word left |
| `select-word-right` | Edit | Move the cursor one word right |
| `scroll-up` | View | Scroll the pane up `lines` lines |
| `scroll-down` | View | Scroll the pane down `lines` lines |
| `scroll-page-up` | View | Scroll up one screen |
| `scroll-page-down` | View | Scroll down one screen |
| `scroll-to-top` | View | Jump to the oldest scrollback line |
| `scroll-to-bottom` | View | Jump back to the live screen |
| `command-palette` | View | Search and run any action |
| `open-menu` | View | Open the menu bar |
| `show-help` | Help | List the key bindings |
| `workspace-switcher` | Workspace | Pick a workspace, tab or pane |
| `new-workspace` | Workspace | Create a new workspace |
| `rename-workspace` | Workspace | Give the current workspace a name |
| `next-workspace` | Workspace | Switch to the next workspace |
| `prev-workspace` | Workspace | Switch to the previous workspace |
| `detach` | File | Detach; everything keeps running in the background |
| `quit` | File | Close every pane and stop the background service |
| `reload-config` | File | Re-read config.toml |
| `open-config` | File | Edit config.toml in a new tab |
| `enter-mode` | View | Switch to another key table |
| `exit-mode` | View | Return to normal mode |
| `lock` | View | Send every key to the pane until unlocked |
| `run` | File | Run `command` in a new tab, split or the background |
| `send-keys` | Edit | Type `keys` into the focused pane |

## Theme

Set colours in `[theme]` as `"#rrggbb"`, an ANSI index (`"238"`) or a name (`"blue"`). The keys are `status_bg`, `status_fg`, `accent`, `border`, `active_border`, `menu_bg`, `menu_fg`, `menu_highlight_bg` and `menu_highlight_fg`.

## Sessions

- Workspaces are saved automatically every 30 seconds after a change, and again when the server stops. `rmx save` saves them immediately.
- After a reboot, `rmx restore` brings them back: layouts, tab names, each pane's folder, and any command a pane was started with. Running programs themselves can't survive a reboot; panes come back as fresh shells in the same folders.
- `restore_on_start = true` restores them automatically whenever the background server starts.
- The session file is `<local data dir>/rmx/<server>-session.json`.
