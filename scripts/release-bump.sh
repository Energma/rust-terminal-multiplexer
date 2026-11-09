#!/usr/bin/env bash
# Sets the workspace version in Cargo.toml and Cargo.lock.
# Why: `rmx update` compares the binary's own version with the release tag, and
# release builds use --locked, so both files must carry the tag's version.
#
# Usage: release-bump.sh <version>
set -euo pipefail

v="${1:?usage: release-bump.sh <version>}"

sed -i.bak -E "/^\[workspace\.package\]/,/^\[/ s/^version = \"[^\"]*\"/version = \"$v\"/" Cargo.toml
rm -f Cargo.toml.bak
grep -q "^version = \"$v\"" Cargo.toml || { echo "version not set in Cargo.toml" >&2; exit 1; }
cargo update --workspace --quiet
cargo metadata --locked --format-version 1 >/dev/null
