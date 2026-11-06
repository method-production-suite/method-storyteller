# The design pass: what is done and what is left

Roadmap item D: match Method Storyteller to the family (Method Budget's and Method Schedule's
BRAND.md), with every step pictured offscreen in light and dark. [BRAND.md](BRAND.md) holds
the results; this page is the plan for the rest. Nothing here needs the app on anyone's screen:
every picture is drawn by the offscreen gallery (`gallery` in `crates/app/src/screens.rs`).

## Done

| Step | Where it is |
| --- | --- |
| The final mark and wordmark (decision 10) | `docs/brand/`, drawn by `scripts/make-brand.py` |
| The app icon at every size, and document icons for `.osf`, `.fountain`, `.fdx` and `.mstory` | `packaging/icons/`, drawn by `scripts/make-icons.py` |
| Colour, type and spacing tokens | `crates/app/src/theme.rs`, listed in BRAND.md |
| Contrast for every pair, held by a test | `theme::tests::every_pair_meets_wcag_aa` |
| Dark mode, surface by surface | BRAND.md, "Dark mode, surface by surface" |
| Empty and error states in the family voice | `crates/app/src/messages.rs`, with tests |
| Scene colours as tokens, 3:1 on the panel in both themes, and the act row in `ink-soft` | `theme::SCENE_COLOURS`, `scene_colours_hold_three_to_one_on_the_panel` |
| A visible border on text boxes in dialogs | `theme::field`, using `rule-strong` |
| A gallery of 31 surfaces in both themes, every surface built so far | `gallery`, writing `gallery/` in the build folder |
| The contact sheet: every shot, light beside dark, labelled | `gallery/contact-sheet.png`, drawn with the gallery |

## Left to do

The owner's look at the contact sheet, and whatever that turns up. To draw it:

```
cargo test -p method-storyteller --release gallery -- --ignored
```

It is `gallery/contact-sheet.png` in the build folder (`STORYTELLER_GALLERY_OUT` to put it
elsewhere). A surface added later gets a named shot in `gallery`, in light and dark. Things to
look at in particular: the deeper Yellow and Orange stripes in light mode, which read as olive and
tan so they hold 3:1 on the panel.
