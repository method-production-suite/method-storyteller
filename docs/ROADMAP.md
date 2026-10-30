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
   Fountain plus a small sidecar file; a package format of our own. **Decided by the owner,
   2026-10-19: a package format of our own, `.mstory`**: one ZIP file holding the script as Open
   Screenplay Format and a production data file (revisions with their dates and who made them,
   locked pages, tags), so it works the same on every system, travels as one file, and the script
   inside stays readable by other apps. Unblocks M5.
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
   OSF and `.fdx`; or add a `method_` convention inside the note text. **Decided by the owner,
   2026-10-19: plain `[[…]]` in Fountain**, full notes in OSF and `.fdx`; saving to Fountain
   already warns what it leaves out.
8. **The title page.** `.fdx` stores the title page as free layout (aligned paragraphs, tab
   stops); Fountain and the app use fields. Options: keep fields and lay them out the industry
   way; or add a free-layout title page editor. **Decided by the owner, 2026-10-19: fields for
   V1**, an imported free layout kept and printed as it came, a layout editor after V1.
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
13. **TV structure (new, 2026-10-19).** **Decided by the owner: build it all** (act breaks,
   End of Act, Teaser, Cold Open and Tag, a New TV Script layout, acts in the navigator and Go
   To, and acts in every format). A new TV script asks each time: one-hour (Teaser, Acts One to
   Five, Tag) or half-hour (Cold Open, Acts One to Three, Tag), remembering the last choice.
14. **What new scripts save as, once `.mstory` exists.** **Decided by the owner, 2026-10-19:
   `.mstory`**, so revisions, locks and tags always have a home; Open Screenplay Format and
   Fountain one Save As away. (Until the package lands, new scripts stay `.osf`.)
15. **File > Close (⌘W).** **Decided by the owner, 2026-10-19: close the script**, asking about
   unsaved changes, and leave an empty untitled script in the window; the app keeps running.
16. **Spelling dictionaries.** **Decided by the owner, 2026-10-19: US and UK English, a
   setting**, the first run choosing from the system's region.
17. **TV scene numbering.** **Decided by the owner, 2026-10-19, on industry practice:** one-hour
   and single-camera scripts number scenes straight through the episode (acts do not reset the
   count). Multi-camera sitcoms letter their scenes (SCENE A, SCENE B), each on a new page with
   its characters under the heading, dialogue double-spaced and action in capitals: a third
   shape for New TV Script, built after the TV basics.
18. **End of Act lines.** **Decided by the owner, 2026-10-19: written automatically and kept in
   step** with their act's name (renaming ACT TWO updates END OF ACT TWO); a typed one of the
   writer's own (END OF SHOW) is left alone.
19. **Signing.** **Decided by the owner, 2026-10-19:** Windows installers stay unsigned for now
   (revisit before V1); the macOS signing identity stays as it is.
20. **Scene number letters I and O** ([production-spec.md](production-spec.md), S1). **Built on
   the recommendation: skip neither by default**, with a setting to skip both, until the parity
   check shows current practice. Reversible.
