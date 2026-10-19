# Roadmap

What to build next, in order, so writers can move real scripts in, write the way they expect,
and take a script through production. The V1.0 feature set and its evidence are in
[research/v1-feature-inventory.md](research/v1-feature-inventory.md); the page rules in
[spec.md](spec.md) §5 and [research/pagination-calibration.md](research/pagination-calibration.md).

Status: **[have]** built, **[partial]** some of it built, **[missing]** not started.
Size: **S** a day or two, **M** about a week, **L** more than a week.

---

## Waiting on the owner

Read every Monday for the owner's digest. Each decision shows the recommendation; where the
choice is reversible, it has been **built on the recommendation** and stays unless the owner
says otherwise.

### Decisions

1. **Where production data lives** (revisions, locked pages, tags). Fountain cannot hold it;
   Open Screenplay Format holds more (revision marks, locked page numbers) but has no revision
   dates or tag placement. Options: OSF with `method_` extension data as the production format;
   Fountain plus a small sidecar file; a package format of our own. **Recommend: OSF with
   extensions**: OSF is now the standard (decision 2), so production data goes there. Needed
   before M5.
2. **Default working format for new scripts.** **Decided by the owner, 2026-10-18: Open
   Screenplay Format is the standard and the default for new scripts.** Fountain stays fully
   supported: open, Save As, File > Save New Scripts as Fountain, and copy as Fountain. Existing
   files keep their format on Save. The study is [research/osf-vs-fountain.md](research/osf-vs-fountain.md);
   its gaps are closed first (below).
3. **Tab after text in a character cue.** The usual convention starts a parenthetical, with an
   option to open the extension list. **Built on the recommendation: parenthetical**; the
   extension option comes with stored autocomplete lists. To confirm in the parity check (M7).
4. **⌘0.** Some screenwriting apps use it for a General element, which the model does not have; here it is
   Actual Size. **Built on the recommendation: keep Actual Size.**
5. **"Paginate like…" profiles for other apps.** Only one app's pagination is calibrated against
   real output. **Recommend: after V1**, starting with the (MORE)/(CONT'D) text and split
   behaviour, which are cheap and documented.
6. **Spell check engine and dictionary.** Options: the operating system's own checker (native on
   macOS and Windows, nothing on Linux, three code paths); or one pure-Rust Hunspell-compatible
   checker (`spellbook`, MPL-2.0, allowed by `deny.toml`) with a bundled English dictionary from
   SCOWL/wordlist.aspell.net (permissive licence, attributed in ATTRIBUTION.md), the same on every
   platform and testable offscreen. **Recommend: spellbook with the SCOWL en_US dictionary**,
   other languages as downloadable dictionaries later; ignore lists kept in the document (an
   `.fdx` already carries them). Reversible; built on this unless the owner says otherwise.

7. **Script notes in Fountain.** Fountain has `[[notes]]` inline; they cannot carry a colour,
   an author or a range. Options: keep Fountain notes as plain `[[…]]` and keep full notes only in
   OSF and `.fdx`; or add a `method_` convention inside the note text. **Recommend: plain `[[…]]`
   in Fountain**, full notes in OSF and `.fdx`, and a one-line warning when saving notes with
   colours to Fountain. Changes what a Fountain file holds, so it waits for the owner.
8. **The title page.** `.fdx` stores the title page as free layout (aligned paragraphs, tab
   stops); Fountain and the app use fields. Options: keep fields and lay them out the industry
   way; or add a free-layout title page editor. **Recommend: fields for V1**, an imported free
   layout kept and printed as it came, a layout editor after V1. Changes how writers work, so it
   waits for the owner.
9. **Copy and paste between apps.** Inside Storyteller a copy keeps element types. **Recommend:
   also put Fountain text on the clipboard**, so a paste into any text editor or another
   screenwriting app keeps the screenplay's shape. Reversible; built on this.
10. **The final mark (design pass).** The provisional mark was a page outline with the script
   at half strength. **Built on the recommendation: a solid page with the script, the brads
   and a cursor cut out of it**, the language of Method Budget's mark (figures cut out of its
   page), and an app icon of the white mark on a rose square like Method Budget's and Method
   Schedule's. Reversible: the drawing is one function in `scripts/make-brand.py`. Pictures
   are in the brand pull request.
11. **`unsafe` for files opened from the Finder.** **Decided by the owner, 2026-10-18:**
   `crates/macos-open` is the one crate allowed `unsafe`, with a SAFETY comment on every block
   (enforced by clippy). Recorded in AGENTS.md.
12. **Rank for `.osf` in the Finder.** **Decided by the owner, 2026-10-18: Default**, like
   `.fountain` and `.fdx`: open formats, so the app claims none of them as their owner.

### Tests that need the owner's hands

- **The writing feel (M3).** Write a few pages from scratch: Enter, Tab, the element picker,
  autocomplete, ⌘1–7 and ⌘⌥1–7. Say what feels wrong.
