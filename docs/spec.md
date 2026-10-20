# Method Storyteller: product spec

Method Storyteller is an open-source screenwriting app, written in pure Rust. Its layout and `.fdx` support aim for parity with the files and pages the industry exchanges.
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
5. **Open formats, the writer's choice.** Open Screenplay Format (structured XML that keeps production detail) is the standard working format (the owner's decision, 2026-10-18; see [research/osf-vs-fountain.md](research/osf-vs-fountain.md)). New scripts are saved as a `.mstory` package (decisions 1 and 14): one ZIP file holding the script as `script.osf` and the production data as `production.json` (today its format name and version; the production data of M5 and M6 goes in here), any other member kept as it was. File > Save New Scripts as… chooses the package, plain `.osf` or Fountain; settings from before move from `.osf` to the package once. On a Mac the package is the app's own document type, with its own icon. Fountain (plain text: diffable, future-proof, readable anywhere) is fully supported: it opens, saves, and can be the default for new scripts (File > Save New Scripts as Fountain). An existing file keeps its format on Save.

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
| New Act | A television act's opening line (`TEASER`, `COLD OPEN`, `ACT ONE`, `TAG`): capitals, centred, and it starts a new page. `.fdx` New Act, OSF style "New Act"; Fountain has no act element, so it writes a centred line (`> ACT ONE <`), and a centred line that reads as an act opens one when read back. Element picker B; Enter goes on to a scene heading |
| End of Act | The line closing an act (`END OF ACT ONE`): capitals, centred. `.fdx` End of Act, OSF style "End of Act", a centred `END OF …` line in Fountain. Element picker E; Enter goes on to the next New Act. Kept in step (decision 18): after an edit, an act named the usual way (TEASER, ACT TWO, TAG) has its last End of Act line renamed `END OF <act>` when that line names an act; a writer's own line (END OF SHOW) is left alone. An act whose line is being written gets an End of Act line at its end when it has none. Opening a file changes nothing until the first edit |
| Section / Synopsis | Fountain `#` / `=`; outline only, not printed |
| Page break | Fountain `===`; forced break |

Inline emphasis (`*italic*`, `**bold**`, `_underline_`) and notes (`[[ ]]`) are stored as Fountain
markup inside element text. The PDF and FDX writers turn emphasis into styled runs; notes are not
printed. Script notes from an `.fdx` (with a range, a colour or an author) are kept on their
paragraph beside the text: what they cover in characters of the printed text, or the whole
paragraph.

## 4. Editing (MVP)

### 4.1 Element flow (as Final Draft does it)

Built in milestone M3 from Final Draft's documented defaults and its tutorial videos
([docs/research/v1-feature-inventory.md](research/v1-feature-inventory.md)). One table in the
engine (`flow.rs`) holds every rule, so they can become a preference later.

| In | Enter starts | Tab on an empty element | Tab after text starts | Shift-Tab gives |
| --- | --- | --- | --- | --- |
| Scene heading | Action | Action | Action | Action |
| Action | Action | Character | Character | Scene heading |
| Character | Dialogue | Transition | Parenthetical | Action |
| Parenthetical | Dialogue | Dialogue | Dialogue | Dialogue |
| Dialogue | Action | Parenthetical | Parenthetical | Character |
| Transition | Scene heading | Scene heading | Scene heading | Action |
| Shot | Action | Action | Action | Action |

- **Enter on an empty element** opens the element picker under the caret: one letter per
  element (S Scene heading, A Action, C Character, P Parenthetical, D Dialogue, T Transition,
  H Shot, N Centered, L Lyrics), or the arrows and Enter. Esc keeps the element as it is.
- Enter in the middle of an element splits it at the caret, and the second half takes the next
  element's type. Backspace at the start of an element joins it to the previous one. Up/Down at
  the first or last line move between elements.
- **⌘1–⌘7** (Ctrl on Windows and Linux) start a new element of that type after the current one:
  1 Scene heading, 2 Action, 3 Character, 4 Parenthetical, 5 Dialogue, 6 Transition, 7 Shot. On
  an empty element they change its type instead. **⌘⌥1–⌘⌥7** change the current element's type
  in place (Ctrl+Shift+1–7 on Windows and Linux, where Ctrl+Alt is AltGr). Parentheticals get
  their brackets automatically.
- Typing `int.` or `ext.` at the start of action turns it into a scene heading (its own undo
  step).
- Scene headings, character cues and transitions display and save in capitals.
- The status bar always shows what Tab and Enter will do next.
- Bold, italic and underline (Format > Bold ⌘B, Italic ⌘I, Underline ⌘U; command
  `format.style`) act on the selection in the caret's paragraph, or on the word the caret is
  in, and take the style off when all of it has it already. The text keeps Fountain's markup
  (`**bold**`, `*italic*`, `_underline_`); the editor draws the styled text in Courier Prime's
  bold and italic faces with the markers and `[[notes]]` faint, and the selection stays on the
  same words. A style the markup cannot say (bold straight after italic, with no space or
  punctuation between) is refused with a note in the status bar, never written wrong.
