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
| `Method-Storyteller-macOS.dmg` | macOS 11 or later, Apple silicon only | `cargo xtask bundle-macos`, then `cargo xtask dmg-macos` |
| `Method-Storyteller-windows-x64-setup.exe` | Windows 10 or later, x64 | `cargo xtask package-windows` (Inno Setup 6) |
| `Method-Storyteller-linux-amd64.deb` | Debian and Ubuntu, x86_64 | `cargo xtask package-linux` (`dpkg-deb`) |
| `Method-Storyteller-linux-x86_64.AppImage` | Other Linux, x86_64 | `cargo xtask package-linux --appimagetool <path> --runtime <path>` |
| `SHA256SUMS.txt` | | the draft job |

The packaging code is `xtask/src/package.rs`; the files it reads are in `packaging/`:
`macos/Info.plist` (bundle id `com.methodstoryteller.desktop`, display name "Method
Storyteller", executable `method-storyteller-app`; `{{VERSION}}` is filled in),
`windows/method-storyteller.iss`, `linux/method-storyteller.desktop`, and `icons/`.

Everything lands in `target/bundle/<platform>/`.

### Try the Mac app locally

On an Apple silicon Mac, unsigned (ad-hoc signed, as every local build is):

```bash
cargo xtask bundle-macos --target aarch64-apple-darwin
open "target/bundle/macos/Method Storyteller.app"
cargo xtask dmg-macos           # optional: the disk image, from the bundle above
```

`--sign "<Developer ID Application identity>"` signs with the hardened runtime and a secure
timestamp instead, ready to notarize; `--no-build` packages the binary already in
`target/`.

### The icon

The icon is provisional, like the brand: `packaging/icons/mark.svg` is a copy of the mark in
the website's `brand/storyteller` folder. When the brand changes, copy the new mark there
and run `packaging/icons/make-icons.sh` on a Mac; it remakes `icon.icns`, `icon.ico` and the
PNGs. Commit them together.

## Cutting a release

From an up-to-date `main` with a clean tree (`0.2.0` stands for the new version):

1. Work through "Before the tag" in the shared checklist: version, green checks, pictures,
   notes, helpers, README. Retake the pictures with `cargo xtask screens` (offscreen; the
   terminal picture needs Chrome) and compare them with the committed ones before committing.
2. Bump `version` in the root `Cargo.toml`, run `cargo check --workspace` so `Cargo.lock`
   follows, and write `docs/releases/0.2.0.md` from the skeleton.
3. `cargo xtask ci`, then open a pull request with those changes (MReview reviews it).
   Merge it when it is approved and green.
4. Tag the merge commit on `main` and push the tag, **with the owner's approval** (the
   suite's rule for anything outward):

   ```bash
   git switch main && git pull --ff-only
   git tag -a v0.2.0 -m "Method Storyteller 0.2.0"
   git push origin v0.2.0
   ```

5. Watch the `release` workflow. When it is green, the draft release is on the Releases
   page.

**A pushed tag is permanent.** People download from it and the workflow has built from it.
Never move, delete or force-push a public tag: fix a problem with the next patch version.

To rebuild an existing tag's draft (a runner failed, say), run the workflow by hand
(Actions > release > Run workflow) with the tag. It refreshes the files and the notes of a
draft and refuses a release that is already published.

## Before publishing the draft

- Download `Method-Storyteller-macOS.dmg` on an Apple silicon Mac, drag the app to
  Applications and open it. Signed and notarized, it opens without a warning; check with
  `spctl --assess --type execute --verbose=4 "/Applications/Method Storyteller.app"`
  (`accepted`, source Notarized Developer ID).
- Open an invented sample script, type a scene, save it, reopen it.
- Where you can, install the Windows setup and the `.deb` or AppImage and do the same; the
  README's platform table says which were tested.
- Check the four installer names are version-free and `SHA256SUMS.txt` lists them.
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
```

| Secret | Value | Turns on |
|---|---|---|
| `DEVELOPER_ID_CERT_P12_BASE64` | the Developer ID Application certificate with its private key, exported as a `.p12`, base64-encoded | signing the Mac app and disk image (with the next two) |
| `CERT_PASSWORD` | the password the `.p12` was exported with | |
| `KEYCHAIN_PASSWORD` | any long random string (it only locks the throwaway keychain on the build machine) | |
| `AC_API_KEY_ID` | the App Store Connect API key's ID | notarizing and stapling (with the next two, and signing) |
| `AC_API_ISSUER_ID` | that key's Issuer ID | |
| `AC_API_KEY_P8_BASE64` | the key's `.p8` file, base64-encoded | |
| `WINDOWS_CERTIFICATE` | a code signing `.pfx`, base64-encoded (none exists yet) | signing the Windows installer (with the next one) |
| `WINDOWS_CERTIFICATE_PASSWORD` | that `.pfx`'s password | |

The Mac build is signed with whichever Developer ID Application identity the `.p12` holds;
its name is masked in the logs and never written in the repository. Without the Apple
secrets the Mac build is ad-hoc signed, and the draft's description says the first launch
needs System Settings > Privacy & Security > Open Anyway. With the Windows secrets both the
app and the installer are signed; without them both are unsigned. The job log says
what was skipped.

Updates: Storyteller has no updater yet, so there is no updater key and no `latest.json`.
The shared `method-update` crate (SUITE-STANDARDS.md section 1) brings both; its secrets
will be added here when it lands.

## The release workflow

| Job | Permissions | What it does |
|---|---|---|
| `check` | contents: read | the tag is `v` plus the version in `Cargo.toml`, points at a commit on `main`, and `docs/releases/<version>.md` is written |
| `build` (macOS arm64, Windows x64, Linux x64) | contents: read | compiles everything first, before any secret reaches the runner; then packages each platform with `--no-build`; on macOS signs, notarizes, staples and verifies (the binary must be arm64 only), each only when its secrets are there; on Windows signs the app and the installer only with a certificate |
| `draft` | contents: write (the only job with it) | checks all four installers are there, writes `SHA256SUMS.txt`, and runs `gh release create --draft` with the notes |

Only first-party actions are used, each pinned to the suite's full commit SHA. Each secret
is given only to the steps that use it. The two AppImage tools (appimagetool and its
runtime) are downloaded from the AppImage project's GitHub releases and checked against
SHA-256 sums pinned in the workflow; to move to newer ones, update the URLs and the sums
together.
