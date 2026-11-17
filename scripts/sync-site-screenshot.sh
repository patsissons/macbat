#!/usr/bin/env bash
# Embed assets/screenshot.svg into site/index.html between the screenshot:start/end markers,
# so the single-page site stays self-contained. Run by scripts/screenshot.sh.
set -euo pipefail

cd "$(dirname "$0")/.."

python3 - <<'PY'
import re

svg = open("assets/screenshot.svg", encoding="utf-8").read().strip()
# Scale to the container: drop the fixed size (the viewBox keeps the aspect ratio) and
# namespace the ids so they can't collide with the page's own.
svg = re.sub(r'^<svg([^>]*?) width="[^"]*" height="[^"]*"', r'<svg\1 role="img" aria-label="macbat running in a terminal"', svg)
svg = svg.replace('id="shadow"', 'id="shot-shadow"').replace("url(#shadow)", "url(#shot-shadow)")

path = "site/index.html"
html = open(path, encoding="utf-8").read()
pattern = re.compile(r"(<!-- screenshot:start[^>]*-->\n).*?(<!-- screenshot:end -->)", re.S)
html, n = pattern.subn(lambda m: m.group(1) + svg + "\n" + m.group(2), html)
if n != 1:
    raise SystemExit("screenshot markers not found in " + path)
open(path, "w", encoding="utf-8").write(html)
print("embedded assets/screenshot.svg in " + path)
PY