- Smart quotes (Format > Smart Quotes, on by default) curl `"` and `'` as they are typed in action,
  dialogue, parentheticals and lyrics; capitalised elements (headings, cues, transitions) keep
  straight ones. A leading apostrophe (`'90s`, `'til`) curls the right way.

### 4.2 Autocomplete

- **Character:** names already used in the script, most frequent first. Extensions are offered
  after `(`: `V.O.`, `O.S.`, `O.C.`, `CONT'D`. An empty cue in a two-hander offers the speaker
  before last.
- **Scene heading:** the intro (`INT.`, `EXT.`, `INT./EXT.`, `EXT./INT.`, `I/E`, `EST.`), then
  locations already used, then the time of day after ` - `, in Final Draft's order: `DAY`,
  `NIGHT`, `AFTERNOON`, `MORNING`, `EVENING`, `LATER`, `MOMENTS LATER`, `CONTINUOUS`,
  `THE NEXT DAY`, `MAGIC HOUR`, `DAWN`, `DUSK`, `SAME`, `SAME TIME`. Times already used come
  first.
- **Transition:** `CUT TO:`, `FADE IN:`, `FADE OUT.`, `FADE TO BLACK.`, `DISSOLVE TO:`,
  `SMASH CUT TO:`, `MATCH CUT TO:`, `JUMP CUT TO:`, `BACK TO:`, plus any used.

A popup shows matches under the caret. Up/Down choose, Enter or Tab accept, and Esc dismisses.
Autocomplete lists stored in the document, and editing them, come later.

### 4.3 Undo

Unlimited-ish (capped at 500 steps) snapshot undo; consecutive typing in one element merges into
one step.

### 4.4 Navigation and scene numbers

- **Scene navigator** side panel: every scene heading with its number, its length in eighths of a page and its page. A click jumps into the scene, with the caret at the start of its first action line (the first line under the heading when the scene has no action, the heading when it is empty), ready to type.
- **Go To** (Edit > Go To…, ⌘⌥G, Ctrl+Shift+G off macOS; command `nav.go_to`): a scene by its printed number (12, 12A), or by position only when the script has no scene numbers, or a page, or an act by its position (2) or name (TAG, ACT TWO, two); the caret lands on the scene heading, the page's first element or the act line. The navigator shows each New Act line above its scenes, and a click on it goes into the act's first scene (onto the act line when it has none).
- **Characters** (Production > Characters; command `character.table`): every speaking character, most speeches first, with speeches, scenes spoken in, words of dialogue and the first scene (its printed number, or its position). A name takes the caret to the character's first cue. Extensions such as (O.S.) and (CONT'D) are the same character.
- **Scene properties** (the navigator's right-click > Scene Properties…; command `scene.set_properties`): a scene's title, which the navigator shows in place of its heading, a summary, shown when hovering over the row, and a colour, a stripe at the row's left (none, or one of eight). One Undo. Not printed. They are `.fdx` `SceneProperties` `Title`, `Color` (written `#RRRRGGGGBBBB`) and `Summary`, left as they were while unchanged, and an OSF `<method_storyteller_scene title summary color/>` child of the heading; Fountain has no place for them, and saving to it says so.
- **Find and replace** (Edit > Find…, ⌘F; commands `find.next`, `find.replace`, `find.replace_all`): match case, whole words, optionally one element type; matches never span two elements; the current match is highlighted while typing stays in the find bar; Replace All is one Undo step; replacements in capitalised elements (headings, cues, transitions) stay capitalised.
- **Rename character** (Production > Rename Character…; command `character.rename`): every cue for the name, keeping extensions such as `(V.O.)`; optionally every capitalised whole-word mention in other text too, each in its own case (`NORA` → `EMMA`, `Nora's` → `Emma's`; a lowercase `will` is never the character WILL); one Undo step. Renaming onto another character's name merges the two, so the app asks first; a new name with `(`, `)` or `^` is refused. A character list stored in an imported .fdx is not renamed yet.
- **Spell check** (engine `spell.next`, `spell.ignore`, `spell.learn`, `spell.change`): the bundled English Hunspell dictionaries from SCOWL, US and UK (British, -ise spellings), read by `spellbook`; Edit > US English Spelling and UK English Spelling choose (decision 16), and the first run chooses from the system's region (British for GB, IE, AU, NZ, ZA, IN and the like, American otherwise). Character names from the cues, screenplay terms (INT., CONT'D, V.O. …) and words with digits are never flagged; `[[notes]]` are not checked; a word in capitals is right when its usual case is. Learned words are the writer's (kept in the app's settings); ignored words are kept with the script. Change is one Undo. In the app, misspelled words have a thin red underline as the writer types (View > Check Spelling While Typing, on by default), and Edit > Spelling… (⌘;) steps through them with suggestions, Change, Ignore and Learn.
- **Scene numbers** (Production > Number Scenes…; command `scene.number`): a start number,
  Keep existing numbers (on when any scene has one), From the current scene on, and Leave out
  the letters I and O (decision 20, off by default, remembered). Kept numbers stay; a new scene
  after `12` becomes `12A`, then `12B`; one before the first numbered scene becomes `A1`; one
  between `12A` and `12B` goes a level deeper, `12AA`; no number is given twice. Production >
  Edit Scene Number… (`scene.set_number`) sets or clears the caret's scene's number by hand,
  refusing one another scene has. One Undo each. View > Scene Numbers shows them in the editor
  and PDF. Stored in Fountain as `#12#`, in FDX as `Number="12"`, in OSF as `scene_number`.

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
  An element type's settings also say whether it is double-spaced (a row between its lines;
  1.5 rounds up), whether each element of the type starts a new page, and whether it prints in
  capitals however it was typed (typing in it then gives capitals, as for a scene heading). A
  multi-camera script uses all three: double-spaced dialogue, each scene on a new page, action
  in capitals.
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
8. **Automatic `(CONT'D)`** (built, on by default; off when an imported file says so): when the
   same character speaks again in the same scene, the cue gets ` (CONT'D)` at layout time. The
   text is not changed, and the editor shows the addition faintly. A scene heading resets it,
   dual-dialogue cues are left alone, and a cue that already says CONT'D is not doubled.
