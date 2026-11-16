#!/usr/bin/env python3
"""Convert an ANSI terminal capture (tmux capture-pane -e -p) into an SVG "screenshot".

The SVG is drawn like a terminal window (rounded frame, traffic lights, title). Box-drawing
characters, block elements and braille are drawn as shapes rather than font glyphs, so borders,
sparklines and charts line up exactly whatever monospace font the viewer has. Everything else is
text placed on a fixed cell grid.

usage: ansi2svg.py CAPTURE.ansi COLS ROWS [--title TITLE] > out.svg
"""

import argparse
import re
import sys
from html import escape

CELL_W, CELL_H, FONT_SIZE = 8.4, 17.0, 14
PAD, TITLE_H, MARGIN = 16, 36, 24
STROKE = 1.2

BG = "#16181d"
FG = "#d7dae0"
PALETTE = [  # 0-7 normal, 8-15 bright
    "#3f4451", "#ef596f", "#89ca78", "#e5c07b", "#61afef", "#d55fde", "#2bbac5", "#abb2bf",
    "#6b7280", "#ff6b81", "#a5e075", "#f0d58a", "#7cc4ff", "#e57ef0", "#4fd6e0", "#ffffff",
]
FONT = "ui-monospace, SFMono-Regular, 'SF Mono', Menlo, Consolas, 'Liberation Mono', monospace"

# Box drawing: (up, down, left, right, rounded)
BOX = {
    "─": (0, 0, 1, 1, 0), "│": (1, 1, 0, 0, 0),
    "┌": (0, 1, 0, 1, 0), "┐": (0, 1, 1, 0, 0), "└": (1, 0, 0, 1, 0), "┘": (1, 0, 1, 0, 0),
    "╭": (0, 1, 0, 1, 1), "╮": (0, 1, 1, 0, 1), "╰": (1, 0, 0, 1, 1), "╯": (1, 0, 1, 0, 1),
    "├": (1, 1, 0, 1, 0), "┤": (1, 1, 1, 0, 0), "┬": (0, 1, 1, 1, 0), "┴": (1, 0, 1, 1, 0),
    "┼": (1, 1, 1, 1, 0),
}
LOWER = {c: (i + 1) / 8 for i, c in enumerate("▁▂▃▄▅▆▇█")}
LEFT = {c: (i + 1) / 8 for i, c in enumerate("▏▎▍▌▋▊▉")}


