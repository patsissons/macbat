#!/usr/bin/env bash
# Build a release binary and install it to $BINDIR (default: ~/.local/bin).
#   scripts/install.sh
#   BINDIR=/usr/local/bin scripts/install.sh
set -euo pipefail

cd "$(dirname "$0")/.."

bindir="${BINDIR:-$HOME/.local/bin}"

scripts/build.sh

mkdir -p "$bindir"
install -m 755 target/release/macbat "$bindir/macbat"
echo "installed $bindir/macbat"

case ":$PATH:" in
  *":$bindir:"*) ;;
  *) echo "note: $bindir is not on your PATH" >&2 ;;
esac