9. **Scene `CONTINUED`** (v1, off by default): `(CONTINUED)` at the bottom and `CONTINUED:` at the
   top of a page that splits a scene. Each costs a line, so it is part of pagination.

Rules 3 and 4 were measured from Final Draft's PDFs in M2; the old rule that a parenthetical is
never last before `(MORE)` turned out to be wrong. Rule 9 is not built yet: the corpus has scene
`CONTINUED` off in 106 of 109 files.

These rules match how Final Draft behaves by default, observed from its public output, not
from its code. Golden tests pin each rule, and `cargo xtask corpus` measures them against real
files.

## 6. Files

| Format | Read | Write | Notes |
| --- | --- | --- | --- |
| Fountain (`.fountain`, `.spmd`, `.txt`) | ✓ | ✓ | A working format, fully supported. Round-trip is lossless for everything we model except Shots (read back as action) and several paragraphs in one speech (joined). The boneyard (`/* … */`) is kept: the script parses exactly as if the blocks were not there, each block goes with the paragraph that follows it (or after the last one), is never printed, and is written back at the start of that paragraph's first line, so the file reads the same. Deleting a paragraph deletes its boneyard. |
| Final Draft XML (`.fdx`) | ✓ | ✓ | Paragraph types, text styles, scene numbers, dual dialogue, title page. v1 keeps every part it does not model (revisions, tags, settings, Beat Board and so on) and writes it back unchanged. Only direct children of `Content` and `DualDialogue` are script paragraphs. Outline and summary text is written as Final Draft's `Outline 1–4` and `Summary` types, not `Section` and `Synopsis`. Script notes in all three forms (inside the paragraph, the `ScriptNotes` list placed by `Range`, the older `UnanchoredScriptNotes`) are read onto their paragraph and written back in the form they came in; a plain `[[note]]` is written as a script note where it sits. Revision sets (`Revisions`) and each run's `RevisionID` are modelled and written back, and so are locked pages (`LockedPages`, placed by `Position`). Element settings, page layout and mores and continueds are read into the script's format, which pagination uses; unchanged, they go back as written. The words spell check ignores are read from and written to `SpellCheckIgnoreLists/IgnoredWords`; ignored ranges stay as they were. |
| Open Screenplay Format (`.osf`, and `.fadein`, a ZIP holding it) | ✓ | ✓ | The standard working format; new scripts hold it inside a `.mstory` package. Elements, emphasis, scene numbers, dual dialogue, notes, synopses, title page, page layout and element styles. Reads versions 1.2 to 4.0 in all their spellings; writes 3.0 for new documents and an imported document's own version otherwise. Keeps every part it does not model (settings, styles, lists, revision marks, vendor data) and writes it back. Sections, and synopses not attached to a paragraph, carry a `<method_storyteller kind="…"/>` marker. Script notes go into the paragraph's plain `note`, with their range, colour, author and Final Draft data in `<method_storyteller_note …/>` children. Revision sets map to `<lists><revision_colors>` and the active set to `settings@revision`, revised text to `text@revision`. Locked pages map to `settings@pages_locked` and each paragraph's `page_number` (A-pages such as `2A` included). Styles and settings map to the same format as `.fdx` element settings and page layout; what one format holds and the other cannot rides along as `fdx_xml` (OSF) or `StorytellerOsf` (`.fdx`). The words spell check ignores map to `<spelling><ignored>`; the user dictionary stays as it was. |
| PDF | – | ✓ | Courier Prime embedded, selectable text, a bookmark per scene. |

