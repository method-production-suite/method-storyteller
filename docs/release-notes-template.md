# Writing release notes

Every version has notes in `docs/releases/<version>.md`, written once, for the people who
write scripts:

- **On GitHub.** The release workflow uses the file as the draft release's description.
- **In the app.** Help > What's New shows them once the Help menu reads them (the suite
  standard; the app reads the notes as plain text, so no markup is drawn and links show
  their words only).
- **On the website.** The app's page links the newest release.

Start a new file by copying the skeleton at the end of this page. Release steps:
[RELEASING.md](RELEASING.md).

## Voice

- Write for someone writing a script, not for someone reading the code. Say what they can
  do now, or what works better, in their words: "Dual dialogue stays together across a
  page break", not "pagination keeps dual blocks atomic".
- Plain sentences. No em dashes (a colon, a comma or a period instead). No exclamation
  marks. No hype. No internal names: no crate, module, function or file names, no commit
  hashes, no milestone numbers.
- No other vendors' product names. Name a file format by its extension (`.fdx`), as the
  README does.
- Examples use invented scripts only (Field Station). Never a real production, a real
  script or a real person's work: the notes become a public web page.
- Claim only what is built and in this release. Most commits earn no bullet: refactors,
  tests, tooling and documentation don't, unless a writer would notice.

## Shape

1. `# Method Storyteller <version>`, the title. It ends with the version.
2. One headline paragraph: one or two sentences on what this release means.
3. Sections, each `## ` and a heading named for what changed for the reader ("Pages break
   where a production expects", not "Pagination"). Then, in this order when they have
   anything in them: `## Fixed`, `## Not there yet` (known gaps), `## Who helped`.
4. In each section, bullets. A bullet may start with a bold claim, `**Like this.**`,
   followed by a sentence or two. A long bullet wraps onto indented lines.
5. At least one picture or short clip for any visible change, from the capture command
   (invented material only), linked from the repository.

Inline `code` and `**bold**` are allowed. HTML comments are never shown anywhere, so they
are fine for notes to the next writer.

**Who helped** names everyone who contributed to this release, by the name they asked to be
credited with: code, testing, reports, and film knowledge (a script supervisor who explained
how scene numbers lock is a contributor). Ask before naming anyone who isn't already public
on GitHub.

## Length

A minor release: a headline, two to four sections, up to about a dozen bullets. A patch
release: a headline and a `## Fixed` section is often all it needs. The whole file stays
well under 60,000 characters (a GitHub release has a limit).

## The skeleton

Copy the block below into `docs/releases/<version>.md`. Every `{{...}}` is a placeholder:
none may be left when the release is tagged.

<!-- skeleton:start -->
```markdown
# Method Storyteller {{version}}

{{One or two sentences: what this release means for someone writing a script.}}

## {{A heading named for what changed for the reader}}

- **{{The claim, in the reader's words.}}** {{What it means, and where to find it.}}

## Fixed

- **{{What works now.}}** {{What used to go wrong, in a few words.}}

## Not there yet

- {{A known gap, a caveat, or something to do after updating. Delete this section if there is none.}}

## Who helped

- {{Everyone who contributed to this release, film knowledge included.}}
```
<!-- skeleton:end -->
