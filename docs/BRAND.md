# Method Storyteller brand

How Method Storyteller looks and sounds, in the app, in the README and anywhere else it
shows up. It belongs to the same family as the other Method apps: the same voice, the same
neutrals and type, the mark and name drawn the same way, and a rose accent that is its own.

## The name

- Always write **Method Storyteller**: two words, both capitalized, in running text.
- Never shorten it to initials. Initials are ambiguous, and a name exists so the product
  isn't mistaken for anything else. On second mention, "the app" or "it" is fine.
- The command is `method-storyteller`, in code style (`mst` still works as a deprecated alias). The
  desktop app's executable is `method-storyteller-app`. Scripts are ordinary `.fountain`,
  `.osf`, `.fdx` and `.fadein` files, with no format of our own.
- The wordmark is set in lowercase (`method storyteller`). That's artwork, not spelling:
  in text, write Method Storyteller.
- Credit "the Method Storyteller contributors". The copyright line is "Method Production
  Suite Team and the Method Storyteller contributors". No personal, company or studio
  names go in public material: not in pages, alt text, metadata or file names.

## Voice

Plain, confident, specific. Write for someone who writes screenplays, not for someone who
reads the code.

- Say what a person can do, in their words: "Tab after a character name starts the
  dialogue", not "element state machine".
- Claim only what is built. Label planned work as planned ("coming", "on the roadmap").
- No hype: no "revolutionary", "powerful", "seamless", "blazing". A number beats an
  adjective ("every scene starts on the same page as the original on 15 of 15 test
  scripts").
- No em dashes anywhere. Use a colon, a comma, parentheses or a new sentence.
- No exclamation marks.
- Other products get one factual line saying Method Storyteller is not affiliated, in the
  README's clean-room note. Never a comparison, a dig, a screenshot or a logo. Naming a
  file format (`.fdx`, `.fadein`) is fine.
- Examples use invented scripts and invented people only.

### Empty and error states

Say what happened and what to do next, with the file's name; never blame the writer, never
apologize at length. The words live in
[crates/app/src/messages.rs](../crates/app/src/messages.rs), whose tests hold them to this
voice.

| State | What it says |
|---|---|
| No script (a new, empty one) | On the first line: "Scene Heading: start with INT. or EXT." |
| No scenes | "No scenes yet. Start a line with INT. or EXT. and it appears here as a scene." |
| Nothing found | "No matches for “lighthouse”" |
| A file that will not open | "“Field Station.fdx” did not open. The file is not there any more; it may have been moved or renamed." (or: not allowed to read it; a PDF is a printed script; too large; not a script it can read) |
| A failed save | "“Field Station.fountain” was not saved. The folder does not allow changes. Your changes are still here: use Save As to save them somewhere else." |
| A failed update check | "Could not check for updates (offline). Help > Check for Updates tries again." |
| An update that would not install | "The update did not install. It is still ready: Restart to Update tries again." |

Errors show in the status bar in the error color; empty states are a short title and one
muted line, centered where the content would be.

## The mark

A screenplay page. The page is solid; two brad holes are punched down its left edge, and
the script's shape is cut out of it: a scene heading, action, a cue and its dialogue, each
at its own indent, as a page prints. After the last line a text cursor is cut out too,
taller than the lines, because this is the app you write in. Everything that is not page
is empty, so the mark sits on any background, as Method Budget's page with its figures cut
out does. The mark uses the family's box: 40 by 52 units on a 64-unit grid. A separate
16-unit drawing on whole units (five lines, two brads, the cursor) is used for favicons and
the smallest icons, so it stays sharp at 16 and 32 pixels.

| File | Use |
|---|---|
| `docs/brand/mark.svg` | The mark, rose, for light backgrounds |
| `docs/brand/mark-dark.svg` | The mark, light rose, for dark backgrounds |
| `docs/brand/wordmark.svg` | Mark and name, for light backgrounds |
| `docs/brand/wordmark-dark.svg` | Mark and name, for dark backgrounds |
| `docs/brand/favicon.svg` | The 16-unit mark; switches color with the browser's scheme |
| `docs/brand/favicon.ico` | The 16-unit mark at 16 and 32 pixels |
| `docs/brand/apple-touch-icon.png` | 180 pixels, the white mark on a rose square |
| `packaging/icons/` | The app icon and the document icons (below) |

The same files go to `site/` (`site/assets/brand/` for the mark and wordmark,
`site/favicon.*` and `site/apple-touch-icon.png`), which methodstoryteller.com serves and
the suite website reads for its suite bar. All of them are drawn by
[scripts/make-brand.py](../scripts/make-brand.py); don't edit them by hand. Run
`python3 scripts/make-brand.py` after changing it (`--check` changes nothing and exits 1
when a file is out of date). The letters are `scripts/glyphs.py`, the family's strokes,
copied unchanged from the suite website.

Rules:

- Leave clear space around the mark of at least a quarter of its height.
- Don't recolor it outside the palette, outline it, add effects, rotate it, or put
  anything inside the page.
- Smallest sizes: the mark at 16 pixels (use the favicon drawing below 24), and the
  wordmark at 18 pixels tall.
- Use the `-dark` files on dark backgrounds. In HTML, a `<picture>` with a
  `prefers-color-scheme: dark` source does this, and the README does.

### The wordmark

`method storyteller` in lowercase, drawn from the family's strokes rather than typed in a
font. Because it's drawn, it looks the same everywhere and ships no font. The name is ink
and the mark is rose.


### The app icon

The mark in white on a rose (`#AE3D56`) rounded square, the script's lines and the cursor
showing the rose through, as Method Budget's and Method Schedule's icons are drawn from
their marks. The square is the mark's 64-unit grid with corners of 14.3 units. From 48
pixels up it sits inside the macOS icon grid's margin (100 of 1024 a side); at 16 and 32
pixels it is full bleed and draws the 16-unit drawing.

### Document icons

A `.osf`, `.fountain` or `.fdx` file gets a white sheet with its top corner turned, the
mark in rose, and the extension below it in the family's drawn letters (`f` and `x` are
new for this and follow the same rules). Below 64 pixels the letters can't be read, so the
sheet carries the 16-unit mark alone.

