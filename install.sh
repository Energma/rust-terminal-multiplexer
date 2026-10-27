#!/bin/sh
# Installs the latest rmx release for this machine. Linux and macOS.
#   curl -fsSL https://raw.githubusercontent.com/Energma/rust-terminal-multiplexer/main/install.sh | sh
# Afterwards, `rmx update` keeps it current.
set -eu

repo="Energma/rust-terminal-multiplexer"
bin_dir="${RMX_INSTALL_DIR:-}"

os=$(uname -s)
arch=$(uname -m)
case "$os:$arch" in
  Linux:x86_64|Linux:amd64)   target=x86_64-unknown-linux-musl ;;
  Darwin:arm64|Darwin:aarch64) target=aarch64-apple-darwin ;;
  Darwin:x86_64)               target=x86_64-apple-darwin ;;
  *) echo "install.sh: no release build for $os/$arch (see the releases page or cargo install)" >&2; exit 1 ;;
esac

tag=$(curl -fsSL "https://api.github.com/repos/$repo/releases/latest" \
  | sed -n 's/.*"tag_name": *"\([^"]*\)".*/\1/p' | head -n1)
[ -n "$tag" ] || { echo "install.sh: no release found for $repo" >&2; exit 1; }

name="rmx-$tag-$target"
url="https://github.com/$repo/releases/download/$tag/$name.tar.gz"
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

echo "downloading $name"
curl -fsSL -o "$tmp/$name.tar.gz" "$url"
curl -fsSL -o "$tmp/$name.tar.gz.sha256" "$url.sha256"
(cd "$tmp" && if command -v sha256sum >/dev/null; then sha256sum -c --quiet "$name.tar.gz.sha256"; else shasum -a 256 -c --quiet "$name.tar.gz.sha256"; fi)
tar xzf "$tmp/$name.tar.gz" -C "$tmp"

if [ -z "$bin_dir" ]; then
  if [ -w /usr/local/bin ]; then bin_dir=/usr/local/bin; else bin_dir="$HOME/.local/bin"; fi
fi
mkdir -p "$bin_dir"
install -m 755 "$tmp/$name/rmx" "$bin_dir/rmx"
echo "installed rmx $tag to $bin_dir/rmx"
case ":$PATH:" in
  *":$bin_dir:"*) ;;
  *) echo "note: add $bin_dir to your PATH" ;;
esac
