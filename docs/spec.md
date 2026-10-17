# Method Storyteller — product spec

Method Storyteller is an open-source screenwriting app: a pure-Rust alternative to Final Draft.
It runs natively on macOS, Windows and Linux. Licence: MIT OR Apache-2.0.

This spec covers the MVP and names later phases so the model doesn't paint itself into a corner.

## 1. Who it's for and what "done" feels like

A screenwriter opens the app, starts typing, and never thinks about formatting. Tab and Enter move
between elements the way Final Draft users expect. The page count matches what a reader or
producer would get from Final Draft for the same script (within a line or two per page). The
writer can save to Fountain or FDX, export a PDF, and open those files again without losing anything.

## 2. Principles (inherited from the Craft suite)

1. **Never crash.** No `unwrap`/`expect`/`panic!`/`unsafe` outside tests. Every file is hostile
   input: size caps, bounded loops, `get()` instead of indexing. A save never truncates the
   previous file (`craft-atomic`).
2. **Everything is a command.** Every action has an id, label, menu path, shortcut, typed params,
   `enabled()` and `run()`. Menus, keyboard, CLI and the control channel all dispatch by id.
3. **Everything is agent-drivable.** Every interactive widget has a stable automation id. A
   JSON-lines control channel (`--control <port>`, 127.0.0.1 only) can list, click, type into
   and screenshot the UI, run commands and inspect the document. A headless mode runs the
   same control channel against the engine without a window.
4. **Clean-room.** FDX support is built from the publicly known XML structure and from files we
   create ourselves. We never read Final Draft binaries or bundled templates. Fountain follows
   the open spec at fountain.io. No GPL code.
5. **Open formats, the writer's choice.** Fountain (plain text: diffable, future-proof, readable anywhere) is the default working format; Open Screenplay Format (structured XML that keeps more production detail) is the other. Both are open and documented, and either can be the default for new scripts.

## 3. Script elements

| Element | Notes |
| --- | --- |
| Scene heading | `INT.`/`EXT.`/`EST.`/`INT./EXT.`/`I/E` prefixes; optional scene number |
| Action | Paragraphs; may contain hard line breaks |
| Character | Cue, with extensions such as `(V.O.)`, `(O.S.)`, `(CONT'D)`; dual-dialogue flag |
| Parenthetical | Inside a dialogue block |
| Dialogue | Inside a dialogue block |
| Transition | `CUT TO:`, `FADE OUT.` etc.; right-aligned |
| Shot | `CLOSE ON`, `ANGLE ON` etc. (FDX "Shot"; Fountain writes it as forced action-style heading) |
| Centered | Fountain `>TEXT<` |
| Lyrics | Fountain `~` |
| Section / Synopsis | Fountain `#` / `=`; outline only, not printed |
| Page break | Fountain `===`; forced break |

Inline emphasis (`*italic*`, `**bold**`, `_underline_`) and notes (`[[ ]]`) are stored as Fountain
markup inside element text. The PDF and FDX writers turn emphasis into styled runs; notes are not
printed.

## 4. Editing (MVP)

### 4.1 Element flow (Final Draft–style)

Corrected in October 2026 from the V1 research ([docs/research/v1-feature-inventory.md](research/v1-feature-inventory.md)):
Enter after Dialogue, Enter on an empty element and Tab in Character changed. The code follows in
milestone M3.

Enter ends the current element and starts the next one:

| After | Enter starts |
| --- | --- |
| Scene heading | Action |
| Action | Action |
| Character | Dialogue |
| Parenthetical | Dialogue |
| Dialogue | Action |
| Transition | Scene heading |
| Shot | Action |

Enter on an *empty* element opens an element picker under the caret, with a one-letter key per
element (S Scene heading, A Action, C Character, P Parenthetical, D Dialogue, T Transition,
H Shot). Esc keeps the element as it is.

Tab changes the current element:

| In | Tab gives | Shift-Tab gives |
| --- | --- | --- |
| Action | Character | Scene heading |
| Character | Transition, only when the cue is empty (with text in the cue, Tab is to be confirmed) | Action |
| Dialogue | Parenthetical | Character |
| Parenthetical | Dialogue | Dialogue |
| Scene heading | Action | Action |
| Transition | Scene heading | Action |
| Shot | Action | Action |

Enter in the middle of an element splits it at the caret (the second half takes the "next"
element type). Backspace at the start of an element merges it into the previous one.
Up/Down at the first/last line move between elements. ⌘1–⌘7 (Ctrl on Windows/Linux) set the
element type directly: 1 Scene heading, 2 Action, 3 Character, 4 Parenthetical, 5 Dialogue,
6 Transition, 7 Shot. Parentheticals get their brackets added automatically.

Scene headings, character cues and transitions display and save in capitals.

All tables live in one data structure in the engine (`flow.rs`) so they can become a user
preference later.

### 4.2 Autocomplete

- **Character:** names already used in the script, most frequent first; extensions offered after
  `(`: `V.O.`, `O.S.`, `O.C.`, `CONT'D`.
