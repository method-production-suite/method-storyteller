"""The family's drawn letters and small drawing helpers, shared by tools/make-brand.py.
Standard library only.

Copied from Method Reader's scripts/make-brand.py (itself Method Schedule's, itself Method
Budget's), so every wordmark in the suite is drawn from the same strokes. Letters added
here: y (the u with Method Budget's g descender), n (the h without its ascender), p (the d
turned over: a bowl with a descending stem on the left) and i (a stem and a dot).
"""

from __future__ import annotations

import math
import struct
import zlib


def fmt(v: float) -> str:
    s = f"{v:.2f}".rstrip("0").rstrip(".")
    return "0" if s in ("-0", "") else s



def rrect(x: float, y: float, w: float, h: float, r: float) -> str:
    """A rounded rectangle as a closed subpath."""
    return (
        f"M{fmt(x + r)} {fmt(y)}H{fmt(x + w - r)}a{fmt(r)} {fmt(r)} 0 0 1 {fmt(r)} {fmt(r)}"
        f"V{fmt(y + h - r)}a{fmt(r)} {fmt(r)} 0 0 1-{fmt(r)} {fmt(r)}H{fmt(x + r)}"
        f"a{fmt(r)} {fmt(r)} 0 0 1-{fmt(r)}-{fmt(r)}V{fmt(y + r)}a{fmt(r)} {fmt(r)} 0 0 1 {fmt(r)}-{fmt(r)}Z"
    )





# ----------------------------------------------------------------------------- name
# Strokes on a y-up grid: baseline 0, x-height 100, ascender 140, stroke 18. Round
# letters overshoot the baseline and x-height by 1.5 so they look as tall as the stems.
# m, e, t, h, o, d and u are Method Budget's letters unchanged; s, c and l are Method
# Schedule's; r and a are new here and follow the same rules.

W = 18.0
R = 43.5  # bowl radius at the stroke's center
C = 50.0  # bowl center height
ASC = 140.0
TASC = 126.0  # t is shorter than d, h and l
BOWL = 2 * R + W  # 105: width of a bowl letter


def bowl(x0: float) -> list[str]:
    cx = x0 + W / 2 + R
    return [f"M{cx - R} {C}A{R} {R} 0 1 1 {cx + R} {C}A{R} {R} 0 1 1 {cx - R} {C}Z"]


def on_bowl(cx: float, cy: float, rx: float, ry: float, deg: float) -> tuple[float, float]:
    a = math.radians(deg)
    return cx + rx * math.cos(a), cy + ry * math.sin(a)


def g_m(x0):
    r = 34.0
    c = 100 - W / 2 - r + 1.5  # arch tops overshoot like the bowls
    s1, s2, s3 = x0 + W / 2, x0 + W / 2 + 2 * r, x0 + W / 2 + 4 * r
    return [
        f"M{s1} 0V100",
        f"M{s1} {c}A{r} {r} 0 0 1 {s2} {c}V0",
        f"M{s2} {c}A{r} {r} 0 0 1 {s3} {c}V0",
    ], 4 * r + W


def g_e(x0):
    cx = x0 + W / 2 + R
    ex, ey = on_bowl(cx, C, R, R, -38)
    return [f"M{cx - R} {C}H{cx + R}A{R} {R} 0 1 0 {ex:.2f} {ey:.2f}"], BOWL


def g_c(x0):
    """An o opened on the right, its ends cut where the e's tail ends."""
    cx = x0 + W / 2 + R
    sx, sy = on_bowl(cx, C, R, R, 40)
    ex, ey = on_bowl(cx, C, R, R, -40)
    return [f"M{sx:.2f} {sy:.2f}A{R} {R} 0 1 0 {ex:.2f} {ey:.2f}"], BOWL - 10


def g_s(x0):
    """Two stacked loops on one spine: over the top and down the left, across the
    middle, down the right and back along the bottom."""
    rx = 31.0
    ry = (100 + 1.5 - W / 2 - (-1.5 + W / 2)) / 4  # 21.25: the two loops share the height
    cx = x0 + W / 2 + rx
    up, lo = 100 + 1.5 - W / 2 - ry, -1.5 + W / 2 + ry
    sx, sy = on_bowl(cx, up, rx, ry, 28)
    ex, ey = on_bowl(cx, lo, rx, ry, -152)
    return [
        f"M{sx:.2f} {sy:.2f}A{rx} {ry} 0 1 0 {cx} {C}"
        f"A{rx} {ry} 0 1 1 {ex:.2f} {ey:.2f}"
    ], 2 * rx + W


