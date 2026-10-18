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
   extensions**, offered when a writer first locks pages or turns on revision mode, with
   Fountain staying the drafting default. Needed before M5.
2. **Default working format for new scripts.** **Built on the recommendation: Fountain**, with
   Open Screenplay Format one menu choice away (File > Save New Scripts as…).
3. **Tab after text in a character cue.** Final Draft starts a parenthetical by default, with an
   option to open the extension list. **Built on the recommendation: parenthetical**; the
   extension option comes with stored SmartType lists. To confirm in the Final Draft 11 session.
4. **⌘0.** Final Draft uses it for its General element, which the model does not have; here it is
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

### Tests that need the owner's hands

- **The writing feel (M3).** Write a few pages from scratch: Enter, Tab, the element picker,
  SmartType, ⌘1–7 and ⌘⌥1–7. Say what feels wrong.
- **Navigation and editing (M4, once #18 to #21 are in).** In a real script: the scene lengths in
  the navigator against what you expect; ⌘⌥G to a scene number and a page; ⌘F, Enter and ⇧Enter,
  Replace, Replace All and one ⌘Z; Production > Rename Character with and without "Also in
  action, dialogue and other text".
- **The Final Draft 11 session (M7).** One scripted session settles the eight open questions in
  the V1 inventory: whether our `.fdx` exports open cleanly, encodings never seen filled in
  (alternates, tags in text, A-page locks), A-page and A-scene lettering edge cases, pagination
  cases the corpus lacks (dual dialogue at a break, scene CONTINUED, stacked CONT'D, line-spacing
  presets), any break threshold the PDFs leave ambiguous, the undocumented Tab and Shift-Tab
  keys, revision output details, and the outline elements of older versions. Prepared files
  and a click list come before the session.
- **Private corpus.** New FDX+PDF pairs straight out of Final Draft (same day, same draft) widen
  the ground truth. The harness never prints script text; the files stay on one machine.

### Release go-aheads

- **First release (v0.2.0)**, once the release workflow lands: the owner confirms the signing
  and notarization secrets, checks the draft `.dmg`, and publishes. Then the README platform
  table and the website's status change.

---

## Where we are

**[have]**
- Fountain, `.fdx`, Open Screenplay Format (`.osf`, `.fadein`) read and write, keeping
  everything the model does not use; PDF export.
- Pagination matched to Final Draft: every scene of the 15 ground-truth scripts on the same page
  (87% across 108 real files); scene page and length in eighths written to `.fdx`.
- The macOS menu bar, a unified title bar, page view and draft view.
- M3, the writing feel: Enter/Tab flow, element picker, ⌘1–7 and ⌘⌥1–7, status-bar hint,
  automatic (CONT'D), SmartType times and next-speaker guess, smart quotes.

- The suite standards: identity, the Help menu, repo hygiene with the 14-day crate-age check,
  the release workflow, the brand (docs/BRAND.md).

**In review** (each stacked on the one before, merged in this order)
- Pictures: `cargo xtask screens`, drawn offscreen from the Field Station sample
  ([#17](https://github.com/method-production-suite/method-storyteller/pull/17)).
- M4: scene lengths in the navigator ([#18](https://github.com/method-production-suite/method-storyteller/pull/18)),
  Go To ([#19](https://github.com/method-production-suite/method-storyteller/pull/19)), find and
  replace ([#20](https://github.com/method-production-suite/method-storyteller/pull/20)), rename
  character ([#21](https://github.com/method-production-suite/method-storyteller/pull/21)).

---

## Next, in order

### Suite standards (SUITE-STANDARDS.md)
1. **Help menu** [have] S. About, User Guide, Keyboard Shortcuts, What's New, Report a Bug,
   Licenses; Check for Updates wired, live once `method-update` lands in method-libs.
2. **Updates** [missing] S. Wire `method-update` when it lands: check 2 minutes after launch and
   every 6 hours, "Update ready", an off switch.
3. **Repo hygiene** [have] S. `deny.toml` and the 14-day crate-age check in CI; Actions pinned
   to the suite SHAs.
4. **Release workflow** [have] M. Draft releases, a signed and notarized Apple
   silicon `.dmg`, Windows and Linux installers, version-free names.
5. **Brand** [have] S. docs/BRAND.md, the provisional mark, the app icon.
6. **Pictures** [partial, in review] M. A capture command retakes every README picture from an invented
   Field Station script, 1280×800, light and dark.

### V1 milestones
7. **M4, notes and navigation** [partial, in review] M. In review: scene lengths in the
   navigator, Go To (page, scene), find and replace, rename a character. Still to do: spell check
   (decision 6), script notes (range, colour, author), bookmarks for Go To, element-preserving
   copy and paste.
8. **M5, production I** L. Scene numbering with A-suffix inserts, omit and unomit, revision
   sets (name, mark, colour), revision mode and marks, revised-pages header. Needs decision 1.
9. **M6, production II** L. Lock pages and A-pages, revised-pages-only PDF, scene, character,
   location and cast reports with CSV, the structured export for Method Scheduling.
10. **M7, release** M. The Final Draft 11 session, then V1.0.

### Smaller items
- **Page view** [partial] S. A speech split across pages should continue on the next sheet
  instead of lengthening the first.
- **Imports** [missing] S each. `.highland` (a ZIP holding Fountain), PDF (from Method Reader's
  crates once relicensed), Celtx.
- **Stored SmartType lists** [missing] M. Lists kept in the document, learned as you type,
  editable; the extension-on-Tab option.
- **Scene CONTINUED** [missing] S. Off by default; measured only if a corpus file turns it on.

---

## Later

Beat board and outline editor (their `.fdx` data already round-trips), right-to-left page
layout, iPad, real-time collaboration, watermarked PDFs, breakdown tagging, writing goals and
focus modes.
