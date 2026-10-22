# `.fdx` and Open Screenplay Format: what converts, and how exactly

The owner's goal (2026-10-18): everything that `.fdx` and Open Screenplay Format (OSF) can both
hold converts exactly in both directions, and a round trip through the other format comes back
the same. This report says what maps exactly, what is approximated, and what cannot map and why.
The table is checked by `crates/core/tests/fidelity.rs`. That test lists the same rows, and it
round-trips an invented script through both formats for every row marked exact. Formats are
named factually here; how other apps behave is not described.

## How the two formats are joined

- **One model.** Both readers fill the same model: elements with their notes, revised spans and
  locked page starts, the script's revision sets, and its format (page layout and element
  settings). Both writers write from it, and pagination comes from it. A script therefore
  paginates the same whichever format it was saved in.
- **Unchanged parts go back as read.** Each format keeps the sections it came with. A section
  whose model is unchanged is written back exactly as read, so `.fdx` to `.fdx` loses nothing.
- **What the other format cannot hold rides along.** We add our own elements and attributes,
  named `method_storyteller…` in OSF and `Storyteller…` in `.fdx`. Other apps ignore them, and a
  trip through the other format comes back the same. They are written only when writing from the
  model would not recreate the data anyway, so a file that started in one format gains nothing
  it does not need.
- **Offsets.** A note's range, revised text and a locked page's start count characters of a
  paragraph's printed text. `.fdx` `ScriptNotes@Range` and `LockedPage@Position` count across the
  whole script, one character per paragraph break, with a dual-dialogue block's wrapper counted
  after its speeches. The private corpus confirms this: 505 of 508 scene headings sit on the
  locked page that `.fdx` recorded for them.
- **Lengths** are EMU, 914,400 to the inch. That holds `.fdx` inches and points and OSF tenths
  of a millimetre exactly; only writing to the coarser unit rounds (see the approximated rows).

## The table

