# Attribution

Every bundled asset, with its author, source and licence. Each file also has a
`<file>.attribution` sidecar next to it; `cargo xtask assets` checks both.

## Fonts

| File | Author | Source | Licence |
| --- | --- | --- | --- |
| `assets/fonts/CourierPrime-Regular.ttf` | Alan Dague-Greene for Quote-Unquote Apps; © 2015 The Courier Prime Project Authors | [google/fonts @ 7204f5a9](https://github.com/google/fonts/tree/7204f5a9b1e530633e00c67bd4954ceb7d8c2483/ofl/courierprime), upstream [quoteunquoteapps/CourierPrime](https://github.com/quoteunquoteapps/CourierPrime) | SIL OFL 1.1 ([OFL.txt](assets/fonts/OFL.txt)) |
| `assets/fonts/CourierPrime-Bold.ttf` | as above | as above | SIL OFL 1.1 |
| `assets/fonts/CourierPrime-Italic.ttf` | as above | as above | SIL OFL 1.1 |
| `assets/fonts/CourierPrime-BoldItalic.ttf` | as above | as above | SIL OFL 1.1 |

The fonts are distributed unmodified. PDF export embeds subsets of them, which the OFL permits.

## Libraries

Rust dependencies are fetched by Cargo and keep their own licences (see `Cargo.lock`).
[craft-libs](https://github.com/storytold/craft-libs) (MIT OR Apache-2.0) is used as a git
dependency pinned to one revision; its own third-party notices are in its `NOTICE` and
`ATTRIBUTION.md`. No code is copied from it into this repository.
[method-libs](https://github.com/method-production-suite/method-libs) (MIT OR Apache-2.0)
provides the updater, `method-update`, as a git dependency pinned to one revision.
`scripts/check-ignore-expiry.py` is copied from it unchanged.
