#!/usr/bin/env bash
# Build macbat. Release (LTO + stripped) by default; pass --debug for a debug build.
# Any other arguments are passed through to cargo.
set -euo pipefail

cd "$(dirname "$0")/.."

profile=release
args=()
for arg in "$@"; do
  case "$arg" in
    --debug) profile=debug ;;
    *) args+=("$arg") ;;
  esac
done

if [[ "$profile" == release ]]; then
  cargo build --release ${args[@]+"${args[@]}"}
else
  cargo build ${args[@]+"${args[@]}"}
fi

bin="target/$profile/macbat"
echo "built $bin ($(du -h "$bin" | cut -f1 | tr -d ' '))"