`python3 scripts/make-icons.py` draws both from the mark's shapes (it loads
`make-brand.py`, so the two can't drift), keeps the drawings in `packaging/icons/source/`,
and renders every size on a Mac: `icon.icns` (16 to 1024), `icon.ico` (16 to 256),
`icon-1024.png`, `icon-512.png` and `icon-256.png` for Linux, and for each document type
`document-<ext>.icns`, `.ico` and `-512.png`.
## Color

The neutrals, the amber and the token names are the family's, value for value with Method
Budget and Method Schedule: warm paper and ink on the light side, the same dark surfaces on
the other. What is ours is the rose accent, in the same lightness band as the family's other
accents. Every token is a named constant in
[crates/app/src/theme.rs](../crates/app/src/theme.rs); nothing in the app names a color of
its own.

| Token | Light | Dark | Use |
|---|---|---|---|
| desk | `#E8E5DD` | `#111417` | The surface the pages sit on |
| panel | `#F7F6F2` | `#1B1F24` | Top bar, navigator, find bar, status bar |
| window | `#FFFFFF` | `#22272E` | Dialogs, Help windows, menus, popups |
| field | `#FFFFFF` | `#111417` | Text fields |
| control | `#EFEDE7` | `#2B3138` | Buttons at rest |
| control-hover | `#E6E3DB` | `#353C45` | Buttons under the pointer |
| ink | `#1C2228` | `#E4E8EC` | Text |
| ink-soft | `#3A424C` | `#C3CAD2` | Secondary text |
| muted | `#59626D` | `#9AA3AD` | Captions, hints, counts |
| rule | `#DEDBD3` | `#2B3138` | Dividers (decorative) |
| rule-strong | `#8F897C` | `#67717D` | Borders that carry meaning |
| accent | `#AE3D56` | `#EE8DA2` | Focus, links, the mark |
| accent-strong | `#8C2F44` | `#F6B8C6` | Text of a chosen toggle, selection edge |
| accent-ink | `#FFFFFF` | `#2A0E16` | Text on an accent fill |
| accent-soft | `#F6E3E8` | `#47222D` | Selection, the current scene |
| note | `#8A5A00` | `#E3B341` | Amber: caution, notes (never an accent) |
| error | `#B3261E` | `#FF9C9C` | A file that will not open, a failed save |

The page is the same in both themes, white with black type, as a printed script is:

