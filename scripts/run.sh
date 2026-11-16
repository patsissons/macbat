#!/usr/bin/env bash
# Run macbat from the current source. cargo rebuilds only what changed (debug profile).
# Arguments are passed to macbat:
#   scripts/run.sh
#   scripts/run.sh pipe -s 1
set -euo pipefail

cd "$(dirname "$0")/.."

exec cargo run --quiet -- "$@"
