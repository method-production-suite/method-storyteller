# Locked pages from `.fdx`: where a page starts

Measured 2026-10-28 with `cargo xtask corpus` on the private corpus. Only totals are given here;
no script text.

## What the files hold

A `.fdx` with locked pages lists one `LockedPage` per page: `PageNumber` (the printed number),
`Position` (a character offset into the whole script), `LevelIndex` and `LockLevel`, and in later
files `Deleted`. In the corpus, 5 files have 470 locked pages, numbered 1, 2, 3 and so on. None
of their scenes record a lettered page (`SceneProperties@Page`), including heavily revised drafts.

We read `Position` the way we read script note ranges and deletion marks: printed characters,
one for each paragraph break. That keeps every lock exact from `.fdx` to OSF to `.fdx` (470/470).
Scene headings also sit under the lock Final Draft recorded for them in 505 of 508 cases.

## What does not hold yet

A page can only start at the beginning of a paragraph or of a word. Read our way:

- 89 of the 470 locks start a paragraph;
- 381 fall inside a paragraph, and only 69 of those (18%) land at a word start. That is about
  what positions picked at random would give;
- in each file, the early locks are exact: every one up to page 31 in two of the files, and up
  to page 20 or so in another. Then each lock sits a few characters off, sometimes before the true
  start and sometimes after it. The error does not build up as you go through the script.

Changes we tried, none of which fits:

- zero or two characters for each paragraph break;
- literal asterisks, double spaces, leading and trailing spaces, and tabs, all of which the
  importer already keeps exactly;
- characters outside the Basic Multilingual Plane, of which there are none;
- one more line per page (the page fit was not what moved them).

Honouring these locks as read adds 191 pages to the 470 (A-pages and combined pages that Final
Draft's own records give no sign of). It puts 455 of 508 scene headings on the page Final Draft
printed, against 457 with the locks ignored.

## So, for now

`paginate` honours locks made where we can place them exactly: our own locking (M6, next) and
OSF's paragraph-level `page_number`. Locks read from `.fdx` are kept and written back unchanged,
but the script paginates as unlocked. The corpus run reports both, so a fix shows at once.

Settling it needs a small locked script saved by the reference app. Lock it, then make one
known edit before page 3: add a word, delete a paragraph, add a revised line. Save the file
after each edit, and compare the `Position` values. This is the ninth question for the
reference-app session (ROADMAP, waiting on the owner).
