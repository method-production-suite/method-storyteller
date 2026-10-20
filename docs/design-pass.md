# The design pass: what is done and what is left

Roadmap item D: match Method Storyteller to the family (Method Budget's and Method Schedule's
BRAND.md), with every step pictured offscreen in light and dark. [BRAND.md](BRAND.md) holds
the results; this page is the plan for the rest. Nothing here needs the app on anyone's screen:
every picture is drawn by the offscreen gallery (`gallery` in `crates/app/src/screens.rs`).

## Done

| Step | Where it is |
| --- | --- |
| The final mark and wordmark (decision 10) | `docs/brand/`, drawn by `scripts/make-brand.py` |
| The app icon at every size, and document icons for `.osf`, `.fountain`, `.fdx` and `.mstory` | `packaging/icons/`, drawn by `scripts/make-icons.py` |
| Colour, type and spacing tokens | `crates/app/src/theme.rs`, listed in BRAND.md |
| Contrast for every pair, held by a test | `theme::tests::every_pair_meets_wcag_aa` |
| Dark mode, surface by surface | BRAND.md, "Dark mode, surface by surface" |
| Empty and error states in the family voice | `crates/app/src/messages.rs`, with tests |
| A gallery of 13 surfaces in both themes | `gallery`, writing `target/gallery/` |

## Left to do

### 1. The gallery catches up with this week

These surfaces were built after the gallery and are not in it yet. Each becomes a named shot,
in light and dark:

| Shot | What it shows |
| --- | --- |
| `go-to-act` | Go To with the Act tab chosen |
| `new-tv` | New TV Script with its three shapes |
| `navigator-tv` | A TV script's navigator: act rows above their scenes |
| `navigator-scenes` | Scene titles in place of headings, colour stripes, a hover with a summary |
| `scene-properties` | The Scene Properties dialog with a colour chosen |
| `characters` | Production > Characters with a few rows |
| `recovery` | Recover unsaved changes? |
| `styled-text` | Bold, italic and underline on the page, with the markup faint |
| `multi-cam-page` | A multi-camera page: lettered scene, cast line, double-spaced dialogue |
| `open-recent` | The window menu's File > Open Recent (Windows and Linux) |
| `spelling` | Edit > Spelling… with suggestions |
| `picker` | The element picker on an empty line |

### 2. Colours the app names itself

BRAND.md says nothing in the app names a colour of its own. Two places do today:

- **Scene colours.** The eight colours of Scene Properties are written in `app.rs`. They move
  to `theme.rs` as `SCENE_RED` to `SCENE_PINK`, each checked as a 3:1 stripe against panel in
  both themes (a stripe carries meaning, so it needs a border's contrast). A colour that fails
  in dark mode gets a dark-mode value, as the accent has; the value saved in the file stays the
  light one.
- **The act row.** It draws in the strong text colour by hand. It becomes the `ink-soft`
  token, the one section labels use.

### 3. The contact sheet

One picture for the owner to look over: every gallery shot, light beside dark, in a grid,
each labelled with its name (`target/gallery/contact-sheet.png`). It is the "design pass (D)"
check in the roadmap's Waiting on the owner, so looking at it takes minutes and opens nothing.

### 4. BRAND.md catches up

- The new surfaces join "Dark mode, surface by surface".
- The scene colours join the colour table and the contrast table.
- "Document icons" lists `.mstory` (done in this change).

## Order

1. The scene colour and act row tokens (section 2), with their contrast tests. A code change:
   built and tested on power.
2. The new gallery shots and the contact sheet (sections 1 and 3). A code change to the
   gallery test only.
3. BRAND.md's tables (section 4), with the measured numbers from step 1.
4. The owner looks at the contact sheet; anything that feels off comes back as a small PR.

Steps 1 and 2 need a build, so they wait for full power. Step 3 waits for their numbers.