def g_t(x0):
    stem = x0 + 22
    return [f"M{stem} {TASC}V0", f"M{x0} {100 - W / 2}H{x0 + 54}"], 54.0


def g_h(x0):
    l, r = x0 + W / 2, x0 + W / 2 + 2 * R
    return [f"M{l} 0V{ASC}", f"M{l} {C}A{R} {R} 0 0 1 {r} {C}V0"], BOWL


def g_l(x0):
    return [f"M{x0 + W / 2} 0V{ASC}"], W


def g_o(x0):
    return bowl(x0), BOWL


def g_d(x0):
    return bowl(x0) + [f"M{x0 + W / 2 + 2 * R} 0V{ASC}"], BOWL


def g_u(x0):
    l, r = x0 + W / 2, x0 + W / 2 + 2 * R
    return [f"M{l} 100V{C}A{R} {R} 0 0 0 {r} {C}", f"M{r} 100V0"], BOWL


def g_a(x0):
    """A single-storey a: the bowl with a stem on its right that stops at the x-height,
    the d without its ascender."""
    return bowl(x0) + [f"M{x0 + W / 2 + 2 * R} 0V{100 + 1.5 - W / 2}"], BOWL


def g_r(x0):
    """A stem and the h's shoulder, stopped where it starts to fall."""
    l = x0 + W / 2
    ex, ey = on_bowl(l + R, C, R, R, 35)
    return [f"M{l} 0V100", f"M{l} {C}A{R} {R} 0 0 1 {ex:.2f} {ey:.2f}"], ex - x0 + W / 2


DESC = -12.0  # where a descender's stem turns into its hook (Method Budget's g)
DESC_END = -64.5  # the lowest ink of a descender, for setting a wordmark's height


def g_n(x0):
    """The h without its ascender."""
    l, r = x0 + W / 2, x0 + W / 2 + 2 * R
    return [f"M{l} 0V100", f"M{l} {C}A{R} {R} 0 0 1 {r} {C}V0"], BOWL


def g_p(x0):
    """The bowl with a stem on its left that runs down to the descender line."""
    return bowl(x0) + [f"M{x0 + W / 2} 100V{DESC_END + W / 2}"], BOWL


def g_y(x0):
    """The u, its right stem carried down into Method Budget's g hook."""
    l, r = x0 + W / 2, x0 + W / 2 + 2 * R
    cx = r - R
    a = math.radians(-158)
    ex, ey = cx + R * math.cos(a), DESC + R * math.sin(a)
    return [f"M{l} 100V{C}A{R} {R} 0 0 0 {r} {C}",
            f"M{r} 100V{DESC}A{R} {R} 0 0 1 {ex:.2f} {ey:.2f}"], BOWL


def g_i(x0):
    """A stem to the x-height and a round dot (a 1-unit circle, so the stroke makes it a
    disc 20 across) above it."""
    x = x0 + W / 2
    return [f"M{x} 0V100", f"M{x - 1} 128A1 1 0 1 1 {x + 1} 128A1 1 0 1 1 {x - 1} 128Z"], W


GLYPHS = {"m": g_m, "e": g_e, "t": g_t, "h": g_h, "o": g_o, "d": g_d, "u": g_u,
          "s": g_s, "c": g_c, "l": g_l, "a": g_a, "r": g_r,
          "n": g_n, "p": g_p, "y": g_y, "i": g_i}
# Space between neighbours, outer edge to outer edge: tighter beside a bowl. The pairs
# Method Budget's, Method Schedule's and Method Reader's names have keep their values.
GAP = {"me": 13, "et": 9, "th": 14, "ho": 13, "od": 12, "ud": 13,
       "sc": 11, "ch": 13, "he": 13, "ed": 12, "du": 14, "ul": 16, "le": 13,
       "re": 6, "ea": 12, "ad": 12, "de": 12, "er": 13,
       "pr": 13, "ro": 7, "uc": 12, "ct": 9, "ti": 14, "io": 14, "on": 13,
       "st": 10, "to": 10, "or": 13, "ry": 5, "yt": 6, "el": 14, "ll": 16, "su": 12, "ui": 14, "it": 14}
WORD_SPACE = 52.0


def set_name(text: str) -> tuple[list[str], float]:
    """Paths (y up) and the advance width for a lowercase phrase."""
    x, paths, prev = 0.0, [], ""
    for ch in text:
        if ch == " ":
            x += WORD_SPACE
            prev = ""
            continue
        if prev:
            x += GAP.get(prev + ch, 13)
        p, w = GLYPHS[ch](x)
        paths += p
        x += w
        prev = ch
    return paths, x


