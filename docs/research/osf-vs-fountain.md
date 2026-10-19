# Open Screenplay Format or Fountain as the default working format

Asked by the owner on 2026-10-18: should new scripts be saved as Open Screenplay Format (OSF)
rather than Fountain, for what OSF can hold, without giving up compatibility? This compares the
two formats, measures what Method Storyteller's own reader and writer keep, and ends with a
recommendation. Formats are named factually; comparisons with other apps stay in private notes.

> **Decision (the owner, 2026-10-18):** Open Screenplay Format is the standard and the default
> for new scripts, with Fountain fully supported and one choice away, and a "Lossless .fdx"
> milestone alongside. The study's recommendation below was to keep Fountain for now; the owner
> chose OSF for its feature set. The gaps in section 5 are being closed first.

**In short:** OSF can hold far more of a production draft (revisions, locked pages, styles,
per-element settings), and our OSF round trip already keeps every element type Fountain cannot.
But today the extra capacity is mostly unused: what Storyteller writers can make right now
survives both formats equally, apart from Shots and multi-paragraph speeches. Fountain is what
the rest of the Method suite reads, what any editor and git handle best, and the only one of the
two with a public, stable spec. **Recommendation: keep Fountain as the default for drafting now,
and make OSF the default the moment a script holds production data (revisions or locked pages,
M5),** with the other format always one Save As away. Details and trade-offs below.

---

## 1. What each format can hold

