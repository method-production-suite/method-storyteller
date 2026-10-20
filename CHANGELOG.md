# Changelog

What changed in each version of Method Storyteller, newest first. The app shows the section
for its own version in Help > What's New, once after an update.

## Unreleased

### New

- New scripts save as a `.mstory` package: one ZIP file holding the script as Open Screenplay
  Format and its production data beside it, with its own icon in the Finder. File > Save New
  Scripts as… still offers plain Open Screenplay Format and Fountain.

### Before you update

- Scripts saved as `.mstory` by this version open only in this version and later.

## 0.4.0

### New

- Bold, italic and underline: Format > Bold (⌘B), Italic (⌘I) and Underline (⌘U) on the
  selection or the word the caret is in, and the editor shows the styles, with the markup faint.
- File > New TV Script offers a multi-camera half-hour: Cold Open, Acts One and Two, Tag, with
  lettered scenes (a new scene takes its letter), each scene on a new page with its speaking
  characters under the heading, dialogue double-spaced and action in capitals.
- Scripts that set an element to double spacing, to start each one on a new page, or to print in
  capitals now look that way on the page and in the PDF, and typing in a capitals element gives
  capitals.
- End of Act lines follow their acts: rename ACT TWO and its END OF ACT TWO follows, and a
  new act gets its End of Act line as you name it. Your own lines, such as END OF SHOW, stay
  as you wrote them.
- Each release's checksum list comes signed (`SHA256SUMS.minisig`).

## 0.3.0

### New

- Television acts: New Act (TEASER, COLD OPEN, ACT ONE, TAG) and End of Act lines, in capitals
  and centred, each act on a new page. In the element picker as B and E, and in Format.
- File > New TV Script: a one-hour or half-hour script laid out in acts, ready to write.
- Acts in the scene navigator, each above its scenes; a click goes into the act's first scene.
  Go To finds an act by its number or its name (2, TAG, ACT TWO).
- One universal Mac app for Apple silicon and Intel; it updates itself on both.

### Before you update

- Scripts with act lines saved by this version show those lines as action in 0.2.0. Update
  before opening them there.

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