def xterm256(n):
    if n < 16:
        return PALETTE[n]
    if n < 232:
        n -= 16
        lv = [0, 95, 135, 175, 215, 255]
        return "#%02x%02x%02x" % (lv[n // 36], lv[n // 6 % 6], lv[n % 6])
    v = 8 + (n - 232) * 10
    return "#%02x%02x%02x" % (v, v, v)


def parse(text, cols, rows):
    """Return a rows×cols grid of (char, fg, bg, bold)."""
    grid = [[(" ", None, None, False)] * cols for _ in range(rows)]
    fg = bg = None
    bold = False
    for r, line in enumerate(text.split("\n")[:rows]):
        c = 0
        for tok in re.split(r"(\x1b\[[0-9;:]*m)", line):
            if tok.startswith("\x1b["):
                codes = [int(x) if x else 0 for x in re.split(r"[;:]", tok[2:-1])] or [0]
                i = 0
                while i < len(codes):
                    k = codes[i]
                    if k == 0:
                        fg = bg = None
                        bold = False
                    elif k == 1:
                        bold = True
                    elif k == 22:
                        bold = False
                    elif 30 <= k <= 37:
                        fg = PALETTE[k - 30]
                    elif 90 <= k <= 97:
                        fg = PALETTE[k - 90 + 8]
                    elif 40 <= k <= 47:
                        bg = PALETTE[k - 40]
                    elif 100 <= k <= 107:
                        bg = PALETTE[k - 100 + 8]
                    elif k == 39:
                        fg = None
                    elif k == 49:
                        bg = None
                    elif k in (38, 48) and i + 1 < len(codes):
                        if codes[i + 1] == 5 and i + 2 < len(codes):
                            color, i = xterm256(codes[i + 2]), i + 2
                        elif codes[i + 1] == 2 and i + 4 < len(codes):
                            color, i = "#%02x%02x%02x" % tuple(codes[i + 2:i + 5]), i + 4
                        else:
                            color = None
                        if k == 38:
                            fg = color
                        else:
                            bg = color
                    i += 1
            else:
                for ch in tok:
                    if c < cols:
                        grid[r][c] = (ch, fg, bg, bold)
                    c += 1
    return grid


def f(v):
    return f"{v:.2f}".rstrip("0").rstrip(".")


def render(grid, cols, rows, title):
    W, H = CELL_W, CELL_H
    win_w = cols * W + 2 * PAD
    win_h = rows * H + 2 * PAD + TITLE_H
    total_w, total_h = win_w + 2 * MARGIN, win_h + 2 * MARGIN
    ox, oy = MARGIN + PAD, MARGIN + TITLE_H + PAD

    out = [
        f'<svg xmlns="http://www.w3.org/2000/svg" width="{f(total_w)}" height="{f(total_h)}" '
        f'viewBox="0 0 {f(total_w)} {f(total_h)}">',
        '<defs><filter id="shadow" x="-10%" y="-10%" width="120%" height="130%">'
        '<feDropShadow dx="0" dy="8" stdDeviation="10" flood-color="#000" flood-opacity="0.35"/>'
        "</filter></defs>",
        f'<rect x="{MARGIN}" y="{MARGIN}" width="{f(win_w)}" height="{f(win_h)}" rx="10" '
        f'fill="{BG}" stroke="#ffffff" stroke-opacity="0.12" filter="url(#shadow)"/>',
    ]
    for i, color in enumerate(["#ff5f57", "#febc2e", "#28c840"]):
        out.append(f'<circle cx="{MARGIN + 20 + i * 20}" cy="{MARGIN + TITLE_H / 2}" r="6" fill="{color}"/>')
    out.append(
        f'<text x="{f(MARGIN + win_w / 2)}" y="{f(MARGIN + TITLE_H / 2 + 4.5)}" text-anchor="middle" '
        f'font-family="{FONT}" font-size="13" fill="#9ca3af">{escape(title)}</text>'
    )

    bgs, shapes, texts = [], {}, []
    blocks = {}  # (x0, x1, top_frac, color) -> rows; merged into rects below

    def block(c0, c1, r, frac_top, color):
        blocks.setdefault((c0, c1, frac_top, color or FG), []).append(r)

    def shape(color, s):
        shapes.setdefault(color or FG, []).append(s)

    for r in range(rows):
        y = oy + r * H
        # background runs
        c = 0
        while c < cols:
            bg = grid[r][c][2]
            start = c
            while c < cols and grid[r][c][2] == bg:
                c += 1
            if bg:
                bgs.append(f'<rect x="{f(ox + start * W)}" y="{f(y)}" width="{f((c - start) * W)}" height="{f(H)}" fill="{bg}"/>')

        c = 0
        while c < cols:
            ch, fg, _, bold = grid[r][c]
            x = ox + c * W
            cx, cy = x + W / 2, y + H / 2
            if ch == "─":  # merge horizontal runs into one segment
                start = c
                while c + 1 < cols and grid[r][c + 1][0] == "─" and grid[r][c + 1][1] == fg:
                    c += 1
                shape(fg, f"M{f(x)} {f(cy)}H{f(ox + (c + 1) * W)}")
            elif ch in BOX:
                up, down, left, right, rounded = BOX[ch]
                if rounded:
                    rad = W / 2
                    vy = y if up else y + H
                    hx = x if left else x + W
                    sy = cy - rad if up else cy + rad
                    sx = cx - rad if left else cx + rad
                    sweep = 1 if (down and right) or (up and left) else 0
                    shape(fg, f"M{f(cx)} {f(vy)}V{f(sy)}A{f(rad)} {f(rad)} 0 0 {sweep} {f(sx)} {f(cy)}H{f(hx)}")
                else:
                    if up or down:
                        shape(fg, f"M{f(cx)} {f(cy if not up else y)}V{f(cy if not down else y + H)}")
                    if left or right:
                        shape(fg, f"M{f(cx if not left else x)} {f(cy)}H{f(cx if not right else x + W)}")
            elif ch in LOWER:  # merge horizontal runs of the same height
                start = c
                while c + 1 < cols and grid[r][c + 1][0] == ch and grid[r][c + 1][1] == fg:
                    c += 1
                block(start, c + 1, r, 1 - LOWER[ch], fg)
            elif ch in LEFT:
                block(c, c + LEFT[ch], r, 0, fg)
            elif 0x2800 < ord(ch) <= 0x28FF:
                bits = ord(ch) - 0x2800
                dots = [(0, 0), (0, 1), (0, 2), (1, 0), (1, 1), (1, 2), (0, 3), (1, 3)]
                for b, (dx, dy) in enumerate(dots):
                    if bits & (1 << b):
                        px, py = x + (dx + 0.5) * W / 2, y + (dy + 0.5) * H / 4
                        texts.append(f'<circle cx="{f(px)}" cy="{f(py)}" r="1.6" fill="{fg or FG}"/>')
            elif ch != " " and ch != "⠀":
                start = c
                run = ch
                while (
                    c + 1 < cols
                    and grid[r][c + 1][1:] == (fg, grid[r][c][2], bold)
                    and grid[r][c + 1][0] not in BOX
                    and grid[r][c + 1][0] not in LOWER
                    and grid[r][c + 1][0] not in LEFT
                    and not 0x2800 <= ord(grid[r][c + 1][0]) <= 0x28FF
                ):
                    c += 1
                    run += grid[r][c][0]
                stripped = run.rstrip(" ")
                xs = " ".join(f(ox + (start + i) * W) for i in range(len(stripped)))
                weight = ' font-weight="700"' if bold else ""
                texts.append(
                    f'<text x="{xs}" y="{f(y + H * 0.74)}" fill="{fg or FG}"{weight} xml:space="preserve">'
                    f"{escape(stripped)}</text>"
                )
                c = start + len(run) - 1
            c += 1

    # stack identical full-height runs on consecutive rows into one rect
    for (c0, c1, top, color), rs in blocks.items():
        rs.sort()
        i = 0
        while i < len(rs):
            j = i
            while top == 0 and j + 1 < len(rs) and rs[j + 1] == rs[j] + 1:
                j += 1
            y0 = oy + rs[i] * H + top * H
            bgs.append(
                f'<rect x="{f(ox + c0 * W)}" y="{f(y0)}" width="{f((c1 - c0) * W)}" '
                f'height="{f(oy + (rs[j] + 1) * H - y0)}" fill="{color}"/>'
            )
            i = j + 1

    out.append('<g shape-rendering="crispEdges">')
    out += bgs
    out.append("</g>")
    for color, segs in shapes.items():
        out.append(
            f'<path d="{"".join(segs)}" fill="none" stroke="{color}" stroke-width="{STROKE}" stroke-linecap="square"/>'
        )
    out.append(f'<g font-family="{FONT}" font-size="{FONT_SIZE}">')
    out += texts
    out.append("</g></svg>")
    return "\n".join(out) + "\n"


def main():
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("capture")
    ap.add_argument("cols", type=int)
    ap.add_argument("rows", type=int)
    ap.add_argument("--title", default="")
    a = ap.parse_args()
    with open(a.capture, encoding="utf-8") as fh:
        grid = parse(fh.read(), a.cols, a.rows)
    sys.stdout.write(render(grid, a.cols, a.rows, a.title))


if __name__ == "__main__":
    main()