<!-- table:start -->
| What | Fidelity | How |
| --- | --- | --- |
| Element types, custom types, outline and summary paragraphs | exact | paragraph `Type` and OSF style; types OSF lacks carry `method_storyteller kind` |
| Bold, italic, underline | exact | `Text@Style` and `text@bold/italic/underline` |
| Scene numbers, A-numbers included | exact | `Paragraph@Number` and `para@scene_number` (`number` in 4.0) |
| Dual dialogue | exact | `DualDialogue` and `dualdialogue` on the first cue |
| Page breaks | exact | `StartsNewPage` and `pagebreakbefore` |
| Title page fields | exact | `TitlePage` paragraphs and bookmarked `titlepage` paragraphs |
| Script notes: text, range, colour, author, all three `.fdx` forms | exact | OSF `note` (plain) plus `method_storyteller_note`; ranges count printed characters, one per paragraph break |
| Revision sets: name, mark, mark colour, page colour, active set | exact | `Revisions` and `revision_colors`, `settings@revision`; colours OSF has no place for ride on `method_storyteller_` attributes |
| Revised text | exact | `Text@RevisionID` and `text@revision` |
| Deleted-text marks | exact | `DeletedText` (`Offset` counts like note ranges) and our `method_storyteller_deletion`; OSF's own `marks` are kept as read |
| Locked pages and A-pages | exact | `LockedPages` (`Position` counts like note ranges) and `pages_locked` with `para@page_number`; a page starting inside a paragraph carries `method_storyteller_page` |
| Element settings: font, size, styles, alignment, indents, space before, line spacing, page break, Enter and Tab | exact | `ElementSettings` and `styles`, in thousandths of an inch |
| Page size, top and bottom margins, (MORE), (CONT'D), automatic continueds | exact | `PageLayout`, `MoresAndContinueds` and `settings` |
| Words spell check ignores | exact | `SpellCheckIgnoreLists/IgnoredWords` and `<spelling><ignored>`, as written; OSF's user dictionary stays in the file |
| Pagination | exact | both formats paginate from the same model |
| OSF revision colour names | approximated | OSF has a fixed palette without RGB: the name comes from the set's name, else the nearest page colour; the palette's RGB values are our choice |
| `.fdx` lengths written to OSF | approximated | OSF counts tenths of a millimetre, coarser than `.fdx` hundredths of an inch; our own files bring the `.fdx` value back, and other apps get the nearest tenth |
| Lengths from OSF written to `.fdx` | approximated | can need up to six decimals of an inch (`7.248031`) to come back exact; to confirm in the final check |
| Note body formatting | approximated | fonts and indents on a note's own text survive `.fdx` to `.fdx`; through OSF the text is rewritten plain |
| Tab before and after | approximated | OSF has both, `.fdx` one: `style_tab_after` is `TabKey`, `style_tab_before` rides along |
| Omitted scenes | approximated | the heading, its number and the scene it keeps go both ways (`StorytellerOmitted`, `method_storyteller_omitted`); a file's own `OmittedScene` wrapper stays in `.fdx` |
| OSF page margins, keep-with-next, header and footer styles | cannot map | `.fdx` has no place: they ride along in `StorytellerOsf` and come back to OSF |
| `.fdx` shortcuts, outline levels, hidden text, scene CONTINUEDs, header margins | cannot map | OSF has no place: they ride along in `fdx_xml`, `fdx_page_layout`, `fdx_mores` and come back to `.fdx` |
| Ranges spell check ignores | cannot map | `.fdx` `IgnoredRanges` are character spans OSF has no place for; kept in `.fdx` |
| Run fonts, colours and highlights | cannot map | not kept yet in either format |
| Beat Board, cast, tags, SmartType lists, header and footer, scene number options, spelling lists | cannot map | kept in the format they came in; not converted (OSF lists and spelling likewise) |
| Scene page and length | cannot map | `.fdx` caches them and OSF does not; written from our pagination on every `.fdx` save |
<!-- table:end -->

## Evidence

- Round trips on invented files: `crates/core/tests/{notes,revisions,locked_pages,format,fidelity}.rs`.
- The private corpus of 108 real `.fdx` files, measured with `cargo xtask corpus` (counts only,
  nothing printed or committed). After `.fdx` to OSF to `.fdx`, all of these come back the same:
  50 of 50 script notes, 10,280 of 10,280 revised spans, the revision sets in 108 of 108 files,
  470 of 470 locked pages, and the element settings and page layout in 108 of 108 files. The
  layout each file paginates with is also the same in 108 of 108. Pagination against the cached
  scene pages is unchanged by the milestone (7,914 of 9,052 scenes).

## The owner's final check

Two invented files in `crates/core/tests/fixtures/`, written by Method Storyteller from the test's script:
`lossless-check.fdx` (after a trip through OSF) and `lossless-check.osf`. Open the `.fdx` in the
licensed reference app and look at:

1. The title page: THE STATION.
2. ACT ONE is an outline paragraph (hidden when printing), and ACT TWO is a New Act element,
   bold and underlined, centred, on a new page.
3. Scene 4A, INT. RADIO ROOM - NIGHT, with a script note on the word NIGHT ("Which set?", green,
   by A. Writer).
4. The action line, with a red note "Louder?" by R. Lee at its start, and "crackles" marked as
   revised in Blue Rev. (asterisk in the margin, blue).
5. Dual dialogue, MARA and BEN side by side.
6. Production > Revisions shows one set, Blue Rev., and it is active.
7. Pages are locked: page 1, then 1A starting at "crackles" in the action line. Say how the app shows the
   A-page, since no real file had one.
8. Format > Elements: Action and New Act have the indents, spacing and Enter key above; Page
   Layout is US Letter with 1" top and bottom margins.
9. The spelling check passes over MARA: it is on the script's ignore list.
10. Save in the reference app, send the file back, and we check that it reads the same.

Open questions this settles: how a dual-dialogue wrapper counts in `Range` and `Position`; how
an A-page's `PageNumber` is spelled; whether notes without `DateTime` and indents with up to six
decimals are accepted; whether the reference app keeps our `Storyteller…` attributes. One more
is OSF's own: no public file shows a filled `<ignored>` list, so we write its entries like the
user dictionary's (`<entry word="…"/>`) and read either shape.