- **Scene heading:** prefix (`INT.`, `EXT.`, `INT./EXT.`, `EST.`), then locations already used,
  then time of day after ` - ` (`DAY`, `NIGHT`, `MORNING`, `EVENING`, `CONTINUOUS`, `LATER`,
  `MOMENTS LATER`, plus any used).
- **Transition:** `CUT TO:`, `FADE IN:`, `FADE OUT.`, `DISSOLVE TO:`, `SMASH CUT TO:`,
  `MATCH CUT TO:`, plus any used.

A popup shows matches under the caret; Up/Down choose, Enter or Tab accept, Esc dismisses.

### 4.3 Undo

Unlimited-ish (capped at 500 steps) snapshot undo; consecutive typing in one element merges into
one step.

### 4.4 Navigation and scene numbers

- **Scene navigator** side panel: every scene heading with its number and page; click to jump.
- **Scene numbers:** `Production → Number scenes` assigns 1, 2, 3…; existing numbers are kept
  when "keep existing" is chosen (A-numbering for inserted scenes is a later-phase revision
  feature). Toggle for showing them in the editor and PDF. Stored in Fountain as `#12#`, in FDX
  as `Number="12"`.

### 4.5 Title page

Fields: Title, Credit, Author(s), Source, Draft date, Contact, Copyright, Notes. Edited in a
dialog; stored as Fountain title-page keys. Printed on its own unnumbered page.

## 5. Pagination (industry standard)

The page model is the product's heart; it is pure, deterministic and golden-tested.

These rules come from settings Final Draft writes into its own files (105 of 109 complete saves
in the local corpus agree), its knowledge base, and measurements of its exported PDFs. They were
calibrated in milestone M2 until every scene of 15 ground-truth scripts (1,092 scenes) starts on
Final Draft's page ([docs/research/pagination-calibration.md](research/pagination-calibration.md)).
`cargo xtask corpus` re-measures it. An imported `.fdx` brings its own page layout, element
settings and per-paragraph overrides, and these replace the defaults below.

- **Page:** US Letter 8.5" × 11", A4 in v1. Courier Prime 12 pt, 6 lines per inch, 1 line = 12 pt.
  Indents sit on a 10-per-inch grid, but text advances **7.04 pt per character** (Final Draft fits
  61 characters on a 6.00" line and 35 on a 3.50" one). Characters per line are
  `floor(width × 72 / 7.04)`, and the PDF draws text at that advance.