| Token | Value | Use |
|---|---|---|
| page | `#FFFFFF` | The paper |
| page-ink | `#1A1A1A` | The script |
| page-muted | `#707070` | Page and scene numbers, an empty line's element name |
| page-rule | `#C4C4C4` | The dashed rule at a page end in the draft view (decorative) |
| find-mark | `#EBBCC8` | A find match: the accent as a light wash |
| page-selection | `#F5D9E0` | Selected text: a lighter wash, so a find match inside it shows |

### Contrast (WCAG 2.x)

Body text needs 4.5:1 (AA); borders that carry meaning need 3:1. The test
`theme::tests::every_pair_meets_wcag_aa` fails when a pair drops below what it needs, and
`cargo test -p method-storyteller contrast_table -- --nocapture` prints this table.

| Pair | Light | Dark | Needs |
|---|---|---|---|
| ink on panel | 14.84 | 13.45 | 4.5 |
| ink on window | 16.04 | 12.20 | 4.5 |
| ink on control | 13.71 | 10.66 | 4.5 |
| ink on control-hover | 12.51 | 9.05 | 4.5 |
| ink on accent-soft | 13.04 | 11.11 | 4.5 |
| ink-soft on panel | 9.41 | 10.01 | 4.5 |
| muted on panel | 5.72 | 6.48 | 4.5 |
| muted on window | 6.19 | 5.88 | 4.5 |
| muted on desk | 4.92 | 7.23 | 4.5 |
| muted on accent-soft | 5.03 | 5.35 | 4.5 |
| accent on panel | 5.39 | 7.07 | 4.5 |
| accent on window | 5.83 | 6.42 | 4.5 |
| accent on desk | 4.63 | 7.89 | 4.5 |
| accent-strong on accent-soft | 6.55 | 8.20 | 4.5 |
| accent-ink on accent | 5.83 | 7.65 | 4.5 |
| note on window | 5.93 | 7.72 | 4.5 |
| error on window | 6.54 | 7.50 | 4.5 |
| error on panel | 6.04 | 8.27 | 4.5 |
| rule-strong on panel | 3.22 | 3.34 | 3 |
| page ink on page | 17.40 | 17.40 | 4.5 |
| page muted on page | 4.95 | 4.95 | 4.5 |
| page ink on find mark | 10.41 | 10.41 | 4.5 |
| page ink on selection | 13.16 | 13.16 | 4.5 |
| accent on page | 5.83 | 5.83 | 4.5 |
| selected text on selection | 6.10 | 6.10 | 4.5 |

### Dark mode, surface by surface

The app follows the system's light or dark setting. Each surface was checked in both, in the
offscreen gallery (`cargo test -p method-storyteller --release gallery -- --ignored` writes
`target/gallery/<surface>-<theme>.png`):

| Surface | Dark mode |
|---|---|
| Desk | The family's darkest surface, so the white page stands out without a glow |
| Page | White paper and black type in both themes; no scroll fade across it (it read as a smudge) |
| Caret and selection on the page | The light accent in both themes (page-selection `#F5D9E0`, caret `#AE3D56`), never the dark-theme wash |
| Top bar, navigator, status bar | panel; the current scene in accent-soft with ink |
| Find bar | panel; the fields in field, a match on the page in find-mark |
| Dialogs and Help windows | window, opened in the middle of the app's window |
| Save changes? | window with a 16-point margin, over a dimmed app |
| Menus, popups, tooltips | window |

## Type

- The interface uses the system's own faces, so nothing is downloaded. The scale, in points:

  | Token | Size | Use |
  |---|---|---|
  | small | 11 | Section labels, counts, fine print |
  | body | 13 | Labels, buttons, lists (the family's 13) |
  | title | 13.5 | The script's name in the top bar |
  | heading | 17 | Headings in dialogs and Help windows |
  | mono | 12 | Code and plain text |

- The script is set in Courier Prime 12 pt (SIL Open Font License, bundled in
  `assets/fonts`), because page count depends on it.
- The wordmark is artwork, so it needs no font.

## Spacing and shape

Spacing comes in steps of 4 points: 4, 8, 12 and 16. Buttons pad 8 by 3; items sit 8 apart
across and 4 down; windows have a 16-point margin. The top bar is 38 points tall (as Method
Schedule's is), a navigator row 24. Corners: 6 on controls, 8 on menus, 10 on windows.

## Imagery

- No stock photos, and no other product's screens, logos or text.
- Screenshots are taken from invented scripts only, never a real one.
