# Method Storyteller brand

How Method Storyteller looks and sounds, in the app, in the README and anywhere else it
shows up. It belongs to the same family as the other Method apps: the same voice, the same
neutrals and type, the mark and name drawn the same way, and a rose accent that is its own.
The mark is provisional for now.

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

## The mark

A screenplay page. The page is at full strength, with two brads down its left edge. The
script's shape is at half strength: a scene heading, action, a cue and its dialogue. A
text cursor sits after the last line at full strength, because this is the app you write
in. The mark uses the family's box: 40 by 52 units on a 64-unit grid. A separate 16-unit
drawing is used for favicons.

| File | Use |
|---|---|
| `docs/brand/mark.svg` | The mark, rose, for light backgrounds |
| `docs/brand/mark-dark.svg` | The mark, light rose, for dark backgrounds |
| `docs/brand/wordmark.svg` | Mark and name, for light backgrounds |
| `docs/brand/wordmark-dark.svg` | Mark and name, for dark backgrounds |
| `docs/brand/favicon.svg` | The 16-unit mark; switches color with the browser's scheme |
| `docs/brand/favicon.ico` | The 16-unit mark at 16 and 32 pixels |
| `docs/brand/apple-touch-icon.png` | The touch icon |

While the mark is provisional, these files are drawn by the suite website's
`tools/make-brand.py`. Change that script and copy the files here; don't edit them by
hand. Once Storyteller has a site of its own, the script moves into this repository.

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

## Color

The accent is rose, set in [crates/app/src/theme.rs](../crates/app/src/theme.rs). The
app uses it for selection, focus, links and the mark.

| Token | Light | Dark | Use |
|---|---|---|---|
| accent | `#AE3D56` | `#EE8DA2` | Selection, focus, links, the mark |
| panel | `#F6F6F7` | `#1E1E20` | Side panels, toolbar |
| window | `#FFFFFF` | `#2A2A2D` | Dialogs, popups |
| desk | `#E4E4E7` | `#262629` | The surface the pages sit on |

The page itself is always white with black type, as a printed script is.

### Contrast (WCAG 2.x)

Body text needs a contrast ratio of 4.5:1 (AA). Accent text meets that on every surface
it is drawn on.

| Pair | Ratio |
|---|---|
| accent on window (light) | 5.83 |
| accent on panel (light) | 5.40 |
| accent on desk (light) | 4.60 |
| white on accent | 5.83 |
| accent on panel (dark) | 7.11 |
| accent on desk (dark) | 6.44 |
| accent on window (dark) | 6.11 |

## Type

- The interface uses the system's own faces, so nothing is downloaded.
- The script is set in Courier Prime 12 pt (SIL Open Font License, bundled in
  `assets/fonts`), because page count depends on it.
- The wordmark is artwork, so it needs no font.

## Imagery

- No stock photos, and no other product's screens, logos or text.
- Screenshots are taken from invented scripts only, never a real one.
