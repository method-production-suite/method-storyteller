# Releasing Method Storyteller

A release is a version tag, `v<version>`, on `main`. Pushing it starts
[`.github/workflows/release.yml`](../.github/workflows/release.yml), which checks the tag
against the version, builds and packages every platform, signs and notarizes the Mac build
when the secrets are there, and drafts a GitHub Release. **Nothing is published by the
workflow.** A person checks the draft and publishes it.

Every release also follows the suite's shared checklist,
[RELEASE-CHECKLIST.md](https://github.com/method-production-suite/.github/blob/main/RELEASE-CHECKLIST.md)
in the org `.github` repository. This page adds Storyteller's own commands.

## The version

The version lives in one place: `version` under `[workspace.package]` in the root
`Cargo.toml`. Every crate inherits it; the macOS bundle, the Linux package and the Windows
installer read it at packaging time.

```bash
cargo xtask version            # print it
```

Versions are `MAJOR.MINOR.PATCH`, optionally `-PRERELEASE` (`0.2.0-beta.1`). Before 1.0: a
minor bump when a release lets people do something new, a patch bump otherwise. A
prerelease is drafted as a GitHub prerelease.

## Release notes

Every version has `docs/releases/<version>.md`, written for people who write scripts: the
voice, the shape and the skeleton are in [release-notes-template.md](release-notes-template.md)
(what's new, fixed, not there yet, who helped). The workflow uses it as the draft's
description, and refuses to build a tag whose notes are missing or still hold a `{{...}}`
placeholder.

## The installers

Every installer has a version-free name, so `releases/latest/download/<file>` links on the
website and in the README never go stale:

| File | Platform | How it is made |
|---|---|---|
| `Method-Storyteller-macOS.dmg` | macOS 11 or later, one universal app for Apple silicon and Intel | `cargo xtask bundle-macos --target universal-apple-darwin`, then `cargo xtask dmg-macos` |
| `Method-Storyteller-windows-x64-setup.exe` | Windows 10 or later, x64 | `cargo xtask package-windows` (Inno Setup 6) |
| `Method-Storyteller-linux-amd64.deb` | Debian and Ubuntu, x86_64 | `cargo xtask package-linux` (`dpkg-deb`) |
| `Method-Storyteller-linux-x86_64.AppImage` | Other Linux, x86_64 | `cargo xtask package-linux --appimagetool <path> --runtime <path>` |
| `SHA256SUMS` | | the assemble job: one line per file of the release (installers, the updater archive and its signature, `latest.json`), checked before the draft is made |
| `SHA256SUMS.minisig` | | the sign job: `SHA256SUMS` signed with the update key (`cargo xtask sign-file`), checked against the app's public key first; check it with `minisign -Vm SHA256SUMS -P <the public key>` |

The packaging code is `xtask/src/package.rs`; the files it reads are in `packaging/`:
`macos/Info.plist` (bundle id `com.methodstoryteller.desktop`, display name "Method
Storyteller", executable `method-storyteller-app`; `{{VERSION}}` is filled in),
`windows/method-storyteller.iss`, `linux/method-storyteller.desktop`, and `icons/`.

Everything lands in `bundle/<platform>/` in the build folder: `target/`, or on the suite's
Macs the shared cache `~/.cache/method-target/method-storyteller/main` (one folder per checkout), which every local cargo
command names with `CARGO_TARGET_DIR` (xtask asks cargo where it is, never assumes `./target`).

### Try the Mac app locally

On a Mac, unsigned (ad-hoc signed, as every local build is). `universal-apple-darwin` builds
both slices (`rustup target add aarch64-apple-darwin x86_64-apple-darwin` once) and joins them
with `lipo`, as the release does; `aarch64-apple-darwin` alone is quicker for a look:

```bash
cargo xtask bundle-macos --target universal-apple-darwin
open "target/bundle/macos/Method Storyteller.app"
cargo xtask dmg-macos           # optional: the disk image, from the bundle above
```

`--sign "<Developer ID Application identity>"` signs with the hardened runtime and a secure
timestamp instead, ready to notarize; `--no-build` packages the binary already in
`target/`.

### The icon

The app icon and the document icons are drawn from the mark by `scripts/make-icons.py`
(docs/BRAND.md says how). After the mark changes, run `python3 scripts/make-brand.py`, then
`python3 scripts/make-icons.py` on a Mac; it remakes everything in `packaging/icons/`.
Commit them together.

## Cutting a release

From an up-to-date `main` with a clean tree (`0.2.0` stands for the new version):

1. Work through "Before the tag" in the shared checklist: version, green checks, pictures,
   notes, helpers, README. Retake the pictures with `cargo xtask screens` (offscreen; the
   terminal picture needs Chrome) and compare them with the committed ones before committing.
2. Bump `version` in the root `Cargo.toml`, run `cargo check --workspace` so `Cargo.lock`
   follows, and write `docs/releases/0.2.0.md` from the skeleton.
3. `cargo xtask ci`, and the Intel slice under Rosetta on an Apple silicon Mac:
   `cargo test --locked --workspace --target x86_64-apple-darwin`, then `arch -x86_64
   target/x86_64-apple-darwin/debug/method-storyteller --version` and one headless launch
   of the app. Open a pull request with those changes (MReview reviews it). Merge it when
   it is approved and green.
4. Rehearse: Actions > release > Run workflow on `main`, the tag left empty. It builds,
   signs, notarizes and checks the merge commit exactly as a release would, and drafts
   nothing. Wait for green; it is the only full build before the tag.
5. Tag the rehearsed commit and push the tag, **with the owner's approval** (one yes covers
   the tag and publishing). Run each as its own command:

   ```bash
   git switch main && git pull --ff-only
   git tag -a v0.2.0 -m "Method Storyteller 0.2.0"
   git push origin v0.2.0
   ```

6. Watch the `release` workflow. When it is green, the draft release is on the Releases
   page. Check it (below), report to the coordinator, then publish it unless the owner has
   said stop. A failed check stops it.

**A pushed tag is permanent.** People download from it and the workflow has built from it.
Never move, delete or force-push a public tag: fix a problem with the next patch version.

To rebuild an existing tag's draft (a runner failed, say), run the workflow by hand
(Actions > release > Run workflow) with the tag. It refreshes the files and the notes of a
draft and refuses a release that is already published.

## Before publishing the draft

- Download `Method-Storyteller-macOS.dmg` on a Mac, drag the app to
  Applications and open it. Signed and notarized, it opens without a warning; check with
  `spctl --assess --type execute --verbose=4 "/Applications/Method Storyteller.app"`
  (`accepted`, source Notarized Developer ID). `lipo -archs` on the app's binary says
  exactly `x86_64 arm64`, and `arch -x86_64` runs it with `--version` under Rosetta.
- Open an invented sample script, type a scene, save it, reopen it.
- Where you can, install the Windows setup and the `.deb` or AppImage and do the same; the
  README's platform table says which were tested.
- Check the four installer names are version-free and `SHA256SUMS` lists every file of the
  release (exactly one checksum file, under exactly that name), and that
  `SHA256SUMS.minisig` verifies it against the app's public key.
- Check the updater files are there: `Method-Storyteller_<version>_universal.app.tar.gz`, its
  `.sig`, and `latest.json` naming this version with `darwin-aarch64` and `darwin-x86_64`
  entries, both for that one archive. The workflow already refused a signature that does not
  check against the app's public key. Once the draft is published,
  `releases/latest/download/latest.json` serves it and installed apps on Macs find the update
  within six hours (or at once with Help > Check for Updates).
- Then publish. Afterwards, the shared checklist's "After publishing" steps (the website,
  live download links, the announcement), and the README's platform table where a status
  changed.

## Secrets

The suite uses the same secret names in every repository (Method Budget's set), so one
1Password item feeds them all. They live in the team 1Password vault and in this
repository's Actions secrets, nowhere else. The maintainer sets them from a terminal signed
in to GitHub; each command asks for the value or reads it from standard input:

```bash
gh secret set DEVELOPER_ID_CERT_P12_BASE64 -R method-production-suite/method-storyteller
gh secret set CERT_PASSWORD -R method-production-suite/method-storyteller
gh secret set KEYCHAIN_PASSWORD -R method-production-suite/method-storyteller
gh secret set AC_API_KEY_ID -R method-production-suite/method-storyteller
gh secret set AC_API_ISSUER_ID -R method-production-suite/method-storyteller
gh secret set AC_API_KEY_P8_BASE64 -R method-production-suite/method-storyteller
gh secret set TAURI_SIGNING_PRIVATE_KEY -R method-production-suite/method-storyteller
gh secret set TAURI_SIGNING_PRIVATE_KEY_PASSWORD -R method-production-suite/method-storyteller
```

| Secret | Value | Turns on |
|---|---|---|
| `DEVELOPER_ID_CERT_P12_BASE64` | the Developer ID Application certificate with its private key, exported as a `.p12`, base64-encoded | signing the Mac app and disk image (with the next two) |
| `CERT_PASSWORD` | the password the `.p12` was exported with | |
| `KEYCHAIN_PASSWORD` | any long random string (it only locks the throwaway keychain on the build machine) | |
| `AC_API_KEY_ID` | the App Store Connect API key's ID | notarizing and stapling (with the next two, and signing) |
| `AC_API_ISSUER_ID` | that key's Issuer ID | |
| `AC_API_KEY_P8_BASE64` | the key's `.p8` file, base64-encoded | |
| `TAURI_SIGNING_PRIVATE_KEY` | the minisign update key, as the base64 of its secret key box ("Method Storyteller - Update signing key" in 1Password; key ID C4D0E2230F7A95FB) | the updater archive, its `.sig` and `latest.json` (with the next one, and a notarized Mac build) |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | that key's password | |
| `WINDOWS_CERTIFICATE` | a code signing `.pfx`, base64-encoded (none exists yet) | signing the Windows installer (with the next one) |
| `WINDOWS_CERTIFICATE_PASSWORD` | that `.pfx`'s password | |

The Mac build is signed with whichever Developer ID Application identity the `.p12` holds;
its name is masked in the logs and never written in the repository. Without the Apple
secrets the Mac build is ad-hoc signed, and the draft's description says the first launch
needs System Settings > Privacy & Security > Open Anyway. With the Windows secrets both the
app and the installer are signed; without them both are unsigned. The job log says
what was skipped.

Updates: the update key's names are the suite's (Method Budget's), though Storyteller does not
use Tauri; `cargo xtask updater-archive` signs with the `minisign` crate the suite updater is
tested against. The public half is `PUBKEY` in `crates/app/src/updates.rs`; a new key means a
new `PUBKEY` in a release first, or installed apps refuse the next update. Without the key,
or without notarizing, the release has no updater files and installed apps do not see it.

After the signing tools change (the `minisign` crate, the key format), and before the next tag,
run Actions > updater-key-check > Run workflow on `main`. It signs a throwaway bundle with the
real key and checks the signature against the app's `PUBKEY`, as a release does, without the key
leaving GitHub. A red run means the next release's update would be refused: fix it first.

## The release workflow

| Job | Permissions | What it does |
|---|---|---|
| `check` | contents: read | the tag is `v` plus the version in `Cargo.toml`, points at a commit on `main`, and `docs/releases/<version>.md` is written; with no tag, on `main`, a rehearsal of main's latest commit |
| `build` (macOS universal, Windows x64, Linux x64) | contents: read | compiles everything first, before any secret reaches the runner (on macOS both slices, joined with `lipo`); then packages each platform with `--no-build`; on macOS signs, notarizes, staples and verifies (every executable must be exactly `x86_64 arm64`), each only when its secrets are there; then, with the update key, packs and signs the updater archive; on Windows signs the app and the installer only with a certificate |
| `assemble` | contents: read | checks all four installers are there, writes `latest.json` after checking the archive's signature against the app's public key (one universal archive under `darwin-aarch64` and `darwin-x86_64`), writes and checks `SHA256SUMS` (one line per file of the release); runs in a rehearsal too |
| `sign` | contents: read | builds the release tool, then signs `SHA256SUMS` with the update key as `SHA256SUMS.minisig`; runs in a rehearsal too |
| `draft` | contents: write (the only job with it) | runs `gh release create --draft` with the notes and the files; skipped in a rehearsal |

Only first-party actions are used, each pinned to the suite's full commit SHA. Each secret
is given only to the steps that use it. The two AppImage tools (appimagetool and its
runtime) are downloaded from the AppImage project's GitHub releases and checked against
SHA-256 sums pinned in the workflow; to move to newer ones, update the URLs and the sums
together.
