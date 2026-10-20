# Production features: the M5 and M6 spec

What M5 (production I) and M6 (production II and reports) build, in the order they are built,
and the decisions they need. It extends [spec.md](spec.md) and follows the research in
[research/v1-feature-inventory.md](research/v1-feature-inventory.md) and
[research/v1-notes/official-docs.md](research/v1-notes/official-docs.md) (Q4). Formats are named
factually where interop needs it, as the clean room allows; nothing here comes from another
vendor's software or files beyond what a saved `.fdx` and its PDF show.

## 1. Principles

1. **Never lose a production's data.** Everything a shooting script carries (scene numbers,
   omitted scenes, revision sets and marks, locked pages) already round-trips through `.fdx` and
   Open Screenplay Format. Every feature below reads the data that is already there before it
   writes any of its own.
2. **Never move a locked page break.** Once pages are locked, an edit may add A-pages or leave
   a page short, but a page that was issued keeps its number and its first line.
3. **One command, one Undo.** Each action is a named engine command, so the control channel,
   the CLI and tests drive it exactly as the menus do.
4. **Ask before anything that cannot be taken back.** Unlocking all pages and deleting a
   revision set that is in use ask first, and both are still one Undo.
5. **Production data lives in the script.** Numbers, omitted scenes, revision sets and locked
   pages are part of the script, so `.fdx` and Open Screenplay Format carry them. A `.mstory`
   package's `production.json` holds only what neither format has a place for (section 5).

## 2. What is there today

- `Element::scene_number`, printed in both margins; `Script::number_scenes` and the
  `scene.number` command number scenes 1, 2, 3, optionally keeping existing numbers.
- `Script::revisions` (sets with id, name, mark, colour and page colour; the active set) and
  each element's `revised` spans (character ranges and their set), read from and written to
  `.fdx` (`Revisions`, `RevisionID`) and Open Screenplay Format (`revision_colors`,
  `text@revision`). Nothing draws them yet.
- Each element's `page_starts` (locked pages and A-pages such as `2A`, and where in the
  paragraph a page starts), read and written in both formats. Pagination does not use them yet.
- Omitted scenes from `.fdx` are kept as raw XML and written back, but not modelled.
- Lettered scenes (multi-camera scripts) number themselves; this spec leaves them as they are.

## 3. M5, production I

One PR each, in this order.

### 3.1 Scene numbering

Production > Number Scenes… opens a dialog:

- **Start at** (default 1) and **From the current scene** (number only from the caret on).
- **Keep existing numbers** (on once any scene has a number). Unnumbered scenes between two
  numbered ones get the earlier number with a letter: between 12 and 13, `12A`, `12B`. A
  scene before the first numbered one gets a letter before the number: `A1`. Letters run A to
  Z, then AA, AB.
- **Remove numbers.**
- **Show in the left margin**, **Show in the right margin** (both on by default; a script
  setting, kept in the element settings both formats share).

Production > Edit Scene Number… sets or clears one scene's number by hand. Two scenes may not
share a number: the dialog says which scene has it.

Command: `scene.number` gains `from_caret`, `remove`, and `letters_before_first`;
`scene.set_number` (index, number or empty).

*Decision S1, letters I and O.* Some productions skip I and O, which read as 1 and 0. Built on
the recommendation: **skip neither by default**, with a setting to skip both, until the parity
check (M7) shows what current practice in saved files is.

### 3.2 Omit and unomit

Production > Omit Scene replaces a scene's body with `OMITTED`, keeping its heading's number
(`12  OMITTED  12`). The scene's elements are kept on the heading, out of sight, and
Production > Unomit Scene puts them back where they were. In revision mode, omitting counts as
a revision of that line. The navigator shows the row as `OMITTED`, greyed out; a click still
goes there.

Model: `Element::omitted: Vec<Element>` on the heading (empty when the scene is not omitted)
and the printed text `OMITTED`. `.fdx`: read and write the omitted-scene wrapper the importer
keeps raw today (its shape is in the corpus; the structure is read, never the text). Open
Screenplay Format: a `<method_storyteller kind="omitted">` marker holding the scene's
paragraphs. Fountain: written as the heading with `OMITTED` and the kept content in a boneyard
block, so nothing is lost.

Commands: `scene.omit` and `scene.unomit` (index anywhere in the scene).

### 3.3 Revision sets

Production > Revisions… lists the sets: name, mark, text colour, page colour. The writer adds,
renames, reorders and deletes sets, and chooses the active one.

- A new script's first set is **White** (no colour). **Next Set** adds the next in the standard
  cycle: White, Blue, Pink, Yellow, Green, Goldenrod, Buff, Salmon, Cherry, Tan, then Double
  White, Double Blue and on. Each set's name defaults to `Blue Revision` with today's date.
- The mark is up to two characters, `*` by default.
- Deleting a set that marks text asks first, and moves its marks to the set before it.

Commands: `revisions.add`, `revisions.update`, `revisions.delete`, `revisions.set_active`.

### 3.4 Revision mode

Production > Revision Mode (a toggle; ⌘⇧R). While on, everything typed or changed is marked in
the active set:

- Inserted and changed characters become `revised` spans in the active set. Typing inside a
  span of another set marks the new characters in the active one.
- Deleting text leaves a **deletion mark** at the place: a zero-length span the margin shows,
  as `.fdx` files record them (deleted-text markers are in 8 of 14 corpus projects).
- New elements are revised whole; joining two elements marks the join.

Production > Mark Revised and Clear Revised act on the selection (a selection across
paragraphs comes with the rest of M4).

