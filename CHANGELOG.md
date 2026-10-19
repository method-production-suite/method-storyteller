# Changelog

What changed in each version of Method Storyteller, newest first. The app shows the section
for its own version in Help > What's New, once after an update.

## 0.1.0 (not released yet)

### New

- Write a screenplay with Enter and Tab moving between scene heading, action, character,
  parenthetical, dialogue and transition, and ⌘1 to ⌘7 to pick an element directly.
- Character, location and time-of-day autocomplete.
- Pages laid out the way scripts print: Courier Prime 12 pt, standard margins, `(MORE)` and
  `(CONT'D)` at page breaks.
- Save in Fountain or Open Screenplay Format, and choose which one new scripts use
  (File > Save New Scripts as).
- Open and save `.fdx` and `.fadein` files; export PDF.
- Script notes from `.fdx` files keep their place, colour and author when saved as `.fdx` or
  Open Screenplay Format, and back.
- Revision sets (name, mark, colours) and revised text from `.fdx` and Open Screenplay Format
  files are kept when saving in either format.
- Locked pages and A-pages from `.fdx` and Open Screenplay Format files are kept when saving in
  either format.
- Element settings and page layout (fonts, indents, spacing, Enter and Tab flow, paper size,
  margins, (MORE) and (CONT'D)) carry over between `.fdx` and Open Screenplay Format, and a script
  paginates the same in both.
- A custom `.fdx` element type (such as New Act) keeps its name through Open Screenplay Format.
- The words spell check ignores in a script are saved with it in `.fdx` and Open Screenplay Format,
  and carry over between the two.
- Title page, scene navigator, scene numbers, page view and draft view, zoom.
- A Help menu: user guide, keyboard shortcuts, this list, bug reports and licences.
- A command line tool, `method-storyteller`, that converts, paginates and exports without a
  window, and a control channel that drives the app from scripts.

### Known gaps

- No installers yet, and no automatic updates until the first release.
- No printing, revision mode or side-by-side editing of dual dialogue yet.
