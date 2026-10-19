# Changelog

What changed in each version of Method Storyteller, newest first. The app shows the section
for its own version in Help > What's New, once after an update.

## 0.2.0

The first release.

### New

- Write a screenplay with Enter and Tab moving between scene heading, action, character,
  parenthetical, dialogue and transition, and ⌘1 to ⌘7 to pick an element directly. Enter on
  an empty line opens the element picker.
- Character, location and time-of-day autocomplete, a guess at who speaks next, automatic
  (CONT'D), and smart quotes.
- Type INT. or EXT. and the line becomes a scene heading, also when a whole heading is pasted
  into an empty line.
- Spell check as you type: misspelled words are underlined, and Edit > Spelling… (⌘;) steps
  through them with suggestions, Change, Ignore and Learn.
- Cut, Copy and Paste in the Edit menu, and Select All, which selects the paragraph you are
  in. Keyboard Shortcuts lists their keys.
- Edit > Copy Scene, or a right-click on a scene in the navigator, copies the whole scene:
  pasted back it is the same elements, pasted elsewhere it reads as a screenplay. Pasting
  screenplay text in several paragraphs makes headings, action, characters and dialogue.
- The scene navigator: every scene with its number, page and length in eighths. A click goes
  straight into the scene's action, ready to type.
- Go To a scene or a page, find and replace with one Undo for Replace All, and rename a
  character everywhere at once.
- Pages laid out the way scripts print: Courier Prime 12 pt, standard margins, `(MORE)` and
  `(CONT'D)` at page breaks.
- Save in Open Screenplay Format, the standard for new scripts, or Fountain (File > Save New
  Scripts as).
- Open and save `.fdx` files, and Open Screenplay Format in a ZIP archive; export PDF with a
  bookmark for each scene.
- Script notes, revision sets and revised text, locked pages and A-pages, element settings and
  page layout, and the words spell check ignores carry over between `.fdx` and Open Screenplay
  Format, both ways.
- Fountain's boneyard (`/* … */`) is kept, and saving says what a format will not keep.
- On a Mac, double-click a script in the Finder to open it; `.osf`, `.fountain` and `.fdx`
  files show their own document icons.
- The first launch shows the sample script, Field Station; Help > Open Sample Script brings it
  back.
- Title page, scene numbers, page view and draft view, zoom.
- A Help menu: user guide, keyboard shortcuts, this list, bug reports, licences, and checking
  for updates.
- A command line tool, `method-storyteller`, that converts, paginates and exports without a
  window, and a control channel that drives the app from scripts.

### Known gaps

- No revision mode, printing or side-by-side editing of dual dialogue yet.
- Automatic updates on Apple silicon Macs only; elsewhere, download new versions from the
  website.
- The spelling dictionary is US English.
