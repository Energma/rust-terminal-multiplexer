#!/usr/bin/env bash
# Fails when any tracked Rust source file is longer than the limit.
# Why: long files hide several responsibilities; a hard cap forces the split
# decision at review time instead of after the file is a god-object.
#
# Canonical copy: Agent-Harness skills/energma-rust-standards. Keep in sync.
# Usage: check-file-length.sh [max-lines] [path...]   (defaults: 200, repo root)
set -euo pipefail

max="${1:-200}"
shift || true
paths=("${@:-.}")

over=0
while IFS= read -r -d '' file; do
  lines=$(wc -l < "$file")
  if (( lines > max )); then
    echo "$file: $lines lines (limit $max)"
    over=1
  fi
done < <(find "${paths[@]}" -name '*.rs' -not -path '*/target/*' -print0)

if (( over )); then
  echo "Split the files above by responsibility (see docs/DEVELOPMENT.md)." >&2
  exit 1
fi
echo "All Rust files are within $max lines."
