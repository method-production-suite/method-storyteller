#!/usr/bin/env python3
"""Draw the app icon and the document icons from the mark, and render every size the
packages need. Runs on a Mac (the pictures are rendered by AppKit through
packaging/icons/render-svg.swift; iconutil builds the .icns files).

    python3 scripts/make-icons.py

Writes to packaging/icons/:

    source/*.svg                     the drawings every picture below is rendered from
    icon-1024.png, -512, -256        the app icon (Linux packages take 256 and 512)
    icon.icns                        macOS, every size from 16 to 1024
    icon.ico                         Windows, 16 to 256
    document-<ext>.icns, .ico, -512.png
                                     the document icons for .osf, .fountain and .fdx

The app icon is the mark in white on a rose rounded square, the script's lines and the
cursor showing the rose through, as Method Budget's and Method Schedule's icons are drawn
from their marks. The square is the mark's 64-unit grid with corners of 14.3 units; from
48 pixels up it sits inside the macOS icon grid's margin (100 of 1024 on each side), and at
16 and 32 pixels it is full bleed and draws the 16-unit favicon drawing, whose whole units
land on whole pixels. A document icon is a white sheet with its corner turned, the mark in
rose, and the file's extension below it in the family's drawn letters. The shapes are
scripts/make-brand.py's, loaded, so the icons cannot drift from the mark.
"""

from __future__ import annotations

import importlib.util
import struct
import subprocess
import sys
import tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent
OUT = ROOT / "packaging" / "icons"
sys.path.insert(0, str(HERE))

import glyphs  # noqa: E402
from glyphs import GAP, W, flip, fmt, ico, rrect  # noqa: E402

spec = importlib.util.spec_from_file_location("make_brand", HERE / "make-brand.py")
brand = importlib.util.module_from_spec(spec)
spec.loader.exec_module(brand)

SQUARE_RADIUS = 14.3 / 64
INSET = 100 / 1024
SMALL = 32  # this size and under draw the 16-unit drawing, full bleed
DOCUMENTS = ["osf", "fountain", "fdx"]
SHEET = "#FFFFFF"
SHEET_RULE = "#DEDBD3"
FOLD = "#EFEDE7"


# Two letters the extensions need that the names never did, drawn by the same rules
# (y-up, x-height 100, ascender 140, stroke 18).
def g_f(x0):
    """The t's stem carried up into a hook that turns right, and the t's crossbar."""
    stem, r = x0 + 22, 26.0
    top = glyphs.ASC - W / 2
    return [f"M{stem} 0V{top - r}A{r} {r} 0 0 1 {stem + r} {top}H{stem + 36}",
            f"M{x0} {100 - W / 2}H{x0 + 54}"], 67.0


def g_x(x0):
    """Two straight strokes crossing at half the x-height."""
    return [f"M{x0 + W / 2} 0L{x0 + 84 - W / 2} 100", f"M{x0 + W / 2} 100L{x0 + 84 - W / 2} 0"], 84.0


glyphs.GLYPHS.update({"f": g_f, "x": g_x})
GAP.update({"os": 10, "sf": 10, "fo": 8, "dx": 10, "fd": 10})


def mark_group(units_box, page, holes, color, units) -> str:
    """The mark drawn into a square box (x, y, side) whose side is `units` grid units."""
    x, y, side = units_box
    k = side / units
    return (f'<g transform="translate({fmt(x)} {fmt(y)}) scale({k:.5f})">'
            f'{brand.mark_paths(page, holes, color)}</g>')


def app_icon_svg(size: int) -> str:
    """The app icon as drawn at `size` pixels."""
    if size <= SMALL:
        units, page, holes, inset = 16, brand.FAV_PAGE, brand.fav_holes(), 0.0
    else:
        units, page, holes, inset = 64, brand.PAGE, brand.holes(), 1024 * INSET
    side = 1024 - 2 * inset
    square = rrect(inset, inset, side, side, side * SQUARE_RADIUS)
    return ('<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1024 1024">'
            f'<path fill="{brand.ACCENT}" d="{square}"/>'
            f'{mark_group((inset, inset, side), page, holes, "#FFFFFF", units)}</svg>\n')


def label_paths(text: str, centre_x: float, base: float, height: float) -> str:
    """The extension in the family's letters, x-height `height`, centred on `centre_x`.
    The i's dot is drawn as a filled disc: AppKit draws the wordmark's stroked 1-unit
    circle as a hairline ring."""
    paths, width = glyphs.set_name(text)
    paths = [p for p in paths if not p.endswith("Z") or "A1 1" not in p]
    dots, x, prev = [], 0.0, ""
    for ch in text:
        x += GAP.get(prev + ch, 13) if prev else 0.0
        if ch == "i":
            dots.append(f'<circle cx="{fmt(x + W / 2)}" cy="-128" r="10"/>')
        x += glyphs.GLYPHS[ch](x)[1]
        prev = ch
    k = height / 100
    d = " ".join(flip(p, 0) for p in paths)
    left = centre_x - width * k / 2
    return (f'<g transform="translate({fmt(left)} {fmt(base)}) scale({k:.4f})">'
            f'<path fill="none" stroke="{brand.INK}" stroke-width="{fmt(W)}" d="{d}"/>'
            f'<g fill="{brand.INK}">{"".join(dots)}</g></g>')