| | Fountain | OSF |
|---|---|---|
| Scene heading, action, character, parenthetical, dialogue, transition | Yes | Yes |
| Shot | **No** (no Shot element; read back as action) | Yes (built-in style) |
| Centered, lyrics | Yes (`> <`, `~`) | Yes (alignment override, a Lyrics style) |
| Page break | Yes (`===`) | Yes (`pagebreakbefore`) |
| Several paragraphs in one speech | **No** (one dialogue block; paragraphs merge with a line break) | Yes |
| Bold, italic, underline | Yes (`**`, `*`, `_`) | Yes, plus strikethrough and highlight colour |
| Title page | Yes (key: value fields) | Yes (`<titlepage>`, fields as bookmarks, free layout) |
| Dual dialogue | Yes (`^`) | Yes (`dualdialogue` on the first cue) |
| Scene numbers, A-numbers | Yes (`#4A#`) | Yes, plus locking and a `1AB` numbering mode |
| Locked pages, A-pages | **No** | Yes (`pages_locked`, a page number per paragraph) |
| Revisions and colours | **No** | Yes: colour sets with marks, revised text per run, deletion marks; no date or name per set |
| Notes | `[[inline notes]]`: text only, no colour, author or range | One plain note per paragraph; no colour, author or range either |
| Boneyard (text kept out of the script) | Yes (`/* … */`) | No equivalent |
| Sections and synopses (outline) | Yes (`#`, `=`), non-printing everywhere | **No** standard element: a synopsis rides on a paragraph; sections need a custom style |
| Element settings (fonts, indents, spacing, Enter/Tab flow) | No | Yes (styles, per-paragraph overrides) |
| Page layout, headers, footers, (MORE)/(CONT'D) text | No | Yes |
| Character, location and transition lists | No (derived from the text) | Yes (`<lists>`) |
| Spelling user dictionary and ignore list | No | Yes (`<spelling>`) |
| Bookmarks | No | Yes |
| Breakdown tags | No | Category list only; how tags attach to text is not documented |
| Images | No | No |

OSF is a superset for production work. Fountain is a superset only for the boneyard and for an
outline (sections and synopses) that every reader treats as non-printing.

## 2. Round trip through Storyteller's own reader and writer

Measured on 2026-10-18 at `main`: every sample read as Fountain, written in each format, read
back, and compared element by element (type, text, scene number, dual flag, level) and title
page. Samples: the Field Station example, the lighthouse fixture, the 11 pagination golden
scripts, and a synthetic script with a section, a synopsis, a numbered scene, emphasis, a note,
a boneyard, forced elements, centered text, lyrics, dual dialogue, a transition and a page
break.

| Round trip | Result |
|---|---|
| Fountain → Fountain | Identical on every sample |
| Fountain → OSF → model | Identical on every sample |
| Fountain → OSF → Fountain ("Fountain one click away") | Identical on every sample |
| A Shot and a two-paragraph speech, through Fountain | **Lost**: the Shot comes back as action; the two paragraphs come back as one, joined by a line break |
| The same, through OSF | Identical |
| A boneyard in a Fountain file, on opening | **Dropped by our reader** (the model has nowhere to keep it), so it is gone whichever format is saved next |
| An imported `.fdx` (notes, revisions, locked pages, element settings), saved as `.fdx` | Everything kept |
| The same `.fdx`, saved as OSF or as Fountain, then as `.fdx` | Notes, revisions, locked pages and element settings **lost in both**; scene page and length are recomputed |

Two things stand out. First, at the level of what a writer can create in Storyteller today,
the two formats differ only on Shots and multi-paragraph speeches. Second, OSF's extra capacity
is not yet used: our OSF writer does not yet map `.fdx` notes to `note`, revisions to run
revisions, or locked pages to page numbers. Those are writer gaps, not format limits, and M5 and
M6 have to close them anyway.

Sections and synopses survive our own OSF round trip because Storyteller marks them with a
private `<method_storyteller kind="…"/>` child. Another app reading that OSF file would show a
section as a line of action, printed: OSF has no standard outline element.

## 3. Compatibility

| | Fountain | OSF |
|---|---|---|
| Spec | A public syntax page (fountain.io), stable for years, written by its authors; no formal governance, but no owner who can change it | No prose spec, no schema and no licence: a set of example files, one per version (1.2, 2.0, 3.0, 4.0; last published 2021), from the developer of one commercial app whose native `.fadein` file is OSF in a ZIP. An unofficial "2.1" fork renames every attribute |
| Versions in the wild | One | Five spellings of the same ideas; readers must accept all of them |
| Other software | Read and written by most screenwriting apps and many writing tools; plain text opens anywhere | Read natively by one commercial app; imported by a few open-source tools, which drop notes, tags and outline data on the way |
| Human readable | Yes: it reads as a screenplay | XML: readable with effort, a paragraph is four or more lines |
| git and diff | Line-based diffs of the actual words; merges are practical | Noisy diffs (attributes, runs split by style); merges by hand are risky |
| The Method suite today | Method Schedule imports Fountain and `.fdx`; Method Reader writes Fountain and `.fdx` | Neither reads OSF |
| Inside Storyteller | Default since 0.1 | Read and written since October 2026, `.fadein` too |

## 4. Migration if the default changes

- **Existing files are untouched.** The default only decides how a *new* script is first saved.
  A `.fountain` file stays Fountain on Save; Save As changes format.
- **Writers who live in Fountain** (git, plain-text tools, other apps) would see `.osf` files
  appear for new scripts and would need File > Save New Scripts as Fountain once, a setting
  that already exists.
- **The suite:** an OSF script cannot go straight to Method Schedule or come from Method Reader
  until they read OSF, so a writer would export Fountain or `.fdx` for them (or they gain an OSF
  reader).
- **Is "OSF as the working format, Fountain one click away" enough?** For drafting, nearly: the
  round trip above is exact apart from Shots and multi-paragraph speeches. For production
  drafts, no: exporting Fountain drops revisions, locks and settings, so "one click away" is a
  one-way export, not a second working format. And the outline (sections, synopses) is safest in
  Fountain, because other OSF readers print it.

## 5. Recommendation

**Keep Fountain as the default working format now; switch new scripts to OSF automatically when
a script first holds production data.** Concretely:

1. Fountain stays the default for new scripts (decision 2), and OSF stays one choice away.
2. When a writer turns on revision mode or locks pages (M5), Storyteller offers to keep the
   script as OSF from then on, because Fountain cannot hold that work. This is decision 1's
   recommendation, and it lands with M5.
3. Close the gaps this study found, whatever the default:
   - keep the Fountain boneyard on read and write it back (the model needs a place for it);
   - write a Shot to Fountain in a way that reads back as a Shot where possible, and warn when
     saving Shots or multi-paragraph speeches to Fountain;
   - map `.fdx` notes, revisions and locked pages into OSF (M5, M6), so OSF really is the
     production format;
   - write sections and synopses to OSF in a way other OSF readers do not print, or warn.

**Trade-offs.** Making OSF the default now would keep Shots and multi-paragraph speeches, and it
would put every new script in the format production work will need. Against that: the suite and
most writing tools do not read it, its diffs are poor, its spec is unlicensed example files from
one vendor, other OSF readers print our outline, and until M5 there is no production data for it
to hold. Revisit this if Method Schedule and Method Reader gain OSF readers, or if writers ask
for Shots and multi-paragraph speeches in drafts more than for plain-text files.
