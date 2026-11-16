#!/usr/bin/env bash
# Regenerate assets/screenshot.svg from a live run of the TUI.
# Renders macbat headless in tmux, waits for the history to fill, then converts the
# colored capture to an SVG terminal window with scripts/ansi2svg.py.
#   scripts/screenshot.sh            # wait 65 s (fills the 1m history window)
#   WAIT=10 scripts/screenshot.sh    # quicker, sparser charts
# Uses a throwaway HOME so ~/.config/macbat.json is neither read nor written.
set -euo pipefail

cd "$(dirname "$0")/.."

cols=120
rows=32
wait="${WAIT:-65}"
out=assets/screenshot.svg
session="macbat-screenshot-$$"

tmp="$(mktemp -d)"
trap 'tmux kill-session -t "$session" 2>/dev/null || true; rm -rf "$tmp"' EXIT

mkdir -p "$tmp/.config"
echo '{"color":0,"view":"sparkline","window":0}' > "$tmp/.config/macbat.json"

scripts/build.sh
tmux new-session -d -s "$session" -x "$cols" -y "$rows" "HOME='$tmp' ./target/release/macbat"
echo "waiting ${wait}s for history to fill…"
sleep "$wait"
tmux capture-pane -e -p -t "$session" > "$tmp/capture.ansi"

mkdir -p "$(dirname "$out")"
python3 scripts/ansi2svg.py "$tmp/capture.ansi" "$cols" "$rows" --title macbat > "$out"
echo "wrote $out"
