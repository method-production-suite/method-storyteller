# Method Storyteller: architecture

## 1. Workspace and layers

A Cargo workspace (edition 2024, resolver 3, `members = ["crates/*"]`). Lower layers never depend
on higher ones; nothing below L3 depends on egui/eframe/winit/rfd. `cargo xtask layers` checks it.

| Layer | Crate | Purpose | Key deps |
| --- | --- | --- | --- |
| L0 | `storyteller-core` | Document model, Fountain parse/write, FDX import/export, pagination, autocomplete | `serde`, `quick-xml`, `thiserror` |
| L1 | `storyteller-pdf` | Paginated pages → PDF with embedded Courier Prime | `storyteller-core`, `craft-pdf` (write) |
| L2 | `storyteller-engine` | Session (script, path, dirty, undo), command registry, flow rules, control protocol | core, pdf, `craft-atomic`, `serde_json` |
| L3 | `storyteller-cli` (bin `method-storyteller`, deprecated alias `mst`) | Convert, paginate, export, run commands, headless control server | engine |
| L0 | `storyteller-macos-open` | macOS: files opened from the Finder (the open documents Apple Event). The only crate allowed `unsafe`; does nothing on other systems | `objc2`, `objc2-foundation` (macOS only) |
| L3 | `method-storyteller` (bin `method-storyteller-app`) | egui/eframe desktop app | engine, macos-open, `eframe`, `rfd`, `craft-settings`, `craft-quit` |
| tool | `xtask` | `ci`, `layers`, `assets`, `privacy`; `corpus` (local only: measures import and pagination against private Final Draft files) | none |

`storyteller-core` builds for `wasm32-unknown-unknown` (no filesystem, no threads), so a web
version or a FilmCraft integration can reuse it.

### craft-libs

Git dependencies on `https://github.com/storytold/craft-libs` pinned to one full `rev`, plus the
root `[patch.crates-io] krilla = { git = …, rev = <same> }` that `craft-pdf` requires (Cargo
ignores dependency-local patches). Used now: `craft-pdf` (write + simple-text), `craft-atomic`,
`craft-settings`, `craft-quit`. Planned: `craft-ui` (docking, icon buttons) when the UI grows
panels, `craft-print` for printing.

## 2. Document model (`storyteller-core::model`)

```rust
pub struct Script {
    pub title_page: TitlePage,          // ordered (key, value) pairs, Fountain keys
    pub elements: Vec<Element>,
    pub settings: ScriptSettings,       // show scene numbers, lines per page …
}
pub struct Element {
    pub kind: ElementKind,              // SceneHeading, Action, Character, Parenthetical,
                                        // Dialogue, Transition, Shot, Centered, Lyrics,
                                        // Section{level}, Synopsis, PageBreak
    pub text: String,                   // Fountain inline markup (emphasis, notes) kept verbatim
    pub scene_number: Option<String>,   // scene headings only
    pub dual: bool,                     // second cue of a dual-dialogue pair
}
```

- Flat element list (like Final Draft paragraphs). Dialogue blocks are derived, not stored, so
  editing never has to restructure a tree.
- `inline::parse(&str) -> Vec<Run>` turns emphasis markup into styled runs for PDF/FDX, and
  `inline::write(&[Run]) -> String` turns FDX styles back into markup. Notes `[[…]]` are runs
  with `note = true` and are dropped from print.
- All types are `serde` so the control channel can return them.

## 3. Formats

### Fountain (`fountain.rs`)
A line-oriented parser written from the fountain.io spec: title page, forced elements
(`.` `!` `@` `>` `~` `=` `#`), scene numbers `#…#`, character extensions and `^` dual dialogue,
centered text, `===`, boneyard `/* */` (skipped), notes kept inline. The writer emits canonical
Fountain and forces an element (`.`, `!`, `@`, `>`) exactly when the plain form would parse as
something else, which is what makes `parse(write(x)) == x` hold.

### FDX (`fdx.rs`)
`quick-xml` streaming reader (no DOM, no entity expansion beyond the XML builtins, depth and
size caps). Reads `FinalDraft/Content/Paragraph[@Type]` with `Text[@Style]` runs, `Number`,
`DualDialogue`, and `TitlePage/Content/Paragraph[@Alignment]`; everything else is skipped.
Writes a minimal, valid document with the same structure. Title page export uses a fixed layout
(centred title/credit/author/source, left contact, right draft date) that the importer maps
back to keys.

## 4. Pagination (`paginate.rs`)

Pure function `paginate(&Script, &Layout) -> Paginated`.

1. **Measure:** each element → `Block { kind, lines: Vec<LayoutLine>, space_before, breakable }`,
   wrapping with the element's column width (`wrap.rs`, char-based: Courier is monospace).
2. **Group:** keep-together groups: a dialogue block (cue + parentheticals + dialogue lines),
   a scene heading with what follows, a transition with what precedes.
3. **Fill:** greedy page filler over groups with the break rules from spec §5.1, inserting
   `(MORE)` / `NAME (CONT'D)` and sentence-boundary splits.