*Decision S2, shortcuts.* ⌘] and ⌘[ are the familiar keys for Mark and Clear Revised. Built on
the recommendation: use them, since nothing in the app takes them.

### 3.5 Revisions on screen and on the page

- **Editor:** revised text in the set's colour; deletion marks as a thin caret in the margin.
  View > Show Revisions chooses **Active set**, **All sets** or **None**.
- **Margin marks:** each line with revised text, or a deletion mark, gets the set's mark at
  7.75" from the left edge (the column `.fdx` files record), on screen and in the PDF.
- **Header:** revised pages print the set's name and date at the top right, under the page
  number, as `Blue Revision 10/20/26`. Pages with no revision in the issued set print none.
- **Page colour** is the set's, shown on screen behind revised pages when View > Page Colours
  is on. PDFs stay white unless File > Export PDF… chooses **Coloured revised pages**, since
  productions print on coloured paper.

### 3.6 Issuing pages

File > Export PDF… gains:

- **All pages**, **Revised pages only** (pages with any mark in the chosen sets), a **page
  range** (`12-18, 22A`) and a **scene range** (`12-15`).
- **Clean copy:** no marks, no colours, no header label, the same pages.
- With locked pages (M6), every page keeps its locked number, A-pages included.

Command: `file.export_pdf` gains `pages`, `scenes`, `revised_only`, `clean` and `colour`.

### 3.7 M5 data and files

Everything in 3.1 to 3.5 already has a place in both formats, apart from the omitted-scene
marker (3.2) and the show-revisions choice (a view setting, kept in the app's settings).
Fountain keeps scene numbers, `OMITTED` and the kept content; it has no revisions, and saving to
it says so (the warning exists today).

## 4. M6, production II and reports

### 4.1 Locked pages on the page

Pagination honours `page_starts`. Each locked page starts where it started when it was locked
(a paragraph, or a place inside one):

- Content that no longer fits on its page flows onto **A-pages**: after page 12, `12A`, `12B`.
- A page whose content has gone stays short, or empty with its number, rather than pulling the
  next page up.
- Two locked pages whose content now fits on one print as one page numbered `7-8`.
- Scene numbers, (MORE) and (CONT'D) work as they do now, inside these rules.

This alone makes imported shooting scripts print with the right numbers, before any authoring
tool exists. It is measured against the corpus files with locked pages (5 files, 470 pages).

### 4.2 Locking

- Production > Lock Pages freezes the current page breaks into `page_starts`. It asks to turn
  on revision mode, since a locked script is a production draft.
- Production > Lock A-Pages adds a level: A-pages made after it become `12AA`.
- Production > Unlock A-Pages removes one level; Production > Unlock All Pages asks first,
  renumbers straight through, and is one Undo.
- Locking repair tools (relock from the caret, edit a page number by hand) come later.

Commands: `pages.lock`, `pages.lock_a_pages`, `pages.unlock_a_pages`, `pages.unlock_all`.

*Decision S3, lettering past Z and merged pages.* The research has only examples (`21A`, `AA`,
`7BB-9A`). Built on the recommendation: A to Z, then AA to ZZ, and merged pages as `7-8`; the
parity check settles anything a saved file shows differently.

### 4.3 Reports

Production > Reports… opens one window with a report picker, each one also a command:

| Report | Rows | Command |
| --- | --- | --- |
| Scenes | number, heading, INT/EXT, location, time of day, page, length in eighths, characters, title | `report.scenes` |
| Locations | location, scenes, pages in eighths, INT/EXT split | `report.locations` |
| Characters | the character table of M4, with scenes listed | `character.table` |
| Cast | each scene's speaking characters, and the scenes each character is in | `report.cast` |

Each report exports as CSV (comma-separated, UTF-8, a header row) and prints as a PDF table.
File > Export Plain Text… writes the script as it reads on the page, with no markup.

### 4.4 The Method Schedule export

File > Export for Method Schedule… writes a structured file Method Schedule reads: the title
page, and every scene with its number, heading parts (INT/EXT, location, time of day), page,
length in eighths, speaking characters, summary and colour. The format is agreed with the
Method Schedule session before it is built (decision S4), and versioned from its first release.

## 5. What `production.json` holds

The package's production data file holds what neither `.fdx` nor Open Screenplay Format has a
place for, and nothing else:

- `format` and `version` (today's contents);
- the Method Schedule link: the schedule file the script was last exported to, if any;
- report settings the writer chose (columns, sort), so a report comes out the same next time.

Everything else stays in the script document, so a package unzipped and opened in another
app keeps its production data.

## 6. Decisions

| | Decision | Built on |
| --- | --- | --- |
| S1 | Skip the letters I and O in scene numbers | No by default, a setting to skip both |
| S2 | Mark and Clear Revised on ⌘] and ⌘[ | Yes |
| S3 | A-page lettering past Z, and merged pages | AA to ZZ; merged pages as `7-8` |
| S4 | The Method Schedule export format | To agree with the Method Schedule session first |

All four are reversible. S1 to S3 go to the roadmap's Waiting on the owner as built on the
recommendation; S4 waits for the schedule session.

## 7. For the parity check (M7)

Added to the prepared checks: how a saved file letters scenes inserted before scene 1 and past
Z, whether I and O are skipped, how a merged locked page is numbered, the omitted-scene wrapper
for a scene with dual dialogue, and where the revised-page header label sits.

## 8. Order of PRs

1. Scene numbering (3.1).
2. Omit and unomit (3.2).
3. Revision sets (3.3).
4. Revision mode (3.4).
5. Revisions on screen and on the page (3.5).
6. Issuing pages (3.6).
7. Locked pages on the page (4.1), measured on the corpus.
8. Locking (4.2).
9. Reports and plain text (4.3).
10. The Method Schedule export (4.4), once S4 is agreed.