21. **Mark and Clear Revised on ⌘] and ⌘[** (S2). **Built on the recommendation: yes**; nothing
   in the app takes those keys. Reversible.
22. **A-page lettering past Z, and merged locked pages** (S3). **Built on the recommendation:
   AA to ZZ, and a merged page numbered `7-8`**; the parity check settles anything a saved file
   shows differently. Reversible.
23. **The Method Schedule export format** (S4). **Agreed with the Method Schedule session,
   2026-10-20:** a script scenes file (`.mscenes`, JSON, `method-script-scenes` version 1),
   specified in [production-spec.md](production-spec.md) section 4.4 and in Method Schedule's
   EXCHANGE.md. Built on that agreement; reversible until the writer ships.
24. **Show Revisions as three menu items** (§3.5). The spec has one View > Show Revisions
    submenu; the menus have no submenus yet. **Built on the recommendation: three checked
    items** (Show All Revision Sets, Show Active Revision Set, Hide Revisions), moved into a
    submenu if the menus gain them.
25. **Combining Export PDF choices** (§3.6). **Built on the recommendation: each choice
    narrows the pages** (a page range of revised pages only, say); the title page prints only
    with all pages. Through the control interface the choices combine; the dialog offers one
    at a time.
12. **Rank for `.osf` in the Finder.** **Decided by the owner, 2026-10-18: Default**, like
   `.fountain` and `.fdx`: open formats, so the app claims none of them as their owner.

### Tests that need the owner's hands

- **Linux, by a person (eventually).** Each release builds, unit-tests and packages it in CI;
  nobody has yet installed and used it. When there is a machine: install the `.deb` or the
  AppImage, open the Field Station sample, type a scene, save, reopen.

- **The writing feel (M3).** Write a few pages from scratch: Enter, Tab, the element picker,
  autocomplete, ⌘1–7 and ⌘⌥1–7. Say what feels wrong.
- **Navigation and editing (M4).** In a real script: the scene lengths in
  the navigator against what you expect; ⌘⌥G to a scene number and a page; ⌘F, Enter and ⇧Enter,
  Replace, Replace All and one ⌘Z; Production > Rename Character with and without "Also in
  action, dialogue and other text".
- **Lossless `.fdx` (the final check).** Open `crates/core/tests/fixtures/lossless-check.fdx`
  in your licensed copy and go down the ten-step click list in
  [research/fdx-osf-fidelity.md](research/fdx-osf-fidelity.md#the-owners-final-check): notes,
  revisions, locked pages and element settings, then save it and send it back. About 20 minutes,
  nothing to type. Ready now: the whole milestone is on main.
- **Opening files from the Finder (macOS).** Needs the app on screen, so it waits for your yes
  through MOrg. With a build of `main`: double-click an `.osf`, a `.fountain` and an `.fdx`, and
  drop one on the Dock icon. Each should open in the app, and on a first launch the file should
  open instead of the Field Station sample. About 5 minutes; nothing is installed for good (the
  build is registered with the Finder for the test and unregistered after).
- **The parity check (M7).** Also: the shape a scene's Summary takes in a current `.fdx` (the
  corpus has only plain text, which the app writes; paragraphs are read too). One scripted session with a licensed reference app settles the eight open questions in
  the V1 inventory: whether our `.fdx` exports open cleanly, encodings never seen filled in
  (alternates, tags in text, A-page locks), A-page and A-scene lettering edge cases, pagination
  cases the corpus lacks (dual dialogue at a break, scene CONTINUED, stacked CONT'D, line-spacing
  presets), any break threshold the PDFs leave ambiguous, the undocumented Tab and Shift-Tab
  keys, revision output details, and the outline elements of older versions. A ninth (added
  2026-10-28): how a locked page's `Position` counts, from a small locked script saved after
  each of a few known edits ([research/locked-pages.md](research/locked-pages.md)); until then
  locks read from `.fdx` are kept but paginate as unlocked. Prepared files and a click list
  come before the session.
- **The design pass (D).** Look at the contact sheet ([design-pass.md](design-pass.md), section 3)
  once it is drawn, or for now the offscreen pictures of the icon, the editor and every panel
  in light and dark, and say what feels off. Nothing needs the app open on your screen.
- **Private corpus.** New `.fdx` and PDF pairs exported together (same day, same draft) widen
  the ground truth. The harness never prints script text; the files stay on one machine.

### Release go-aheads

- **v0.2.0, the first release:** published 2026-10-19 (tag `v0.2.0` on `33cfb16`, the owner's
  override for muda 0.21.2's last three days). Nothing waiting; signing stays as it is
  (decision 19).
- **v0.3.0:** published 2026-10-20 (tag `v0.3.0` on `53a37c5`): TV acts, and one universal
  Mac app for Apple silicon and Intel. Nothing waiting.
- **v0.4.0:** published 2026-10-20 (tag `v0.4.0` on `a40de7c`): TV structure finished, bold,
  italic and underline, signed checksums.
- **v0.5.0, the security release:** the update checks' TLS fixes (RUSTSEC-2026-0382 and -0381),
  with the `.mstory` package, scene properties, US and UK spelling, recovery at launch, Close
  and Open Recent. Called 0.5.0, not 0.4.1, because a tag must be on main and main already
  carries those features. Published 2026-10-21 (tag `v0.5.0` on `0b861b8`).
- **v0.6.0, the production-draft release:** scene numbering that keeps numbers, margins for
  them, omitted scenes, revision sets, revision mode, revisions on screen and in print, and
  issuing chosen pages. May tag before 2026-11-06: the owner approved clipboard-win 5.4.3 for
  release v0.6.0 (2026-10-27), and the release check honours that override for the tag v0.6.0
  only. Published 2026-10-28 (tag `v0.6.0` on `4f7198f`, the owner's yes through MOrg; draft
  checked: checksums, both signatures, notarisation and Gatekeeper). Nothing waiting; the
  override line comes out on 2026-11-06.

### Deferred (the owner, 2026-10-20)

- **Windows, by a person: deferred indefinitely.** Windows keeps being built, unit-tested and
  packaged in CI and in every release, and its notes keep saying it is not yet tested by a
  person. Nothing waits on it.
---

## Where we are

**[have]**, all on `main` and in the first release, 0.2.0 (2026-10-19):
- Fountain (the boneyard kept), `.fdx` and Open Screenplay Format (`.osf`, `.fadein`, the
  default for new scripts) read and write, keeping everything the model does not use; PDF
  export with a bookmark per scene.
- Lossless `.fdx`: script notes, revisions, locked pages and A-pages, element settings and page
  layout, and the spelling ignore list carry over between `.fdx` and OSF both ways; the
  private corpus (108 files) round-trips identically, and the fidelity report lists what maps.
- Pagination matched to real production PDFs: every scene of the 15 ground-truth scripts on
  the same page (87% across 108 real files); scene page and length in eighths written to `.fdx`.
- M3, the writing feel: Enter/Tab flow, element picker, ⌘1–7 and ⌘⌥1–7, status-bar hint,
  automatic (CONT'D), autocomplete, smart quotes; INT./EXT. typed fast or pasted becomes a
  heading.
- M4 so far: scene lengths in the navigator, a navigator click lands in the scene's action, Go
  To (scene or page), find and replace with one Undo for Replace All, rename a character, spell
  check (underlines and Edit > Spelling…), script notes in all three `.fdx` forms and OSF.
- The Mac app: the menu bar, Cut/Copy/Paste/Select All, the Help menu, the first-launch
  sample, Finder double-click with document icons for `.osf`, `.fountain` and `.fdx`, the
  updater (live with the first release), `--open-sample` and `--check-for-updates`.
- The suite standards: identity, repo hygiene (cargo deny, the 14-day crate-age check over the
  whole lock), the release workflow with one `SHA256SUMS`, the updater key check, the brand,
  pictures that retake themselves, the methodstoryteller.com page, and CI the ArtCraft way
  (dependency checks on pull requests, a weekly full build).

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
7. **Method Launcher rules** (§2, §3, §6, adopted 2026-10-18) [partial] S each:
   - every release attaches exactly one `SHA256SUMS` listing every downloadable asset [have];
   - the app accepts `--check-for-updates` and `--open-sample` [have];
   - `recent.json` in the data folder, `[{"path": "/abs", "opened": "<RFC 3339>"}]`, newest
     first, replacing the recent list in settings;
   - installed names: the Linux package `method-storyteller`, the Windows uninstall name
     `Method Storyteller`;
   - the whole-lock crate-age check on every push to main and weekly [have].

### V1 milestones, worked continuously

One small PR per item, each to MReview, the next item started without waiting for the merge.
Items that touch separate files run side by side in their own sessions; items marked *owner*
wait for a decision above.

7. **M4, notes and navigation** [partial] M. Done: scene lengths in the navigator, a click
   into the scene's action, Go To (page, scene), find and replace, rename a character, spell
   check, script notes in all three `.fdx` generations and OSF, Copy Scene and pasting
   screenplay text as elements (decision 9, first half), scene titles, summaries and colours,
   the character table. Next, in order: a selection across paragraphs, so any range copies and
   cuts with its element types (decision 9, second half), drag to reorder scenes in the
   navigator (one Undo), bookmarks for Go To, show
   and hide outline paragraphs.
8. **Lossless `.fdx`** [have, the owner's check to come] M (owner, 2026-10-18). Everything `.fdx` and Open Screenplay
   Format can both hold converts exactly both ways (OSF to `.fdx`, `.fdx` to OSF, `.fdx` to
   `.fdx`), proven by round-trip tests on files we write ourselves. Today an imported `.fdx`
   keeps everything only when saved back as `.fdx`. One PR each, in order (all in review; the
   fidelity report lists what maps exactly and what cannot; all merged 2026-10-19):
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
8a. **TV structure** [have] M (owner, 2026-10-19; decision 13). One PR each, in order:
   - New Act and End of Act as element types (caps, centred, a new act starts a new page),
     read and written as `.fdx` New Act and End of Act, OSF styles, and centred Fountain lines;
   - New TV Script: Teaser or Cold Open, the acts and Tag laid out, one-hour or half-hour
     [have];
   - acts in the navigator (each act heading its scenes) and in Go To [have];
   - End of Act lines written and kept in step automatically (decision 18) [have];
   - a multi-camera sitcom shape: lettered scenes, each on a new page with its characters,
     double-spaced dialogue, action in capitals (decision 17) [have].
8b. **Bold, italic and underline in the editor** [have] S (owner, 2026-10-19). Format > Bold,
   Italic and Underline (⌘B, ⌘I, ⌘U) on a selection or the caret's word, and the editor drawing
   the styles with the markup faint. Hiding the markers altogether waits for the editor's own
   text drawing (with capitals by display, below).
8c. **The `.mstory` package** [have] M (decision 1). One ZIP file: the script as Open
   Screenplay Format, and a production data file. Open and save, File > Save As, the Finder's
   document type and icon. Comes first in M5.
9. **M5, production I** L. Spec: [production-spec.md](production-spec.md), section 3, with its
   order of PRs. Done: scene numbering (letter inserts, start, from the caret, I and O, edit one
   number), omit and unomit, revision sets, revision mode with Mark and Clear Revised and
   deletion marks, revisions drawn in the editor and the PDF with the revised-page header and
   page colours, File > Export PDF… with revised pages only, page and scene ranges, clean
   copies and coloured pages, scene numbers in the left or right margin, new paragraphs and joins
   marked, revision mode kept in the file. M5 is done; kept in the `.mstory` package (decision 1).
10. **M6, production II and reports** L. Spec: [production-spec.md](production-spec.md),
   section 4. Done: pagination keeps locked pages (A-pages, short pages, pages printed
   together as `7-8`, pages starting inside a paragraph or a speech), for locks made here and
   OSF's; locks read from `.fdx` wait on how their positions count
   ([research/locked-pages.md](research/locked-pages.md)); Lock Pages, Lock A-Pages, Unlock
   A-Pages and Unlock All Pages. Next: scene,
   location, character and cast reports with CSV; plain-text export; the structured export
   Method Schedule reads.
11. **D, the design pass** M (new, owner 2026-10-18). What is done and what is left:
   [design-pass.md](design-pass.md). Matched to the family (Method Budget's and
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
- **File > Open Recent and Close** [have] S. Open Recent needs a menu built at run time (the
  menu is fixed today); Close (⌘W) closes the script and leaves an empty one (decision 15).
- **Recovery copies at launch** [have] S. The app writes a recovery copy as the writer works
  but never offers it after a crash. Offer it at launch, ahead of the first-launch sample.
- **The editor lays out only the rows on screen** [missing] M. Every paragraph is laid out
  each frame today; long scripts will feel it first in the spelling underline and find marks.
- **Capitals by display** [missing] S. Typing into a capitals type (a multi-camera script's
  action) stores capitals today; keep the writer's casing and show capitals on screen and page
  instead, once the editor draws a capitals type itself (MReview's note on #82).
- **UK English dictionary** [have] S (decision 16). US and UK English, a setting, the first
  run choosing from the system's region.

---

## Later

Beat board and outline editor (their `.fdx` data already round-trips), right-to-left page
layout, iPad, real-time collaboration, watermarked PDFs, breakdown tagging, writing goals and
focus modes.
