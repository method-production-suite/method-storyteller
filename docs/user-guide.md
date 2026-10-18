# Method Storyteller user guide

Method Storyteller is a screenwriting app. You type, and it formats the script the way scripts
print. This guide covers the basics; Help > Keyboard Shortcuts lists every key.

(⌘ is the Command key on a Mac. On Windows and Linux, use Ctrl instead.)

## Start a script

Open the app and start typing. The first line is a scene heading: type `int. kitchen - night`
and it shows as `INT. KITCHEN - NIGHT`. Press Enter to write action under it.

To open a script you already have, choose File > Open (⌘O). Method Storyteller opens Fountain
(`.fountain`, `.spmd`, `.txt`), Open Screenplay Format (`.osf`), `.fadein` and `.fdx` files.

## Write without formatting by hand

Each paragraph of a script is an element: scene heading, action, character, parenthetical,
dialogue, transition or shot. The element at the caret shows in the bottom left of the window,
and you can pick another one there.

- **Enter** ends the element and starts the one that usually comes next: after a scene heading,
  action; after a character name, dialogue; after dialogue, the next character.
- **Enter on an empty element** changes its type, so an empty action becomes a scene heading.
- **Tab** and **Shift-Tab** change the type of the element you are in: action to character,
  dialogue to parenthetical, and back.
- **⌘1 to ⌘7** set the element directly: scene heading, action, character, parenthetical,
  dialogue, transition, shot.
- **Shift-Enter** starts a new line inside the same action or dialogue paragraph.
- **Backspace** at the start of an element joins it to the one before.

As you type a character name, a location or a time of day, names you have used before are
offered. The arrow keys choose one, Enter or Tab takes it, Escape closes the list, or keep
typing.

## Find your way around

- The **scene navigator** on the left lists every scene with its page. Click one to go there.
  View > Scene Navigator (⌘⇧N) shows or hides it.
- **Pages** shows the script as separate pages, as they print. **Draft** shows one continuous
  column. View > Page View (⌘⇧P) switches between them.
- **Zoom** with ⌘= and ⌘−, and ⌘0 returns to actual size.
- The status bar shows the page you are on and how many pages the script has.

## Title page and scene numbers

File > Title Page (⌘⇧T) sets the title, credit, author, contact and the other title page fields.
Production > Number Scenes numbers every scene; Production > Remove Scene Numbers takes them off.
View > Show Scene Numbers shows or hides them.

## Save and export

- **File > Save** (⌘S) saves the script. A new script is saved as Fountain unless you choose
  File > Save New Scripts as Open Screenplay Format.
- **Fountain** is plain text that any text editor opens. **Open Screenplay Format** is structured
  XML that keeps more production detail. Both are open formats you own.
- **File > Export PDF** (⌘E) writes a PDF ready to print or send.
- File > Export also writes `.fdx`, Open Screenplay Format and Fountain copies.

While a script has unsaved changes, the app copies it every minute to the `Recovery` folder in
its data folder, so a crash or a power cut costs at most a minute of work.

## Get help

- Help > Report a Bug opens a bug report in your browser with the app version and your system
  filled in. Nothing is sent until you submit it yourself.
- Help > What's New lists what changed in this version.
- Help > Licenses and Notices shows the licences of Method Storyteller and what it includes.

Method Storyteller is free and open source, under MIT OR Apache-2.0. It needs no account and
works offline.
