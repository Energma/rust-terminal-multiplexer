#!/usr/bin/env bash
# Prints the next release version (no leading v) for HEAD.
# Why: the release workflow runs after every merge, so the version has to come
# from the commits themselves rather than from someone typing it in.
#
# Usage: release-version.sh [auto|patch|minor|major] [exact-version]
#   auto (default) reads conventional commits since the last v* tag:
#   `!:` or "BREAKING CHANGE" = major, `feat` = minor, anything else = patch.
#   While at 0.x a major bump becomes minor, as semver allows.
#   An exact version wins over the bump.
# Exits 3 when auto finds no commits since the last tag (nothing to release).
set -euo pipefail

bump="${1:-auto}"
exact="${2:-}"

last=$(git tag -l 'v[0-9]*' --sort=-v:refname | head -1)

if [[ -n "$exact" ]]; then
  v="${exact#v}"
else
  if [[ "$bump" == auto ]]; then
    log=$(git log --format='%s%n%b' "${last:+$last..}HEAD")
    [[ -z "$log" ]] && exit 3
    if grep -qE '^[a-z]+(\([^)]*\))?!:|^BREAKING[ -]CHANGE' <<<"$log"; then
      bump=major
    elif grep -qE '^feat(\([^)]*\))?:' <<<"$log"; then
      bump=minor
    else
      bump=patch
    fi
  fi
  IFS=. read -r major minor patch <<<"${last#v}"
  major=${major:-0} minor=${minor:-0} patch=${patch:-0}
  [[ "$bump" == major && "$major" == 0 ]] && bump=minor
  case "$bump" in
    major) v="$((major + 1)).0.0" ;;
    minor) v="$major.$((minor + 1)).0" ;;
    patch) v="$major.$minor.$((patch + 1))" ;;
    *) echo "unknown bump \"$bump\" (auto, patch, minor or major)" >&2; exit 2 ;;
  esac
fi

if ! [[ "$v" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
  echo "\"$v\" is not a version like 1.2.3" >&2
  exit 2
fi
if git rev-parse -q --verify "refs/tags/v$v" >/dev/null; then
  echo "tag v$v already exists" >&2
  exit 2
fi
echo "$v"
