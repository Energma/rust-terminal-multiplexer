# Developing rmx

How we build `rmx` day to day: where code goes, the rules every change follows, and the
checks that must pass. The house style for all Energma Rust is the `energma-rust-standards`
skill in Agent-Harness. This page applies it to this repo. The product design is in [PLAN.md](PLAN.md).

## Principles

- **KISS.** Choose the solution a reviewer understands in one read.
- **YAGNI.** Add no trait, generic, builder or crate "for later". Add an abstraction when its second real caller arrives.
- **DRY of knowledge, not of look-alike code.** There is one action registry, one key parser and one config parser. Two blocks that only look the same today can stay separate.
- **Make illegal states unrepresentable.** Use enums instead of combinations of flags, and newtypes (`PaneId`) instead of bare integers.
- **Behaviour-neutral refactors land alone.** Never mix them with features or fixes.

## Workspace layout

Crates are layered by dependency direction. Pure logic sits at the bottom, I/O at the top, and
there are no cycles.

```
rust-multiplexer (src/)      the `rmx` binary: CLI, logging, self-update
├── rmx-client               terminal front end, input capture, clipboard
└── rmx-server               the core task: workspaces, panes, UI, persistence, control API
    ├── rmx-ipc              sockets / named pipes, detached spawn, security
    ├── rmx-config           actions, keys, keymap engine, presets, config.toml
    ├── rmx-core             pure logic: layout tree, terminal emulator, key encoder
    └── rmx-proto            client/server messages and wire codec
```

Each crate's `lib.rs` exposes its API through `pub use`. Other crates import from there, not from internal module paths.

### Module map

| Where | What lives there |
|---|---|
| `src/main.rs` | Entry point only: parse args, reset SIGPIPE, call `cli::run` |
| `src/cli/` | clap types (`args.rs`), dispatch, one file per command group (`session`, `workspace`, `config`, `doctor`, …) |
| `src/update/` | `rmx update`: find the release, download, verify, replace |
| `rmx-client/src/` | `session` (message loop), `render`, `input`, `terminal`, `clipboard` |
| `rmx-config/src/action/` | `Action` enum; `registry/` (menu/palette metadata by category); `parse/` (`args/`, builders) |
| `rmx-config/src/config/` | `Config` type; `parse/` (sections, values, keys, warnings); `load.rs`; `error.rs` |
| `rmx-config/src/keys/` | Key-chord parser: `parse`, `normalize`, `display` |
| `rmx-config/src/keymap/` | Binding tables, hints, and `engine/` (leader/mode state machine) |
| `rmx-config/src/presets/` | One file per preset (`windows`, `tmux`) plus shared bindings |
| `rmx-core/src/` | `layout/` (tree, geometry, resize, arrange), `emulator/` (vt100 backend), `encode/` |
| `rmx-proto/src/` | `msg` (messages), `wire` (cells and colours), `channel` (in-memory), `codec` |
| `rmx-ipc/src/` | Neutral `endpoint`, `spawn` and `bridge`; `unix/` and `windows/` behind the same names |
| `rmx-server/src/core/` | `Core`: one `impl Core` block per concern (`event_loop`, `input`, `panes`, `workspaces`, `control`, `persistence`, …), plus `actions/` (one file per action group) and `mouse/` |
| `rmx-server/src/ui/` | Overlays: `menu/`, `palette/`, `manager/` (session manager), `dialog`, `help` |
| `rmx-server/src/` | `compositor/` (frames, status bar, separators), `select/` (select mode), `pane/` (PTY lifecycle) |
| `rmx-server/tests/` | Real-PTY integration tests with shared helpers in `common/` |

## Code rules

1. **No `.rs` file over 200 lines, tests included.**
   - CI enforces this with `scripts/check-file-length.sh`.
   - When a file grows past the limit, split it by responsibility, never by cutting at line 200.
2. **Folder modules use `mod.rs`.**
   - Keep `mod.rs` thin: a `//!` doc, `mod` declarations, `pub use` re-exports, and at most the central type.
   - When an item moves, re-export it from its old path so callers don't break.
3. **Split a large `impl` into several `impl` blocks**, one per concern, in sibling files (see `rmx-server/src/core/`).
4. **Give items the narrowest visibility that compiles.** Items shared between sibling files are `pub(super)` or `pub(crate)`. Never `pub` for convenience.
5. **Start every module with a `//!` line** saying what it owns. Comments explain *why*; the code already shows how.
6. **Errors.**
   - Use `anyhow` with `.context(..)` in the binary.
   - Config errors carry a `file:line:col` location and a "did you mean" suggestion.
   - No `unwrap()` on user input, files or IPC.
7. **Keep platform code in `unix/` and `windows/` modules** that share function names. Don't put `#[cfg]` in business logic.
8. **Concurrency.**
   - The server core is one task that owns all state. Everything else sends it messages; there are no shared locks.
   - Blocking PTY I/O runs on dedicated threads.

## Tests

- Unit tests sit next to the code in a sibling file: `#[cfg(test)] mod tests;` points to `tests.rs`, or to a `tests/` folder.
- Integration tests are in `crates/rmx-server/tests/`, with shared helpers in `tests/common/mod.rs`.
- Every bug fix gets a regression test.
- Tests that drive a real PTY or socket must have timeouts, so a hang fails CI instead of stalling it.

## Checks (run all before pushing; CI runs the same)

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
bash scripts/check-file-length.sh 200 src crates
# The other OSes, type-check only (one-time setup: rustup target add x86_64-pc-windows-msvc aarch64-apple-darwin)
cargo check --workspace --all-targets --target x86_64-pc-windows-msvc
cargo check --workspace --all-targets --target aarch64-apple-darwin
```

CI (`.github/workflows/ci.yml`) runs:
- fmt and the file-length check
- clippy and tests on Ubuntu and macOS
- an MSRV check with Rust 1.88

Windows is type-checked locally until the named-pipe test hang in `crates/rmx-ipc/src/windows/` is fixed.

For a structural refactor, also confirm:
- **Same tests:** `cargo test --workspace -- --list | grep -c ': test$'` gives the same count before and after.
- **Same CLI help:** `rmx --help` and every subcommand's `--help` print the same text before and after.

## Commits and PRs

- **One concern per commit.** Use a conventional, imperative message that says why, e.g. `refactor(config): split action registry by category` or `fix(ipc): …`.
- **Branch from `main`** and open a PR. The PR is merged only when CI is green.
- **No AI attribution.** No `Co-Authored-By` trailers for tools.
- **Releases** follow the `energma-release` skill.

## Logs and debugging

Logs go to `<local data dir>/rmx/`: `client.log` and `server-<name>.log`. Set the level with `RMX_LOG=debug`.