- **Navigation and editing (M4).** In a real script: the scene lengths in
  the navigator against what you expect; ⌘⌥G to a scene number and a page; ⌘F, Enter and ⇧Enter,
  Replace, Replace All and one ⌘Z; Production > Rename Character with and without "Also in
  action, dialogue and other text".
- **Lossless `.fdx` (the final check).** Open `crates/core/tests/fixtures/lossless-check.fdx`
  in your licensed copy and go down the nine-step click list in
  [research/fdx-osf-fidelity.md](research/fdx-osf-fidelity.md#the-owners-final-check): notes,
  revisions, locked pages and element settings, then save it and send it back. About 20 minutes,
  nothing to type. Ready once the milestone's PRs (#38, #40, #42, #43 and the report) have merged.
- **The parity check (M7).** One scripted session with a licensed reference app settles the eight open questions in
  the V1 inventory: whether our `.fdx` exports open cleanly, encodings never seen filled in
  (alternates, tags in text, A-page locks), A-page and A-scene lettering edge cases, pagination
  cases the corpus lacks (dual dialogue at a break, scene CONTINUED, stacked CONT'D, line-spacing
  presets), any break threshold the PDFs leave ambiguous, the undocumented Tab and Shift-Tab
  keys, revision output details, and the outline elements of older versions. Prepared files
  and a click list come before the session.
- **The design pass (D).** Look at the offscreen pictures of the icon, the editor and every panel
  in light and dark, and say what feels off. Nothing needs the app open on your screen.
- **Private corpus.** New `.fdx` and PDF pairs exported together (same day, same draft) widen
  the ground truth. The harness never prints script text; the files stay on one machine.

### Release go-aheads

- **First release (v0.2.0)**, once the release workflow publishes the updater files: the signing,
  notarization and update keys are in place (2026-10-18); the owner checks the draft `.dmg` and
  `latest.json`, and publishes. Then the README platform
  table and the website's status change.

---

## Where we are

**[have]**
- Fountain, `.fdx`, Open Screenplay Format (`.osf`, `.fadein`) read and write, keeping
  everything the model does not use; PDF export.
- Pagination matched to real production PDFs: every scene of the 15 ground-truth scripts on the same page
  (87% across 108 real files); scene page and length in eighths written to `.fdx`.
- The macOS menu bar, a unified title bar, page view and draft view.
- M3, the writing feel: Enter/Tab flow, element picker, ⌘1–7 and ⌘⌥1–7, status-bar hint,
  automatic (CONT'D), autocomplete times and next-speaker guess, smart quotes.

- The suite standards: identity, the Help menu, repo hygiene with the 14-day crate-age check,
  the release workflow, the brand (docs/BRAND.md), and pictures that retake themselves
  (`cargo xtask screens`, drawn offscreen from the Field Station sample).
- M4 so far: scene lengths in the navigator, Go To (scene or page), find and replace with one
  Undo for Replace All, rename a character.

**In review**
- The updater, `method-update`, wired and inactive until Storyteller has an updater key
  ([#23](https://github.com/method-production-suite/method-storyteller/pull/23)).
- The methodstoryteller.com page, `site/` ([#24](https://github.com/method-production-suite/method-storyteller/pull/24)).

---

## Next, in order

### Suite standards (SUITE-STANDARDS.md)
1. **Help menu** [have] S. About, User Guide, Keyboard Shortcuts, What's New, Report a Bug,
   Licenses, Check for Updates.
2. **Updates** [partial, in review] S. `method-update` wired: a check 2 minutes after launch and
   every 6 hours, Restart to Update, an off switch. Live once Storyteller has an updater key and
   the release publishes `latest.json`.
3. **Repo hygiene** [have] S. `deny.toml` and the 14-day crate-age check in CI; Actions pinned
   to the suite SHAs.
4. **Release workflow** [have] M. Draft releases, a signed and notarized Apple
   silicon `.dmg`, Windows and Linux installers, version-free names.
5. **Brand** [have] S. docs/BRAND.md, the mark, the app icon.
6. **Pictures** [have] M. A capture command retakes every README picture from an invented
   Field Station script, 1280×800, light and dark.
7. **Method Launcher rules** (§2, §3, §6, adopted 2026-10-18) [missing] S each, after v0.2.0:
   - every release attaches exactly one `SHA256SUMS` listing every downloadable asset (in the
     release workflow before the first release);
   - the app accepts `--check-for-updates` and `--open-sample`;
   - `recent.json` in the data folder, `[{"path": "/abs", "opened": "<RFC 3339>"}]`, newest
     first, replacing the recent list in settings;
   - installed names: the Linux package `method-storyteller`, the Windows uninstall name
     `Method Storyteller`;
   - the whole-lock crate-age check on every push to main and weekly [in review].

### V1 milestones, worked continuously

One small PR per item, each to MReview, the next item started without waiting for the merge.
Items that touch separate files run side by side in their own sessions; items marked *owner*
wait for a decision above.

7. **M4, notes and navigation** [partial] M. Done: scene lengths in the navigator, Go To (page,
   scene), find and replace, rename a character. Next, in order: spell check (decision 6, waits
   on the dictionary's licence check), element-preserving copy and paste (decision 9), script
   notes on a line or a range with colour and author in all three `.fdx` generations and OSF
   (decision 7 for Fountain), scene titles, summaries and colours, drag to reorder scenes in
   the navigator (one Undo), bookmarks for Go To, the character table, show and hide outline
   paragraphs.
8. **Lossless `.fdx`** [in review] M (owner, 2026-10-18). Everything `.fdx` and Open Screenplay
   Format can both hold converts exactly both ways (OSF to `.fdx`, `.fdx` to OSF, `.fdx` to
   `.fdx`), proven by round-trip tests on files we write ourselves. Today an imported `.fdx`
   keeps everything only when saved back as `.fdx`. One PR each, in order (all in review; the
   fidelity report lists what maps exactly and what cannot):
   - script notes: the three `.fdx` generations (a note inside its paragraph, the older
     unanchored list, the newer list with character ranges) into the model with range, colour
     and author, to OSF `note` and back (colour and author in a `method_storyteller` child, so
     our own round trip is exact);
   - revisions: revision sets (name, colour, mark) and each run's revision, to OSF
     `revision_colors` and `text@revision`, and back;
   - locked pages and A-pages: locked pages into the model, to OSF `pages_locked` and each
     paragraph's `page_number`, and back;
   - element settings and page layout, to OSF `styles` and `settings` and back, through the
     mapping layout.rs already has;
   - the fidelity report, `docs/research/fdx-osf-fidelity.md`: what maps exactly, what is
     approximated, what cannot map and why, checked by a test;
   - the final check with the owner (above).
9. **M5, production I** L. Full scene numbering (A-suffix inserts, prefix schemes, left and
   right), omit and unomit, revision sets (name, mark, colour), revision mode, mark and clear,
   deleted-text markers, revisions drawn in the editor and the PDF, the revised-pages header,
   revised-pages-only and range PDFs, clean copies. Needs decision 1.
10. **M6, production II and reports** L. Locked pages as imported, then lock pages, A-pages, lock
   and unlock; scene, location, character and cast reports with CSV; plain-text export; the
   structured export Method Schedule reads.
11. **D, the design pass** M (new, owner 2026-10-18). Matched to the family (Method Budget's and
    Method Schedule's BRAND.md), every step pictured offscreen in light and dark:
    - the final mark and wordmark (replacing the provisional ones in docs/brand and site/);
    - the app icon at every macOS size, plus Windows and Linux, and a document icon for
      `.fountain`, `.osf` and `.fdx`;
    - the type scale, spacing and colour of the editor, the navigator, the status bar, the find
      bar, the dialogs and the Help windows, as tokens in theme.rs that BRAND.md lists;
    - dark mode checked surface by surface, contrast measured as in BRAND.md;
    - empty states (no script, no scenes, nothing found) and error states (a file that will not
      open, a failed save, a failed update) written in the family voice.
    Runs beside M4 to M6: it touches theme, icons and copy, not the engine.
12. **M7, parity check, then V1.0** M. The parity check (the eight prepared checks in one sitting,
    each answer a committed golden test), the fixes it finds, then V1.0.

**Parallel tracks while M4 runs:** the release workflow's updater files (`latest.json` and the
signed `.app.tar.gz`), the OSF and Fountain study, and the design pass. Each is its own PR.

### Smaller items
- **Page view** [partial] S. A speech split across pages should continue on the next sheet
  instead of lengthening the first.
- **Imports** [missing] S each. `.highland` (a ZIP holding Fountain), PDF (from Method Reader's
  crates once relicensed), `.celtx`.
- **Stored autocomplete lists** [missing] M. Lists kept in the document, learned as you type,
  editable; the extension-on-Tab option.
- **Scene CONTINUED** [missing] S. Off by default; measured only if a corpus file turns it on.
- **File > Open Recent and Close** [missing] S. Open Recent needs a menu built at run time (the
  menu is fixed today); Close needs a decision on what it does in a one-window app (an empty
  script, or the window). From the screen test.
- **Recovery copies at launch** [missing] S. The app writes a recovery copy as the writer works
  but never offers it after a crash. Offer it at launch, ahead of the first-launch sample.
- **The editor lays out only the rows on screen** [missing] M. Every paragraph is laid out
  each frame today; long scripts will feel it first in the spelling underline and find marks.
- **British English dictionary** [missing] S. Field Station is written with British spellings
  (grey, colour, tyres); the bundled dictionary is US English only.

---

## Later

Beat board and outline editor (their `.fdx` data already round-trips), right-to-left page
layout, iPad, real-time collaboration, watermarked PDFs, breakdown tagging, writing goals and
focus modes.