Saving says what a format will not keep: to Fountain, shots (read back as action), speeches
in more than one paragraph (joined), revision marks, coloured script notes and the words spell
check ignores (checked again next time); to Open Screenplay Format, sections (other OSF apps may print
them). The message names the count and the format that keeps them (`files::save_warnings`).

Limits: inputs over 32 MiB or 200 000 elements are refused with a message, not a crash.

## 7. App

- One window: menu bar, a scene navigator on the left (toggle), the script page view in the middle
  (a white page column with real margins), a status bar (element type, page x of y, scene).
- File: New, New TV Script…, Open…, Open Recent (the last ten scripts by name and folder, then
  Clear Menu; greyed out when empty), Close (⌘W: the script closes, asking first about unsaved
  changes, and an empty one takes its place, decision 15), Save, Save As…, Import FDX…, Export FDX…, Export PDF…, Title
  Page…, Quit. New TV Script asks one-hour (Teaser, Acts One to Five, Tag) or half-hour (Cold
  Open, Acts One to Three, Tag) or a multi-camera half-hour (Cold Open, Acts One and Two, Tag),
  offering the writer's last choice first, and lays the acts out: each act's line, an empty
  scene heading with the caret in the first, and its END OF line. A multi-camera script letters
  its scenes (A to Z, then AA; a script is lettered when every scene number it has is letters,
  and after each edit its headings are lettered again in order, so a new or moved scene takes
  its place), lists each scene's speaking characters under the heading on the page,
  `(NORA, OWEN)`, in the order they first speak, starts each scene on a new page, prints action in
  capitals and double-spaces dialogue. Those three are element settings, so `.fdx` and OSF keep
  them; Fountain keeps the letters (`#A#`) but not the settings. Typing into a capitals type
  stores capitals, as for a scene heading; keeping the writer's own casing and showing capitals
  only on screen and page waits for the editor to draw a capitals type itself.
  Unsaved changes prompt on close (craft-quit on macOS). While a script has unsaved changes, a
  copy goes into the app's Recovery folder every minute, as Fountain, under the script's name;
  saving or Don't Save takes it away. A copy still there at the next launch is offered first,
  ahead of the first-launch sample: Open the Copy (an unsaved script, to save with Save As; the
  copy stays, and the script's copies go on under its name, until it is saved or thrown away),
  Throw It Away, or Later (the copy waits for the next launch). A script's copies never take the
  name of one still waiting: a new Untitled script writes `Untitled 2` beside a waiting
  `Untitled`.
- Edit: Undo, Redo, Cut, Copy, Paste, Select All (the paragraph the caret is in), Find…, Go To….
  Cut, Copy, Paste and Select All carry no menu shortcut on purpose: ⌘X, ⌘C, ⌘V and ⌘A go
  straight to the text being edited, and the menu items do the same.
- Edit > Copy Scene, and Copy Scene on a right-click in the navigator, put the scene on the
  clipboard as Fountain text: pasted back into Storyteller it is the same elements, pasted
  anywhere else it reads as a screenplay. Pasting text in several paragraphs (a blank line
  between two) makes elements, read as Fountain, as one Undo: the text before the caret stays
  in the line, the pasted elements follow, and the text after it becomes a line of its own. A
  paste within one paragraph is an ordinary paste. Copying any range of elements, across
  paragraphs, comes with a selection that spans them (next).
- Settings (craft-settings): recent files, show scene numbers, navigator visibility, zoom.
- The first launch with nothing to open (no file given, no recent files) shows the sample script,
  Field Station, once, as an unsaved copy, with a quiet line saying so. A writer who already has
  scripts never sees it unasked. Save asks where to keep a copy; the bundled sample is never
  written. Help > Open Sample Script brings it back at any time.
- At launch, a file named on the command line opens first, then a file opened from the Finder
  (macOS hands a launch's files to the app before it finishes launching, so the sample never
  shows first and is retired unshown), then the first-launch sample.
- Command-line requests, for Method Launcher (SUITE-STANDARDS.md §3): `--open-sample` opens
  the sample at start, unless a file was given; `--check-for-updates` runs Help > Check for
  Updates on the first frame.

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
