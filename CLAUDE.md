# CLAUDE.md

`rmx` (rust-multiplexer) is a cross-platform terminal multiplexer in a Rust cargo workspace.

- **How we develop:** [docs/DEVELOPMENT.md](docs/DEVELOPMENT.md). Read it first. It covers the layout, the code rules and the checks.
- **Product design and roadmap:** [docs/PLAN.md](docs/PLAN.md). **Config reference:** [docs/CONFIG.md](docs/CONFIG.md).
- **House style:** the `energma-rust-standards` skill (KISS, YAGNI, DRY, folder modules).

Hard rules:
- No `.rs` file over 200 lines (`bash scripts/check-file-length.sh 200 src crates`).
- Behaviour-neutral refactors land on their own.
- Every bug fix gets a regression test.
- No AI attribution in commits or PRs.

Before saying a change is done, run:

```sh
cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
cargo check --workspace --all-targets --target x86_64-pc-windows-msvc
```
