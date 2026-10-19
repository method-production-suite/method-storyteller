#!/usr/bin/env python3
"""Draw Method Storyteller's mark, wordmark and favicons, and check them. Standard library
only.

    python3 scripts/make-brand.py           write the files below
    python3 scripts/make-brand.py --check   change nothing; exit 1 if a file is out of date

Writes the same files to docs/brand/ (the README) and site/ (methodstoryteller.com, which
the suite website also reads for its suite bar):

    mark.svg, mark-dark.svg          the mark, for light and dark backgrounds
    wordmark.svg, wordmark-dark.svg  mark and name
    favicon.svg                      the 16-unit drawing; switches with the browser's scheme
    favicon.ico                      the 16-unit drawing at 16 and 32 pixels
    apple-touch-icon.png             180 pixels, the white mark on a rose square

The mark is a screenplay page. The page is solid; two brad holes are punched down its left
edge, and the script's shape is cut out of it: a scene heading, action, a cue and its
dialogue, each at its own indent, as a page prints. After the last line a text cursor
is cut out too, taller than the lines, because this is the app you write in. Everything
that is not page is empty, so the mark sits on any background, as Method Budget's page
with its figures cut out does. Same box as the family: 40 by 52 units on a 64-unit grid.
A separate 16-unit drawing on whole units is used for favicons and the smallest icons.

The letters are scripts/glyphs.py, the family's strokes (copied unchanged from the suite
website's tools/glyphs.py). The app icon and document icons are drawn from these shapes
by scripts/make-icons.py, which loads this file. Rules are in docs/BRAND.md.
"""

from __future__ import annotations

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

from glyphs import (DESC_END, W, flip, fmt, ico, in_rrect, png, raster, rrect,  # noqa: E402
                    set_name)

ROOT = Path(__file__).resolve().parent.parent
ACCENT, ACCENT_DARK = "#AE3D56", "#EE8DA2"
INK, INK_DARK = "#1C2228", "#E4E8EC"

# A shape is (x, y, w, h, r). The mark is the page minus its holes.
PAGE = (12, 6, 40, 52, 3)


def holes():
    """What is cut out of the page, on the 64-unit grid."""
    brads = [(17, 12.5, 6, 6, 3), (17, 45.5, 6, 6, 3)]
    lines = [
        (27, 13, 19, 3.5),  # scene heading
        (27, 20, 19, 3.5),  # action
        (27, 26, 13, 3.5),
        (34, 32.5, 8, 3.5),  # cue
        (30, 38.5, 15, 3.5),  # dialogue
        (30, 44.5, 6, 3.5),
    ]
    cursor = [(38.5, 43.5, 2.5, 6, 0.75)]
    return brads + [(x, y, w, h, h / 2) for x, y, w, h in lines] + cursor


FAV_PAGE = (3, 1, 10, 14, 1)


def fav_holes():
    """The 16-unit drawing: whole units, so it stays sharp at 16 and 32 pixels."""
    return [
        (4, 2, 1, 1, 0.5), (4, 13, 1, 1, 0.5),  # brads
        (6, 3, 6, 1, 0.5),  # scene heading
        (6, 5, 6, 1, 0.5), (6, 7, 4, 1, 0.5),  # action
        (8, 9, 3, 1, 0.5),  # cue
        (7, 11, 3, 1, 0.5),  # dialogue
        (11, 11, 1, 2, 0.25),  # cursor
    ]


def mark_paths(page, cut, color: str | None) -> str:
    fill = f' fill="{color}"' if color else ""
    d = rrect(*page) + "".join(rrect(*h) for h in cut)
    return f'<path{fill} fill-rule="evenodd" d="{d}"/>'


def cover_of(page, cut):
    def cover(u, v):
        if not in_rrect(u, v, *page):
            return 0.0
        return 0.0 if any(in_rrect(u, v, *h) for h in cut) else 1.0
    return cover


HEADER = "<!-- {what}. Drawn by scripts/make-brand.py; edit that, not this file. -->"
LABEL = "Method Storyteller"


def mark_file(color: str) -> str:
    return ('<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64" role="img"'
            f' aria-label="{LABEL}">\n{HEADER.format(what="Method Storyteller mark")}\n<title>{LABEL}</title>\n'
            f'{mark_paths(PAGE, holes(), color)}\n</svg>\n')


def wordmark_file(color: str, ink: str) -> str:
    """The mark as tall as the ascenders, then the name, on one baseline (base at y 150,
    ascenders at y 10). The y's descender makes the box taller, as Method Budget's g does."""
    text = "method storyteller"
    paths, width = set_name(text)
    base = 150.0
    d = " ".join(flip(p, base) for p in paths)
    k = (base - 10) / 52
    tx, ty = -12 * k, 10 - 6 * k
    name_x = 40 * k + 40
    height = base - DESC_END + 10
    return (f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {fmt(name_x + width)} {fmt(height)}"'
            f' role="img" aria-label="{LABEL}">\n{HEADER.format(what="Method Storyteller wordmark")}\n<title>{LABEL}</title>\n'
            f'<g transform="translate({fmt(tx)} {fmt(ty)}) scale({k:.4f})">{mark_paths(PAGE, holes(), color)}</g>\n'
            f'<path transform="translate({fmt(name_x)} 0)" fill="none" stroke="{ink}"'
            f' stroke-width="{fmt(W)}" d="{d}"/>\n</svg>\n')


def favicon_file() -> str:
    light = mark_paths(FAV_PAGE, fav_holes(), ACCENT)
    dark = mark_paths(FAV_PAGE, fav_holes(), ACCENT_DARK)
    return ('<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16">\n'
            f'{HEADER.format(what="Method Storyteller favicon")}\n'
            '<style>.d{display:none}@media (prefers-color-scheme:dark){.l{display:none}.d{display:inline}}</style>\n'
            f'<g class="l">{light}</g><g class="d">{dark}</g>\n</svg>\n')


def outputs() -> dict[Path, str | bytes]:
    fav = cover_of(FAV_PAGE, fav_holes())
    files: dict[str, str | bytes] = {
        "mark.svg": mark_file(ACCENT),
        "mark-dark.svg": mark_file(ACCENT_DARK),
        "wordmark.svg": wordmark_file(ACCENT, INK),
        "wordmark-dark.svg": wordmark_file(ACCENT_DARK, INK_DARK),
        "favicon.svg": favicon_file(),
        "favicon.ico": ico([(n, png(n, raster(n, 16, fav, ACCENT))) for n in (16, 32)]),
        "apple-touch-icon.png": png(180, raster(180, 64, cover_of(PAGE, holes()), "#FFFFFF", bg=ACCENT, inset=14)),
    }
    out: dict[Path, str | bytes] = {}
    for name, data in files.items():
        out[ROOT / "docs" / "brand" / name] = data
        site = ROOT / "site" / ("assets/brand/" + name if name.startswith(("mark", "wordmark")) else name)
        out[site] = data
    return out


def main(argv: list[str]) -> int:
    check = "--check" in argv
    stale = []
    for path, data in outputs().items():
        blob = data.encode() if isinstance(data, str) else data
        if path.exists() and path.read_bytes() == blob:
            continue
        stale.append(path)
        if not check:
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(blob)
    for path in stale:
        print(("out of date: " if check else "wrote ") + str(path.relative_to(ROOT)))
    return 1 if check and stale else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
