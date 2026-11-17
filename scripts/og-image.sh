#!/usr/bin/env bash
# Render site/og.png (1200×630 Open Graph image) from assets/screenshot.svg with headless Chrome.
# Run by scripts/screenshot.sh; set CHROME to override the browser path.
set -euo pipefail

cd "$(dirname "$0")/.."

chrome="${CHROME:-/Applications/Google Chrome.app/Contents/MacOS/Google Chrome}"
if [[ ! -x "$chrome" ]]; then
  echo "og-image: Chrome not found at $chrome (set CHROME), skipping site/og.png" >&2
  exit 0
fi

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

python3 - "$tmp/og.html" <<'PY'
import re, sys

svg = open("assets/screenshot.svg", encoding="utf-8").read().strip()
svg = re.sub(r'^<svg([^>]*?) width="[^"]*" height="[^"]*"', r"<svg\1", svg)

html = """<!doctype html>
<html><head><meta charset="utf-8"><style>
  * { box-sizing: border-box; }
  html, body { margin: 0; width: 1200px; height: 630px; overflow: hidden; }
  body {
    position: relative;
    background:
      radial-gradient(900px 420px at 85% 0%, rgba(137, 202, 120, 0.22), transparent 70%),
      radial-gradient(700px 400px at 0% 100%, rgba(41, 163, 131, 0.16), transparent 70%),
      #17181c;
    color: #e6e7ea;
    font-family: -apple-system, BlinkMacSystemFont, "Inter", "Segoe UI", Roboto, sans-serif;
    -webkit-font-smoothing: antialiased;
  }
  .text { position: absolute; left: 72px; top: 56px; right: 72px; }
  .brand { display: flex; align-items: center; gap: 16px; font-size: 34px; font-weight: 700; }
  .brand svg { width: 52px; height: 52px; }
  .url { margin-left: auto; font-size: 22px; font-weight: 500; color: #a1a5b0; }
  h1 { margin: 26px 0 10px; font-size: 54px; line-height: 1.08; letter-spacing: -0.02em; }
  p { margin: 0; font-size: 25px; line-height: 1.35; color: #a1a5b0; }
  .shot { position: absolute; left: 76px; top: 286px; width: 1048px; }
  .shot svg { display: block; width: 100%; height: auto; }
</style></head><body>
  <div class="text">
    <div class="brand">
      <svg viewBox="0 0 64 64"><defs><linearGradient id="g" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="#89ca78"/><stop offset="1" stop-color="#29a383"/></linearGradient></defs><rect width="64" height="64" rx="14" fill="url(#g)"/><rect x="12" y="21" width="36" height="22" rx="5" fill="none" stroke="#fff" stroke-width="4"/><rect x="50" y="27" width="4" height="10" rx="2" fill="#fff"/><path d="M32 24l-7 9h6l-3 7 9-10h-6l3-6z" fill="#fff"/></svg>
      macbat
      <span class="url">macbat.lgtm.page</span>
    </div>
    <h1>See where your MacBook’s power goes</h1>
    <p>A sudoless terminal power monitor for Apple Silicon laptops.</p>
  </div>
  <div class="shot">SCREENSHOT</div>
</body></html>
""".replace("SCREENSHOT", svg)
open(sys.argv[1], "w", encoding="utf-8").write(html)
PY

"$chrome" --headless=new --disable-gpu --hide-scrollbars --force-device-scale-factor=1 \
  --window-size=1200,630 --screenshot="$PWD/site/og.png" "file://$tmp/og.html" >/dev/null 2>&1
echo "wrote site/og.png"