4. **Output:** `Page { number, lines: Vec<PageLine> }`, each line with a column offset (in
   1/10 inch), alignment, styled runs and optional scene numbers. The same output feeds the PDF
   writer, the editor's page breaks/page numbers, the navigator's page column and the CLI text
   dump used by golden tests.

`Layout` holds the page geometry and element margins (spec §5), so A4 or a 2-line scene spacing
is just another `Layout`.

Golden tests: `crates/core/tests/golden/<name>.fountain` → `<name>.pages.txt` (one block per
page, lines indented by column). `UPDATE_GOLDEN=1 cargo test` regenerates them; reviewers diff.

## 5. PDF (`storyteller-pdf`)

Draws each `PageLine` with `craft_pdf::write` (Krilla): x = column × 7.2 pt, y = top + line ×
12 pt, one text run per style (Courier Prime Regular/Bold/Italic/BoldItalic, embedded and
subset), underline as a stroked line. Title page first, a document outline entry per scene,
metadata (title, author). The whole export runs inside `craft_pdf::write::guard`.

Fonts live in `assets/fonts/` with `OFL.txt`, `<file>.attribution` sidecars and an
`ATTRIBUTION.md` entry, embedded via `include_bytes!`.

## 6. Engine (`storyteller-engine`)

- **`Session`:** `script`, `path`, `format`, `dirty`, `undo: History`, `cursor: Caret { element,
  offset }` (byte offset, always clamped to a char boundary).
- **Commands:** `struct Command { id, label, menu, shortcut, params: &[Param], enabled, run }`
  in a `Registry`. `run(&mut Session, &Params) -> Result<Outcome, CommandError>`. Examples:
  `file.new`, `file.open`, `file.save`, `file.save_as`, `file.export_pdf`, `file.export_fdx`,
  `edit.undo`, `edit.redo`, `element.insert`, `element.set_text`, `element.set_kind`,
  `element.split`, `element.merge_previous`, `element.delete`, `flow.enter`, `flow.tab`,
  `flow.shift_tab`, `scene.number`, `scene.clear_numbers`, `view.scene_numbers`,
  `title_page.set`, `autocomplete.suggest`.
- **Flow rules** (`flow.rs`) are a data table (spec §4.1), so the UI just calls `flow.enter`.
- **Saving** uses `craft_atomic::atomic_write`; the previous file survives any failure.
- **Control protocol** (`control.rs`): JSON lines, `{"id":1,"method":"command.run",
  "params":{"command":"flow.enter"}}` → `{"id":1,"result":…}` or `{"id":1,"error":…}`. Methods:
  `command.list`, `command.run`, `doc.inspect`, `doc.paginate`, plus `ui.*` methods that the
  host (app or headless server) implements through a `UiHost` trait: `ui.elements`, `ui.click`,
  `ui.type`, `ui.key`, `ui.focus`, `ui.screenshot`. Bound to 127.0.0.1 only; a line limit of
  1 MiB.

## 7. Desktop app (`method-storyteller`)

- eframe 0.36. The script view is a scrolling column of page-width frames; each element is a
  frameless `TextEdit::multiline` positioned at its margin with the element's width. Page breaks
  and page numbers come from the paginator, re-run when the text changes (debounced).
- Key handling: before the focused `TextEdit` sees input, Enter/Tab/Shift-Tab/Backspace-at-start/
  ⌘1–7/Up/Down-at-edge are consumed and turned into engine commands. Typed text is committed with
  `element.set_text` (merged undo).
- **Automation:** every widget is created through `ui_id("editor.element.12")`-style helpers
  that record `(id, rect, role, label)` each frame. The control server thread forwards requests
  over a channel; the UI thread answers them on the next frame and requests a repaint.
  `ui.screenshot` uses `ViewportCommand::Screenshot`.
- **Never crash:** a panic hook writes a crash note and an autosave beside the document;
  frame code runs under `catch_unwind` as a last resort.

## 8. Testing

| What | Where |
| --- | --- |
| Fountain parse/write, round-trip, spec examples | `crates/core/tests/fountain.rs` |
| FDX import/export, round-trip, hostile XML | `crates/core/tests/fdx.rs` |
| Pagination rules (goldens) | `crates/core/tests/pagination.rs` + `tests/golden/` |
| Hostile input never panics (random bytes, deep nesting, huge lines) | `crates/core/tests/hostile.rs` |
| Commands, flow, undo, save/load | `crates/engine/tests/` |
| PDF opens and contains the text | `crates/pdf/tests/` |
| App smoke via control channel (manual/CI-optional) | `docs/control-protocol.md` |

## 9. Later phases (designed for, not built)

- **Revisions:** `Element` gains `revision: Option<RevisionId>`; pagination gains locked-page
  mode (A-pages) by freezing page boundaries and inserting `nA` pages.
- **Reports / beat board:** derived views over `Script` in core.
- **FilmCraft link:** export scene list with page and eighth counts as FilmCraft markers.