def document_svg(ext: str, small: bool) -> str:
    """A sheet with its top right corner turned, the mark, and the extension."""
    left, top, width, height, fold = 212, 92, 600, 840, 150
    right, bottom = left + width, top + height
    sheet = (f"M{left + 24} {top}H{right - fold}L{right} {top + fold}V{bottom - 24}"
             f"a24 24 0 0 1-24 24H{left + 24}a24 24 0 0 1-24-24V{top + 24}a24 24 0 0 1 24-24Z")
    corner = f"M{right - fold} {top}V{top + fold - 20}a20 20 0 0 0 20 20H{right}Z"
    out = ['<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1024 1024">',
           f'<path fill="{SHEET}" stroke="{SHEET_RULE}" stroke-width="12" stroke-linejoin="round" d="{sheet}"/>',
           f'<path fill="{FOLD}" stroke="{SHEET_RULE}" stroke-width="12" stroke-linejoin="round" d="{corner}"/>']
    if small:
        # Below 64 pixels the letters cannot be read: the mark alone, larger.
        out.append(mark_group((272, 262, 480), brand.FAV_PAGE, brand.fav_holes(), brand.ACCENT, 16))
    else:
        out.append(mark_group((322, 232, 380), brand.PAGE, brand.holes(), brand.ACCENT, 64))
        longest = max(glyphs.set_name(e)[1] for e in DOCUMENTS)
        out.append(label_paths(ext, left + width / 2, 820, min(110, (width - 120) / longest * 100)))
    out.append("</svg>\n")
    return "".join(out)


def render(jobs: list[tuple[str, Path, int]]) -> None:
    """Render (svg text, png path, pixels) through AppKit in one run."""
    with tempfile.TemporaryDirectory() as tmp:
        args = []
        for n, (svg, png_path, px) in enumerate(jobs):
            src = Path(tmp) / f"{n}.svg"
            src.write_text(svg)
            args += [str(src), str(png_path), str(px)]
        subprocess.run(["swift", str(OUT / "render-svg.swift"), *args], check=True)


def icns(pngs: dict[str, bytes]) -> bytes:
    body = b""
    for kind, data in pngs.items():
        body += kind.encode() + struct.pack(">I", len(data) + 8) + data
    return b"icns" + struct.pack(">I", len(body) + 8) + body


# The .icns slots: (type, pixels).
ICNS = [("icp4", 16), ("icp5", 32), ("ic11", 32), ("ic12", 64), ("ic07", 128), ("ic13", 256),
        ("ic08", 256), ("ic14", 512), ("ic09", 512), ("ic10", 1024)]
ICO = [16, 24, 32, 48, 64, 256]


def main() -> int:
    source = OUT / "source"
    source.mkdir(parents=True, exist_ok=True)
    (source / "app-icon.svg").write_text(app_icon_svg(1024))
    (source / "app-icon-small.svg").write_text(app_icon_svg(16))
    for ext in DOCUMENTS:
        (source / f"document-{ext}.svg").write_text(document_svg(ext, small=False))
        (source / f"document-{ext}-small.svg").write_text(document_svg(ext, small=True))

    sizes = sorted({px for _, px in ICNS} | set(ICO))
    with tempfile.TemporaryDirectory() as tmp:
        tmp = Path(tmp)
        jobs = [(app_icon_svg(px), tmp / f"app-{px}.png", px) for px in sizes]
        for ext in DOCUMENTS:
            jobs += [(document_svg(ext, px < 64), tmp / f"{ext}-{px}.png", px) for px in sizes]
        render(jobs)

        def read(name: str) -> bytes:
            return (tmp / name).read_bytes()

        files: dict[str, bytes] = {
            "icon-1024.png": read("app-1024.png"),
            "icon-512.png": read("app-512.png"),
            "icon-256.png": read("app-256.png"),
            "icon.icns": icns({kind: read(f"app-{px}.png") for kind, px in ICNS}),
            "icon.ico": ico([(px, read(f"app-{px}.png")) for px in ICO]),
        }
        for ext in DOCUMENTS:
            files[f"document-{ext}-512.png"] = read(f"{ext}-512.png")
            files[f"document-{ext}.icns"] = icns({kind: read(f"{ext}-{px}.png") for kind, px in ICNS})
            files[f"document-{ext}.ico"] = ico([(px, read(f"{ext}-{px}.png")) for px in ICO])
    for name, data in files.items():
        (OUT / name).write_bytes(data)
        print(f"wrote packaging/icons/{name} ({len(data)} bytes)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