- **Body:** 1.0" top and bottom margins, so **54 lines** per page (Final Draft's "Normal" line
  spacing; Loose, Tight and Very Tight presets come later). Page numbers `2.` etc. sit at the top
  right, 0.5" from the top, ending at 7.5"; page 1 is unnumbered. (The right edge is not yet
  confirmed: Final Draft's header data suggests 7.25".)
- **Indents (from the left edge of the page):**

| Element | Left | Right | Characters per line | Blank lines before |
| --- | --- | --- | --- | --- |
| Scene heading | 1.50" | 7.50" | 61 | 2 |
| Action | 1.50" | 7.50" | 61 | 1 |
| Character | 3.50" | 7.25" | 38 | 1 |
| Parenthetical | 3.00" (first line 2.90") | 5.50" | 25 (first line 26) | 0 |
| Dialogue | 2.50" | 6.00" | 35 | 0 |
| Transition | right-aligned in 5.50"–7.10" (7.12" in newer files) | | 16 | 1 |
| Shot | 1.50" | 7.50" | 61 | 2 |
| Centered | centred on 1.50"–7.50" | | 61 | 1 |
| Lyrics | 2.50" | 6.00" | 35 | 0 in dialogue, 1 otherwise |

  Blank lines before are stored as points (24 pt = 2 lines, 12 pt = 1). A paragraph may override
  its element's indents, spacing and alignment. In the corpus, for example, many scene headings
  carry `SpaceBefore="12"`, which gives one blank line.
- **Scene numbers** print in the left margin at 0.75" and the right margin at 7.38".
- **Revision marks** (later, with revision mode) print in a column at 7.75".
- **Wrapping:** at spaces, and after a hyphen inside a word. A hyphen or dash ending a line may
  hang one column past the margin. Long words split hard. A parenthetical's first line hangs
  0.10" left of its continuation lines, so the opening bracket sits outside the text column, and
  holds one more character.
- **Blank lines at the top of a page are dropped.**

### 5.1 Break rules

1. A **scene heading** is never the last thing on a page: it needs at least 2 lines of what
   follows; otherwise it moves to the next page.
2. A **character cue** is never alone at the bottom of a page: a speech either fits, splits
   by rule 3, or moves whole.
3. **Dialogue** may split across pages between sentences or between its paragraphs:
   - The top keeps at least 2 lines, counting parentheticals. A parenthetical may be the last
     line before `(MORE)`.
   - Splitting inside a paragraph needs 2 of its lines on top, and at least 2 lines left over,
     counted in the paragraph as laid out rather than re-wrapped.
   - Of the valid splits, the one keeping the most lines on top wins; on a tie, the later one.
   - The top ends with `(MORE)` at the character indent. `(MORE)` hangs one row into the bottom
     margin, so it takes no body line.
   - The next page starts with `CHARACTER (CONT'D)`, one row above the body, which takes no body
     line either.
   - If no valid split exists, the whole speech moves.
4. **Action** may split between sentences. At least 2 lines stay on top, and at least 2 are left
   over, counted in the paragraph as laid out. Otherwise the paragraph moves whole.
5. A **transition** is never the first element on a page: if it would be, the previous element
   moves down with it (when that is possible without breaking rules 1–4).
6. **Dual dialogue** never splits.
7. `===` forces a break.
8. **Automatic `(CONT'D)`** (v1, on by default): when the same character speaks again after only
   action in the same scene, the cue gets ` (CONT'D)`. A scene heading resets it.
9. **Scene `CONTINUED`** (v1, off by default): `(CONTINUED)` at the bottom and `CONTINUED:` at the
   top of a page that splits a scene. Each costs a line, so it is part of pagination.

Rules 3 and 4 were measured from Final Draft's PDFs in M2; the old rule that a parenthetical is
never last before `(MORE)` turned out to be wrong. Rules 8 and 9 are not built yet: the corpus
writes `(CONT'D)` into the cue text, and has scene `CONTINUED` off in 106 of 109 files.

These rules match how Final Draft behaves by default, observed from its public output, not
from its code. Golden tests pin each rule, and `cargo xtask corpus` measures them against real
files.

## 6. Files

| Format | Read | Write | Notes |
| --- | --- | --- | --- |
| Fountain (`.fountain`, `.spmd`, `.txt`) | ✓ | ✓ | A working format (the default). Round-trip is lossless for everything we model. |
| Final Draft XML (`.fdx`) | ✓ | ✓ | Paragraph types, text styles, scene numbers, dual dialogue, title page. v1 keeps every part it does not model (revisions, tags, settings, Beat Board and so on) and writes it back unchanged. Only direct children of `Content` and `DualDialogue` are script paragraphs. Outline and summary text is written as Final Draft's `Outline 1–4` and `Summary` types, not `Section` and `Synopsis`. |
| Open Screenplay Format (`.osf`, and `.fadein`, a ZIP holding it) | ✓ | ✓ | Elements, emphasis, scene numbers, dual dialogue, notes, synopses, title page, page layout and element styles. Reads versions 1.2 to 4.0 in all their spellings; writes 3.0 for new documents and an imported document's own version otherwise. Keeps every part it does not model (settings, styles, lists, revision marks, vendor data) and writes it back. Sections, and synopses not attached to a paragraph, carry a `<method_storyteller kind="…"/>` marker. |
| PDF | – | ✓ | Courier Prime embedded, selectable text, a bookmark per scene. |

Limits: inputs over 32 MiB or 200 000 elements are refused with a message, not a crash.

## 7. App

- One window: menu bar, a scene navigator on the left (toggle), the script page view in the middle
  (a white page column with real margins), a status bar (element type, page x of y, scene).
- File: New, Open…, Save, Save As…, Import FDX…, Export FDX…, Export PDF…, Title Page…, Quit.
  Unsaved changes prompt on close (craft-quit on macOS).
- Settings (craft-settings): recent files, show scene numbers, navigator visibility, zoom.

## 8. CLI

`storyteller-cli` (binary `method-storyteller`; the GUI binary is `method-storyteller-app`):

```
method-storyteller convert in.fdx out.fountain      # any supported → any supported (by extension)
method-storyteller pdf script.fountain out.pdf
method-storyteller paginate script.fountain         # page/line report; --text dumps the paginated text
method-storyteller info script.fdx                  # scenes, characters, pages
method-storyteller run script.fountain cmd [k=v…]   # run an engine command and save
method-storyteller serve --control 9876 [file]      # headless control server
```

`mst` is a deprecated alias of `method-storyteller`: same behaviour and output, plus a one-line
rename note on stderr. It will be removed in a future release.

## 9. Out of scope for the MVP (later phases)

- **Revision mode:** coloured revision sets, locked pages, A-pages and A-scenes, revision marks.
- **Reports:** scene, character, location reports; dialogue counts.
- **Beat board / index cards** and outline view (sections and synopses are already parsed).
- **FilmCraft link:** export scene markers / script-to-timeline sync.
- Printing (craft-print), dual-dialogue editing UI (it's parsed, paginated and exported),
  spell check, collaboration, WASM build of the app. (A4 and automatic `(CONT'D)` moved into v1.)

The full V1.0 feature set, its gap analysis against this MVP, the test plan and the milestones
are in [docs/research/v1-feature-inventory.md](research/v1-feature-inventory.md).

## 10. Acceptance for the MVP

- `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace` pass.
- Tests: Fountain and FDX round-trips (parse → write → parse equal, and fixture files); golden
  pagination tests for each break rule; fuzz-style tests that hostile input never panics.
- The app opens a script, edits it with Tab/Enter flow and autocomplete, saves it, and exports a PDF.