def flip(path: str, base: float) -> str:
    """Turn a y-up path into SVG's y-down space with the baseline at `base`. Arc sweep
    flags are written as they look on screen (1 turns clockwise), so they stay."""
    out, tokens = [], tokenize(path)
    i = 0
    cmd = ""
    while i < len(tokens):
        t = tokens[i]
        if t.isalpha():
            cmd = t
            out.append(t)
            i += 1
            continue
        if cmd in "ML":
            x, y = float(tokens[i]), float(tokens[i + 1])
            out.append(f"{fmt(x)} {fmt(base - y)}")
            i += 2
        elif cmd == "H":
            out.append(fmt(float(t)))
            i += 1
        elif cmd == "V":
            out.append(fmt(base - float(t)))
            i += 1
        elif cmd == "A":
            rx, ry, rot, large, sweep, x, y = tokens[i : i + 7]
            out.append(
                f"{fmt(float(rx))} {fmt(float(ry))} {rot} {large} {sweep} "
                f"{fmt(float(x))} {fmt(base - float(y))}"
            )
            i += 7
        else:
            raise ValueError(f"unsupported command {cmd!r}")
    return " ".join(out).replace(" Z", "Z")


def tokenize(path: str) -> list[str]:
    toks, num = [], ""
    for ch in path:
        if ch.isalpha():
            if num:
                toks.append(num)
                num = ""
            toks.append(ch)
        elif ch in " ,":
            if num:
                toks.append(num)
                num = ""
        elif ch == "-" and num and num[-1] not in "eE":
            toks.append(num)
            num = ch
        else:
            num += ch
    if num:
        toks.append(num)
    return toks

def in_rrect(u, v, x, y, w, h, r):
    if not (x <= u <= x + w and y <= v <= y + h):
        return False
    cx = min(max(u, x + r), x + w - r)
    cy = min(max(v, y + r), y + h - r)
    return (u - cx) ** 2 + (v - cy) ** 2 <= r * r


def hex_rgb(h):
    return tuple(int(h[i : i + 2], 16) for i in (1, 3, 5))


def raster(size, units, cover, fg, bg=None, inset=0.0):
    """RGBA rows. `units` is the drawing's grid size, shown across size minus inset on
    each side. bg None means transparent."""
    ss = 8
    scale = units / (size - 2 * inset)
    fr, fgc, fb = hex_rgb(fg)
    out = bytearray()
    for j in range(size):
        out.append(0)
        for i in range(size):
            acc = 0.0
            for sy in range(ss):
                for sx in range(ss):
                    u = (i - inset + (sx + 0.5) / ss) * scale
                    v = (j - inset + (sy + 0.5) / ss) * scale
                    acc += cover(u, v)
            a = acc / (ss * ss)
            if bg is None:
                out += bytes((fr, fgc, fb, round(255 * a)))
            else:
                br, bgc, bb = hex_rgb(bg)
                out += bytes((round(br + (fr - br) * a), round(bgc + (fgc - bgc) * a),
                              round(bb + (fb - bb) * a), 255))
    return bytes(out)


def png(size, rows):
    def chunk(t, b):
        return struct.pack(">I", len(b)) + t + b + struct.pack(">I", zlib.crc32(t + b) & 0xFFFFFFFF)
    ihdr = struct.pack(">IIBBBBB", size, size, 8, 6, 0, 0, 0)
    return (b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", ihdr)
            + chunk(b"IDAT", zlib.compress(rows, 9)) + chunk(b"IEND", b""))


def ico(images):
    """An .ico holding PNG images: [(size, png bytes)]."""
    head = struct.pack("<HHH", 0, 1, len(images))
    offset = 6 + 16 * len(images)
    entries, data = b"", b""
    for size, blob in images:
        entries += struct.pack("<BBBBHHII", size % 256, size % 256, 0, 0, 1, 32, len(blob), offset + len(data))
        data += blob
    return head + entries + data



def luminance(h: str) -> float:
    c = [int(h[i : i + 2], 16) / 255 for i in (1, 3, 5)]
    c = [x / 12.92 if x <= 0.04045 else ((x + 0.055) / 1.055) ** 2.4 for x in c]
    return 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2]


def ratio(a: str, b: str) -> float:
    x, y = sorted((luminance(a), luminance(b)), reverse=True)
    return (x + 0.05) / (y + 0.05)

