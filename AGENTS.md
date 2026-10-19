# Method Storyteller — rules for contributors and agents

These rules override everything else in the repository.

## 0. Never crash

Writers trust this app with their work; a crash can lose it.

- No `unwrap`, `expect`, `panic!`, `unreachable!`, `todo!`, `unimplemented!` or indexing that
  can panic outside tests (the workspace lints deny them). Return `Result` and propagate with
  `?`; use `get()`, `let … else`, checked or saturating maths.
- No `unsafe` (forbidden workspace-wide). The one exception is `crates/macos-open`: a few
  Objective-C messages with no safe binding, each block with a `// SAFETY:` comment, which clippy enforces there
  (`undocumented_unsafe_blocks = "deny"`); the owner approved this exception on 2026-10-18. Nothing else goes
  there, and no other crate opts out of the workspace lints.
- Every file, control-channel request and CLI argument is hostile: cap sizes
  (`MAX_INPUT_BYTES`, `MAX_ELEMENTS`), bound loops and recursion, never divide by a value that
  can be zero.
- Saves go through `craft-atomic`; a failed save must leave the previous file intact.
- Background threads run under `catch_unwind`; the UI frame does too, as a last resort, not a
  licence.
- Every crash fix comes with a regression test that panicked before.

## 1. Clean-room and licences

- Never read, disassemble or copy anything from Final Draft (binaries, templates, sample files
  shipped with it). FDX support comes from the public structure of the format and files we
  write ourselves.
- Fountain follows the spec at fountain.io.
- Never copy GPL/LGPL/AGPL code.
- Every bundled asset is openly licensed, has a `<file>.attribution` sidecar and an
  `ATTRIBUTION.md` entry (`cargo xtask assets`). Keep licence notices for anything adapted.

## 2. Architecture

- Layers (`cargo xtask layers`): `storyteller-core` (pure model, formats, pagination; no UI, no
  filesystem) → `storyteller-pdf` → `storyteller-engine` → `storyteller-cli` /
  `method-storyteller`. Only the app may depend on egui, eframe, winit, rfd or craft-quit.
- **Everything is a command** in `storyteller-engine`: id, label, menu path, shortcut, params,
  `enabled`, `run`. Menus, keys, the CLI and the control channel dispatch by id.
- **Everything is agent-drivable**: every interactive widget registers an automation id
  (`Automation::add`); new UI state is reachable through `ui.state` or `doc.inspect`.
- Pagination output is golden-tested; review golden diffs like code.

## 3. Gates

Before every commit: `cargo xtask ci` (fmt, clippy `-D warnings`, tests, layers, assets, privacy).
Pull requests get no build on GitHub (SUITE-STANDARDS.md §6): open each as a draft, run
`cargo xtask ci` on its latest commit, paste the command and its pass line under "Tested
locally" with the commit's SHA, then `gh pr ready`. CI on a pull request runs only the crate-age
check and cargo deny; `full.yml` builds on macOS, Windows and Linux weekly and by hand before a
release. `cargo xtask ci` builds with `--locked`, so a stale Cargo.lock fails it.
CI also runs `cargo deny check` (deny.toml) and the 14-day crate-age check
(`scripts/check-crate-ages.py`): a new dependency must be at least 14 days old.
Real scripts never enter the repository: `cargo xtask corpus` measures against them from a
manifest outside it (`STORYTELLER_CORPUS`) and prints numbers only.
Commits are small and green. For UI work, look at the result: run the app with
`--control <port>` and take `ui.screenshot`.
