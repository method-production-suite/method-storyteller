//! The screenplay document model: a title page and a flat list of elements.

use serde::{Deserialize, Serialize};

/// The type of a script paragraph.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ElementKind {
    SceneHeading,
    Action,
    Character,
    Parenthetical,
    Dialogue,
    Transition,
    Shot,
    Centered,
    Lyrics,
    Section,
    Synopsis,
    PageBreak,
    /// A television act's opening line (TEASER, ACT ONE, TAG): its own page.
    NewAct,
    /// The line that closes an act (END OF ACT ONE).
    EndOfAct,
}

impl ElementKind {
    /// Every kind, in menu order.
    pub const ALL: [ElementKind; 14] = [
        ElementKind::SceneHeading,
        ElementKind::Action,
        ElementKind::Character,
        ElementKind::Parenthetical,
        ElementKind::Dialogue,
        ElementKind::Transition,
        ElementKind::Shot,
        ElementKind::Centered,
        ElementKind::Lyrics,
        ElementKind::NewAct,
        ElementKind::EndOfAct,
        ElementKind::Section,
        ElementKind::Synopsis,
        ElementKind::PageBreak,
    ];

    /// Stable snake_case id used by commands and the control channel.
    pub fn id(self) -> &'static str {
        match self {
            ElementKind::SceneHeading => "scene_heading",
            ElementKind::Action => "action",
            ElementKind::Character => "character",
            ElementKind::Parenthetical => "parenthetical",
            ElementKind::Dialogue => "dialogue",
            ElementKind::Transition => "transition",
            ElementKind::Shot => "shot",
            ElementKind::Centered => "centered",
            ElementKind::Lyrics => "lyrics",
            ElementKind::Section => "section",
            ElementKind::Synopsis => "synopsis",
            ElementKind::PageBreak => "page_break",
            ElementKind::NewAct => "new_act",
            ElementKind::EndOfAct => "end_of_act",
        }
    }

    /// Parse an [`ElementKind::id`].
    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.id() == id)
    }

    /// Human-readable name.
    pub fn label(self) -> &'static str {
        match self {
            ElementKind::SceneHeading => "Scene Heading",
            ElementKind::Action => "Action",
            ElementKind::Character => "Character",
            ElementKind::Parenthetical => "Parenthetical",
            ElementKind::Dialogue => "Dialogue",
            ElementKind::Transition => "Transition",
            ElementKind::Shot => "Shot",
            ElementKind::Centered => "Centered",
            ElementKind::Lyrics => "Lyrics",
            ElementKind::Section => "Section",
            ElementKind::Synopsis => "Synopsis",
            ElementKind::PageBreak => "Page Break",
            ElementKind::NewAct => "New Act",
            ElementKind::EndOfAct => "End of Act",
        }
    }

    /// Kinds whose text is always shown and saved in capitals.
    pub fn is_caps(self) -> bool {
        matches!(
            self,
            ElementKind::SceneHeading
                | ElementKind::Character
                | ElementKind::Transition
                | ElementKind::Shot
                | ElementKind::NewAct
                | ElementKind::EndOfAct
        )
    }

    /// Kinds that hold exactly one line of text.
    pub fn is_single_line(self) -> bool {
        matches!(
            self,
            ElementKind::SceneHeading
                | ElementKind::Character
                | ElementKind::Transition
                | ElementKind::Shot
                | ElementKind::Parenthetical
                | ElementKind::Section
                | ElementKind::Synopsis
                | ElementKind::PageBreak
                | ElementKind::NewAct
                | ElementKind::EndOfAct
        )
    }

    /// Kinds that continue a dialogue block after a character cue.
    pub fn is_dialogue_part(self) -> bool {
        matches!(self, ElementKind::Parenthetical | ElementKind::Dialogue)
    }

    /// Kinds that appear on the printed page.
    pub fn is_printed(self) -> bool {
        !matches!(self, ElementKind::Section | ElementKind::Synopsis | ElementKind::PageBreak)
    }
}

/// The act line a centred line stands for, if any: `TEASER`, `COLD OPEN`, `ACT ONE` (or `ACT 1`),
/// `TAG`, `PROLOGUE` and `EPILOGUE` open an act; `END OF …` closes one. Fountain has no act
/// element, so this is how a Fountain file's centred act lines read back as acts.
pub fn act_kind(text: &str) -> Option<ElementKind> {
    let line = text.trim().trim_matches(|c: char| c == '_' || c == '*').trim().to_uppercase();
    if line.starts_with("END OF ") && line.len() > "END OF ".len() {
        return Some(ElementKind::EndOfAct);
    }
    let opens = matches!(line.as_str(), "TEASER" | "COLD OPEN" | "TAG" | "PROLOGUE" | "EPILOGUE")
        || line
            .strip_prefix("ACT ")
            .is_some_and(|rest| !rest.is_empty() && rest.split_whitespace().count() <= 2 && rest.chars().all(|c| c.is_alphanumeric() || c == ' '));
    opens.then_some(ElementKind::NewAct)
}

/// The shape of a new television script.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TvShape {
    /// Teaser, Acts One to Five, Tag.
    #[default]
    OneHour,
    /// Cold Open, Acts One to Three, Tag.
    HalfHour,
    /// A multi-camera half hour: Cold Open, Acts One and Two, Tag, its scenes lettered, each
    /// on a new page with its characters, dialogue double-spaced and action in capitals.
    MultiCam,
}

impl TvShape {
    pub fn id(self) -> &'static str {
        match self {
            TvShape::OneHour => "one_hour",
            TvShape::HalfHour => "half_hour",
            TvShape::MultiCam => "multi_cam",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        [TvShape::OneHour, TvShape::HalfHour, TvShape::MultiCam].into_iter().find(|shape| shape.id() == id)
    }
}

/// Whether an element of this kind starts what follows a scene: the next scene, or an act line.
fn ends_scene(kind: ElementKind) -> bool {
    matches!(kind, ElementKind::SceneHeading | ElementKind::NewAct | ElementKind::EndOfAct)
}

/// How Production > Number Scenes numbers (see [`Script::number_with`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Numbering {
    /// The first number given (1 by default).
    pub start: u32,
    /// Keep the numbers headings have, and letter the new ones.
    pub keep_existing: bool,
    /// Number only the headings from this element on (0: all).
    pub from: usize,
    /// Leave out the letters I and O, which read as 1 and 0.
    pub skip_io: bool,
}

impl Default for Numbering {
    fn default() -> Self {
        Numbering { start: 1, keep_existing: false, from: 0, skip_io: false }
    }
}

/// What [`Script::number_with`] did.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Numbered {
    /// Headings whose number changed.
    pub changed: usize,
    /// New scenes left unnumbered, because no number sits between their neighbours.
    pub unplaced: Vec<Unplaced>,
}

/// A new scene with no number between its neighbours: `before` and `after` are theirs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Unplaced {
    pub heading: usize,
    pub before: String,
    pub after: String,
}

/// How many numbers are tried for one inserted scene: the next letter, then a level deeper.
const INSERT_TRIES: usize = 30;

/// Whether `number` comes before `after` in a script's order: by the number's digits, then its
/// letters as written (`2` < `2A` < `2AA` < `2B`). Numbers with letters in front (`A1`) and
/// other forms compare by their digits alone, then as written.
fn sorts_before(number: &str, after: &str) -> bool {
    let key = |n: &str| {
        let digits: String = n.chars().filter(char::is_ascii_digit).collect();
        let value = digits.parse::<u64>().unwrap_or(0);
        let suffix: String = n.chars().skip_while(|c| !c.is_ascii_digit()).skip_while(char::is_ascii_digit).collect();
        let prefix: String = n.chars().take_while(|c| !c.is_ascii_digit()).collect();
        // A1 sorts before 1: a prefix is an insert ahead of its number.
        (value, prefix.is_empty(), prefix, suffix)
    };
    key(number) < key(after)
}

/// The letter after `c` (skipping I and O with `skip_io`), or `None` after Z.
fn next_letter(c: char, skip_io: bool) -> Option<char> {
    let mut next = char::from_u32(u32::from(c) + 1)?;
    while skip_io && matches!(next, 'I' | 'O') {
        next = char::from_u32(u32::from(next) + 1)?;
    }
    (next <= 'Z').then_some(next)
}

/// The `n`th inserted-scene letter group: A … Z, AA, AB … (I and O left out with `skip_io`).
pub fn letters(n: usize, skip_io: bool) -> String {
    let alphabet: Vec<char> = ('A'..='Z').filter(|c| !(skip_io && matches!(c, 'I' | 'O'))).collect();
    let base = alphabet.len();
    let mut n = n;
    let mut out = Vec::new();
    loop {
        out.push(alphabet.get(n % base).copied().unwrap_or('A'));
        if n < base {
            break;
        }
        n = n / base - 1;
    }
    out.iter().rev().collect()
}

/// The `n`th number tried for a scene inserted after `before`, in the order they sort: first
/// the next letter (`12` gives `12A`, `12A` gives `12B`; `12Z` has none), then a level deeper
/// (`12AA`, `12AB` … between `12A` and `12B`). I and O are left out with `skip_io`, even after a
/// suffix that has them.
fn insert_after(before: &str, n: usize, skip_io: bool) -> String {
    let digits_end = before.char_indices().rev().find(|(_, c)| c.is_ascii_digit()).map_or(0, |(i, c)| i + c.len_utf8());
    let suffix = before.get(digits_end..).unwrap_or_default();
    let lettered = digits_end > 0 && !suffix.is_empty() && suffix.chars().all(|c| c.is_ascii_uppercase());
    // The level-deeper letters: A, B … Z without wrapping.
    let deeper = |k: usize| -> Option<char> {
        let mut c = 'A';
        for _ in 0..k {
            c = next_letter(c, skip_io)?;
        }
        Some(c)
    };
    if !lettered {
        return deeper(n).map_or_else(String::new, |c| format!("{before}{c}"));
    }
    if n == 0 {
        let mut chars: Vec<char> = suffix.chars().collect();
        return match chars.last().copied().and_then(|c| next_letter(c, skip_io)) {
            Some(next) => {
                chars.pop();
                chars.push(next);
                format!("{}{}", before.get(..digits_end).unwrap_or_default(), chars.iter().collect::<String>())
            }
            None => String::new(),
        };
    }
    deeper(n - 1).map_or_else(String::new, |c| format!("{before}{c}"))
}

/// The first of `candidate(0)`, `candidate(1)` … that `taken` does not hold.
fn free(taken: &std::collections::HashSet<String>, candidate: impl Fn(usize) -> String) -> String {
    (0..100_000).map(&candidate).find(|c| !taken.contains(c)).unwrap_or_else(|| candidate(0))
}

/// A scene's letter: A to Z, then AA, AB …
pub fn scene_letter(n: usize) -> String {
    let mut n = n;
    let mut letters = Vec::new();
    loop {
        letters.push(char::from(b'A' + u8::try_from(n % 26).unwrap_or(0)));
        if n < 26 {
            break;
        }
        n = n / 26 - 1;
    }
    letters.iter().rev().collect()
}

/// A multi-camera script's element settings: the defaults, with each scene on a new page,
/// action in capitals and dialogue double-spaced.
pub fn multi_cam_format() -> crate::format::Format {
    // The defaults as our own Open Screenplay Format styles carry them, every setting filled in.
    let mut format = crate::osf::parse(&crate::osf::write(&Script::default())).map(|s| s.format).unwrap_or_default();
    for element in &mut format.elements {
        match element.name.as_str() {
            "Scene Heading" => element.new_page = true,
            "Action" => element.all_caps = true,
            "Dialogue" => element.spacing = Some(200),
            _ => {}
        }
    }
    format
}

/// One script paragraph.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Element {
    pub kind: ElementKind,
    /// Text with Fountain inline markup (`*italic*`, `**bold**`, `_underline_`, `[[note]]`).
    pub text: String,
    /// Scene number of a scene heading, such as `12` or `12A`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scene_number: Option<String>,
    /// This character cue is the second speaker of a dual-dialogue pair.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub dual: bool,
    /// Outline depth of a section (1 = `#`).
    #[serde(default, skip_serializing_if = "is_zero")]
    pub level: u8,
    /// Script notes on this paragraph or part of it (see [`ScriptNote`]). Plain `[[notes]]` in
    /// the text are separate: they hold text only.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<ScriptNote>,
    /// Revised text: spans of the printed text marked with a revision set (see [`Revised`]).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub revised: Vec<Revised>,
    /// Places where revised text was taken out: the margin marks them, the text is gone.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub deletions: Vec<Deletion>,
    /// Locked pages that start in this paragraph (see [`PageStart`]). A script whose pages are
    /// locked has one for every page.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub page_starts: Vec<PageStart>,
    /// Final Draft data this paragraph carried that the model does not use (see [`FdxParagraph`]).
    #[serde(skip)]
    pub fdx: Option<Box<FdxParagraph>>,
    /// Open Screenplay Format data this paragraph carried that the model does not use.
    #[serde(skip)]
    pub osf: Option<Box<OsfParagraph>>,
    /// Fountain boneyard blocks (`/* … */`, kept verbatim) that come just before this paragraph:
    /// text the writer set aside. Never printed or paginated; written back to Fountain.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub boneyard: Vec<String>,
    /// A scene heading's own title, summary and colour (`.fdx` `SceneProperties`).
    #[serde(default, skip_serializing_if = "SceneMeta::is_empty")]
    pub scene: SceneMeta,
    /// An omitted scene's heading (printed `OMITTED`) keeps the scene it replaced, as Fountain,
    /// heading and all, for Unomit; empty when the file kept nothing to bring back.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub omitted: Option<String>,
}

/// What an omitted scene's heading says.
pub const OMITTED: &str = "OMITTED";

/// What a writer says about a scene beyond its heading: a short title, a summary and a colour,
/// shown in the navigator. Not printed.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SceneMeta {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub title: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub summary: String,
    /// `#RRGGBB`, or `.fdx`'s own `#RRRRGGGGBBBB`, as written.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
}

impl SceneMeta {
    pub fn is_empty(&self) -> bool {
        self.title.is_empty() && self.summary.is_empty() && self.color.is_none()
    }

    /// The colour as red, green and blue, from `#RRGGBB` or `#RRRRGGGGBBBB`.
    pub fn rgb(&self) -> Option<(u8, u8, u8)> {
        let hex = self.color.as_deref()?.trim().trim_start_matches('#');
        let channel = |i: usize, width: usize| hex.get(i * width..i * width + 2).and_then(|h| u8::from_str_radix(h, 16).ok());
        let width = match hex.len() {
            6 => 2,
            12 => 4,
            _ => return None,
        };
        Some((channel(0, width)?, channel(1, width)?, channel(2, width)?))
    }
}

/// A script note: a comment on a paragraph, or on part of it, that never prints.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScriptNote {
    /// The note itself, paragraphs separated by `\n`.
    pub text: String,
    /// What it covers, in characters of the paragraph's printed text: `start..end`, a point when
    /// the two are equal. An `end` past the paragraph runs on into the paragraphs after it, each
    /// paragraph break counting one character. `None` is the whole paragraph.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub range: Option<(u32, u32)>,
    /// Colour as `#RRGGBB`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    /// Tied to no text (a Final Draft unanchored note); kept with the script's first paragraph.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub unanchored: bool,
    /// Final Draft data the note carried that the model does not use.
    #[serde(skip)]
    pub fdx: Option<Box<FdxNote>>,
}

/// Where a locked page starts, and what it is called.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PageStart {
    /// Characters of the paragraph's printed text before the page starts (0: with the paragraph).
    pub at: u32,
    /// The page number as printed, such as `12` or `12A`.
    pub label: String,
    /// Final Draft `LockedPage` attributes the model does not use, every one in file order.
    #[serde(skip)]
    pub fdx_attributes: Vec<(String, String)>,
}

/// Where text was deleted in a revision: a printed-character position and its set.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Deletion {
    pub at: u32,
    pub set: u32,
}

/// Text marked as changed in a revision.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Revised {
    /// Characters of the paragraph's printed text, `from..to`.
    pub from: u32,
    pub to: u32,
    /// The [`RevisionSet::id`] it was changed in.
    pub set: u32,
}

/// One revision set, such as "Blue Rev.": the colour and mark its changes print with.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RevisionSet {
    /// The number revised text refers to (Final Draft's `ID`, OSF's `index`).
    pub id: u32,
    pub name: String,
    /// The margin mark, usually `*`.
    pub mark: String,
    /// Colour of the mark and revised text, `#RRGGBB`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    /// Colour of the pages issued in this set, `#RRGGBB`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page_color: Option<String>,
    /// Final Draft attributes the model does not use (`FullRevision`, `Style` …), every one in file order.
    #[serde(skip)]
    pub fdx_attributes: Vec<(String, String)>,
}

/// The script's revision sets and which one new changes go into.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Revisions {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active: Option<u32>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sets: Vec<RevisionSet>,
    /// Attributes of Final Draft's `Revisions` element (`RevisionMode`, `Location` …), in file order.
    #[serde(skip)]
    pub fdx_attributes: Vec<(String, String)>,
}

impl Revisions {
    pub fn is_empty(&self) -> bool {
        self.active.is_none() && self.sets.is_empty() && self.fdx_attributes.is_empty()
    }

    /// The set with this id.
    pub fn set(&self, id: u32) -> Option<&RevisionSet> {
        self.sets.iter().find(|s| s.id == id)
    }

    /// Of the sets `ids` that `show` shows, the latest (the last in [`Revisions::sets`]): the
    /// one whose mark a line, or whose name and colour a page, carries.
    pub fn latest<'a>(&self, ids: impl IntoIterator<Item = &'a u32>, show: ShowRevisions) -> Option<&RevisionSet> {
        let ids: Vec<u32> = ids.into_iter().copied().collect();
        self.sets.iter().rev().find(|set| ids.contains(&set.id) && show.shows(set.id, self.active))
    }
}

/// Which revisions show, on screen and on the page (View > Show Revisions).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShowRevisions {
    /// The active set only.
    Active,
    #[default]
    All,
    None,
}

impl ShowRevisions {
    pub fn shows(self, set: u32, active: Option<u32>) -> bool {
        match self {
            ShowRevisions::Active => active == Some(set),
            ShowRevisions::All => true,
            ShowRevisions::None => false,
        }
    }
}

/// The standard order of revision colours, with each page colour: the first round, then
/// `Double White` and on (docs/production-spec.md §3.3).
pub const REVISION_COLORS: [(&str, Option<&str>); 10] = [
    ("White", None),
    ("Blue", Some("#C9DAF8")),
    ("Pink", Some("#F4CCCC")),
    ("Yellow", Some("#FFF2CC")),
    ("Green", Some("#D9EAD3")),
    ("Goldenrod", Some("#F6D58C")),
    ("Buff", Some("#F3E3C3")),
    ("Salmon", Some("#F8C3B0")),
    ("Cherry", Some("#E6A1A8")),
    ("Tan", Some("#D9C3A5")),
];

/// The `n`th colour in the standard order, with its round: `Blue`, then `Double Blue` …
pub fn revision_color(n: usize) -> (String, Option<&'static str>) {
    let (name, page) = REVISION_COLORS.get(n % REVISION_COLORS.len()).copied().unwrap_or(("White", None));
    let prefix = match n / REVISION_COLORS.len() {
        0 => "",
        1 => "Double ",
        2 => "Triple ",
        _ => "Quadruple ",
    };
    (format!("{prefix}{name}"), page)
}

impl Script {
    /// Adds the next set in the standard order (`White` first), named for its colour and
    /// `date` (`Blue Revision 10/21/26`), with the mark `*`, and makes it the active one.
    /// Returns its id.
    pub fn add_revision_set(&mut self, date: &str) -> u32 {
        let id = self.revisions.sets.iter().map(|s| s.id).max().map_or(1, |m| m + 1);
        let (color, page) = revision_color(self.revisions.sets.len());
        let name = if date.is_empty() { format!("{color} Revision") } else { format!("{color} Revision {date}") };
        self.revisions.sets.push(RevisionSet {
            id,
            name,
            mark: "*".into(),
            color: None,
            page_color: page.map(str::to_string),
            fdx_attributes: Vec::new(),
        });
        self.revisions.active = Some(id);
        id
    }

    /// Changes a set's name, mark (up to two characters) or page colour (`#RRGGBB`).
    pub fn update_revision_set(&mut self, id: u32, name: Option<&str>, mark: Option<&str>, page_color: Option<Option<&str>>) -> Result<(), String> {
        let set = self.revisions.sets.iter_mut().find(|s| s.id == id).ok_or("there is no such revision set")?;
        if let Some(name) = name {
            let name = name.trim();
            if name.is_empty() {
                return Err("a revision set needs a name".into());
            }
            set.name = name.to_string();
        }
        if let Some(mark) = mark {
            let mark = mark.trim();
            if mark.is_empty() || mark.chars().count() > 2 {
                return Err("a revision mark is one or two characters, such as *".into());
            }
            set.mark = mark.to_string();
        }
        if let Some(page) = page_color {
            set.page_color = page.map(|c| c.to_uppercase());
        }
        Ok(())
    }

    /// Deletes a set. Text it marks moves to the set before it in the list (or loses its mark
    /// when it was the first); the active set moves to the last one left. Returns how many
    /// marked passages moved.
    pub fn delete_revision_set(&mut self, id: u32) -> Result<usize, String> {
        let at = self.revisions.sets.iter().position(|s| s.id == id).ok_or("there is no such revision set")?;
        let before = at.checked_sub(1).and_then(|b| self.revisions.sets.get(b)).map(|s| s.id);
        self.revisions.sets.remove(at);
        let mut moved = 0;
        for element in &mut self.elements {
            match before {
                Some(before) => {
                    for revised in element.revised.iter_mut().filter(|r| r.set == id) {
                        revised.set = before;
                        moved += 1;
                    }
                }
                None => {
                    let count = element.revised.len();
                    element.revised.retain(|r| r.set != id);
                    moved += count - element.revised.len();
                }
            }
        }
        if self.revisions.active == Some(id) {
            self.revisions.active = self.revisions.sets.last().map(|s| s.id);
        }
        Ok(moved)
    }

    /// How many passages a set marks.
    pub fn revision_marks(&self, id: u32) -> usize {
        self.elements.iter().map(|e| e.revised.iter().filter(|r| r.set == id).count()).sum()
    }
}

/// How a Final Draft file stores a script note.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FdxNoteForm {
    /// A `ScriptNote` inside its paragraph, placed among the text runs.
    Inline,
    /// An entry of the top-level `ScriptNotes` list, placed by `Range` (Version 5 and later).
    Ranged,
    /// An entry of the top-level `UnanchoredScriptNotes` list (Versions 3 and 4).
    Unanchored,
}

/// What an imported Final Draft script note carried beyond the model.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FdxNote {
    pub form: FdxNoteForm,
    /// Every attribute in file order. `Range` is kept as a place holder: it is worked out again
    /// from the note's range when writing.
    pub attributes: Vec<(String, String)>,
    /// The note's XML as read, written back while its text, colour and author are unchanged.
    /// Empty when the note came through another format.
    pub raw: String,
    /// The note as read, to tell whether it changed.
    pub as_read: Option<Box<ScriptNote>>,
}

/// What an imported Open Screenplay Format paragraph carried beyond the model, kept so the file
/// round-trips: its style name, its other attributes and style overrides, and other children.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OsfParagraph {
    /// The paragraph's style name, such as `Normal Text` or a custom `Act Heading`.
    pub style: String,
    /// The kind it was imported as. The style name is written back only while the kind is unchanged.
    pub kind: ElementKind,
    /// `<para>` attributes the model does not hold (`page_number`, `edited_by`, `synopsis_color` …).
    pub attributes: Vec<(String, String)>,
    /// Overrides on the paragraph's own `<style>` the model does not hold (indents, fonts …).
    pub style_attributes: Vec<(String, String)>,
    /// Raw XML of children other than `<style>` and `<text>`: revision marks, vendor data.
    pub children: String,
}

/// What an imported Open Screenplay Format document carried outside its paragraphs.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct OsfDocument {
    /// The `version` attribute (`30`, `40` …); spellings follow it when writing back.
    pub version: String,
    /// Top-level sections in file order.
    pub sections: Vec<OsfSection>,
    /// Other members of the `.fadein` archive it came in, kept as they were.
    pub archive_entries: Vec<ArchiveEntry>,
    /// The revisions as read; the revision list and setting are written back as they were while
    /// they are the same.
    pub revisions: Option<Revisions>,
    /// Whether `settings@pages_locked` was on.
    pub pages_locked: bool,
    /// The page layout and styles as read; written back as they were while they are the same.
    pub format: Option<crate::format::Format>,
    /// The words `<spelling><ignored>` held as read; the section is written back as it was while
    /// the script's list is the same.
    pub spelling_ignored: Vec<String>,
}

/// A member of a ZIP archive, kept compressed exactly as read so it can be written back.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArchiveEntry {
    pub name: String,
    /// ZIP compression method (0 stored, 8 deflated).
    pub method: u16,
    pub crc32: u32,
    /// Size once decompressed.
    pub size: u32,
    /// The member's data as stored in the archive.
    pub data: Vec<u8>,
}

/// One top-level section of an imported Open Screenplay Format document.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OsfSection {
    /// Where `<paragraphs>` was.
    Paragraphs,
    /// Where the title page was, with its raw XML and what it parsed to; the raw XML is written
    /// back while the title page is unchanged.
    TitlePage { raw: String, parsed: TitlePage },
    /// A section the model does not use (`settings`, `styles`, `lists` …), verbatim.
    Raw(String),
}

/// What an imported Final Draft paragraph carried beyond the model, kept so the file
/// round-trips: its own type name, its other attributes and its non-text children.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FdxParagraph {
    /// `Type` as written in the file, such as `Outline 2` or `New Act`.
    pub type_name: String,
    /// The kind it was imported as. The type name is written back only while the kind is unchanged.
    pub kind: ElementKind,
    /// Attributes other than the ones the model holds (`Type`, `Number`, `StartsNewPage`), in file order.
    pub attributes: Vec<(String, String)>,
    /// Raw XML of every child element other than `Text`: scene properties, script notes, omitted scenes.
    pub children: String,
}

/// What an imported Final Draft document carried outside its paragraphs.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FdxDocument {
    /// Attributes of the `FinalDraft` root, in file order.
    pub root_attributes: Vec<(String, String)>,
    /// Top-level sections after `Content`, in file order.
    pub sections: Vec<FdxSection>,
    /// The `ScriptNotes` list as read: each note with its `Range` in the document. The list is
    /// written back as it was while the script's ranged notes are the same.
    pub ranged_notes: Vec<(u32, u32, ScriptNote)>,
    /// The `UnanchoredScriptNotes` list as read.
    pub unanchored_notes: Vec<ScriptNote>,
    /// The revisions as read; the `Revisions` section is written back as it was while they are the same.
    pub revisions: Option<Revisions>,
    /// The `LockedPages` list as read, each page with its `Position` in the script. Written back
    /// as it was while the script's page starts are the same.
    pub locked_pages: Vec<(u32, PageStart)>,
    /// The `DeletedText` list as read, each mark with its `Offset` in the script. Written back
    /// as it was while the script's deletion marks are the same.
    pub deletions: Vec<(u32, Deletion)>,
    /// The page layout and element settings as read; their sections are written back as they
    /// were while they are the same.
    pub format: Option<crate::format::Format>,
    /// The words `SpellCheckIgnoreLists/IgnoredWords` held as read; the section is written back
    /// as it was while the script's list is the same.
    pub spelling_ignored: Vec<String>,
}

/// One top-level section of an imported Final Draft document.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FdxSection {
    /// Where the title page was, with its raw XML and what it parsed to. The raw XML is written
    /// back while the title page is unchanged.
    TitlePage { raw: String, parsed: TitlePage },
    /// A section the model does not use (`ElementSettings`, `Revisions`, `TagData` ...), verbatim.
    Raw(String),
}

fn is_zero(value: &u8) -> bool {
    *value == 0
}

impl Element {
    pub fn new(kind: ElementKind, text: impl Into<String>) -> Self {
        let mut element = Element {
            kind,
            text: text.into(),
            scene_number: None,
            dual: false,
            level: 0,
            notes: Vec::new(),
            revised: Vec::new(),
            page_starts: Vec::new(),
            fdx: None,
            osf: None,
            boneyard: Vec::new(),
            scene: SceneMeta::default(),
            omitted: None,
            deletions: Vec::new(),
        };
        if kind == ElementKind::Section {
            element.level = 1;
        }
        element
    }

    pub fn with_scene_number(mut self, number: impl Into<String>) -> Self {
        self.scene_number = Some(number.into());
        self
    }

    pub fn with_dual(mut self, dual: bool) -> Self {
        self.dual = dual;
        self
    }

    /// Enforce the per-kind invariants: single-line kinds hold one line, capitalised kinds are
    /// upper case, parentheticals are bracketed, and fields that do not apply are cleared.
    pub fn normalize(&mut self) {
        if self.kind.is_single_line() && self.text.contains(['\n', '\r']) {
            self.text = self.text.split(['\n', '\r']).filter(|part| !part.is_empty()).collect::<Vec<_>>().join(" ");
        }
        if self.kind.is_caps() {
            self.text = self.text.to_uppercase();
        }
        if self.kind == ElementKind::Parenthetical {
            let trimmed = self.text.trim();
            if !trimmed.is_empty() && !trimmed.starts_with('(') {
                self.text = format!("({trimmed})");
            } else if trimmed.starts_with('(') && !trimmed.ends_with(')') {
                self.text = format!("{trimmed})");
            }
        }
        if self.kind == ElementKind::PageBreak {
            self.text.clear();
        }
        if self.kind != ElementKind::SceneHeading {
            self.scene_number = None;
        }
        if self.kind != ElementKind::Character {
            self.dual = false;
        }
        if self.kind == ElementKind::Section {
            self.level = self.level.clamp(1, 6);
        } else {
            self.level = 0;
        }
    }

    /// Character name without extensions such as `(V.O.)` or a trailing `^`.
    /// Whether this is an omitted scene's heading: one we omitted, or one a file says is
    /// omitted (its heading reads `OMITTED`, or Final Draft's omitted-scene marker is kept).
    pub fn is_omitted(&self) -> bool {
        self.kind == ElementKind::SceneHeading
            && (self.omitted.is_some()
                || crate::inline::plain(&self.text).trim().eq_ignore_ascii_case(OMITTED)
                || self.fdx.as_deref().is_some_and(|f| f.children.contains("<OmittedScene")))
    }

    pub fn character_name(&self) -> &str {
        character_name(&self.text)
    }

    /// Replaces the text, moving what is anchored to it (revised spans, note ranges) with the
    /// edit: the text both versions share at the start and the end keeps its anchors, and an
    /// anchor inside the changed part moves to its edge (a revised span grows over new text).
    pub fn set_text(&mut self, text: impl Into<String>) {
        let old = crate::inline::plain(&self.text);
        self.text = text.into();
        let new = crate::inline::plain(&self.text);
        if old == new {
            return;
        }
        let edit = Edit::between(&old, &new);
        for span in &mut self.revised {
            span.from = edit.start(span.from);
            span.to = edit.end(span.to);
        }
        self.revised.retain(|r| r.to > r.from);
        for deletion in &mut self.deletions {
            deletion.at = edit.start(deletion.at);
        }
        for page in &mut self.page_starts {
            page.at = edit.start(page.at);
        }
        for note in &mut self.notes {
            if let Some((from, to)) = note.range {
                note.range =
                    Some(if from == to { (edit.start(from), edit.start(from)) } else { (edit.start(from), edit.end(to).max(edit.start(from))) });
            }
        }
    }

    /// Replaces the text as [`Element::set_text`] does and, with a revision set, marks the
    /// change in it: new or changed characters become revised in `set`, and text only taken
    /// out leaves a deletion mark where it was (revision mode, docs/production-spec.md §3.4).
    pub fn set_text_revised(&mut self, text: impl Into<String>, set: Option<u32>) {
        let old = crate::inline::plain(&self.text);
        self.set_text(text);
        let Some(set) = set.filter(|s| *s != 0) else { return };
        let new = crate::inline::plain(&self.text);
        if old == new {
            return;
        }
        let edit = Edit::between(&old, &new);
        if edit.new_tail > edit.prefix {
            self.mark_range(edit.prefix, edit.new_tail, Some(set));
        } else if edit.old_tail > edit.prefix && !self.deletions.iter().any(|d| d.at == edit.prefix && d.set == set) {
            self.deletions.push(Deletion { at: edit.prefix, set });
            self.deletions.sort_by_key(|d| (d.at, d.set));
        }
    }

    /// Marks printed characters `from..to` as revised in `set` (any other set's mark there
    /// gives way), or clears every mark there, deletion marks included, with `None`.
    pub fn mark_range(&mut self, from: u32, to: u32, set: Option<u32>) {
        if to <= from {
            return;
        }
        let mut spans = Vec::new();
        for span in self.revised.drain(..) {
            if span.to <= from || span.from >= to {
                spans.push(span);
                continue;
            }
            if span.from < from {
                spans.push(Revised { to: from, ..span });
            }
            if span.to > to {
                spans.push(Revised { from: to, ..span });
            }
        }
        match set {
            Some(set) => spans.push(Revised { from, to, set }),
            None => self.deletions.retain(|d| d.at < from || d.at > to),
        }
        spans.sort_by_key(|r| (r.from, r.to));
        let mut merged: Vec<Revised> = Vec::new();
        for span in spans {
            match merged.last_mut() {
                Some(last) if last.set == span.set && last.to >= span.from => last.to = last.to.max(span.to),
                _ => merged.push(span),
            }
        }
        self.revised = merged;
    }

    /// Moves everything anchored at or after printed character `at` to `into` (the paragraph an
    /// Enter split off), counted from its start.
    pub fn split_anchors(&mut self, at: u32, into: &mut Element) {
        let mut kept = Vec::new();
        for span in self.revised.drain(..) {
            if span.from < at {
                kept.push(Revised { to: span.to.min(at), ..span });
            }
            if span.to > at {
                into.revised.push(Revised { from: span.from.saturating_sub(at), to: span.to - at, ..span });
            }
        }
        self.revised = kept;
        let (stay, go): (Vec<Deletion>, Vec<Deletion>) = self.deletions.drain(..).partition(|d| d.at <= at);
        self.deletions = stay;
        into.deletions.extend(go.into_iter().map(|d| Deletion { at: d.at - at, ..d }));
        let (stay, go): (Vec<ScriptNote>, Vec<ScriptNote>) = self.notes.drain(..).partition(|n| n.range.is_none_or(|(from, _)| from < at));
        self.notes = stay;

        into.notes.extend(go.into_iter().map(|mut n| {
            n.range = n.range.map(|(from, to)| (from - at, to - at));
            n
        }));
        let (stay, moved): (Vec<PageStart>, Vec<PageStart>) = self.page_starts.drain(..).partition(|p| p.at < at || p.at == 0);
        self.page_starts = stay;
        into.page_starts.extend(moved.into_iter().map(|p| PageStart { at: p.at - at, ..p }));
    }

    /// Takes `other`'s anchors (the paragraph a Backspace joined on), placed `at` printed
    /// characters into this one.
    pub fn absorb_anchors(&mut self, other: &mut Element, at: u32) {
        for span in other.revised.drain(..) {
            mark_revised(&mut self.revised, at.saturating_add(span.from), span.to.saturating_sub(span.from), span.set);
        }
        self.deletions.extend(other.deletions.drain(..).map(|d| Deletion { at: at.saturating_add(d.at), ..d }));
        self.page_starts.extend(other.page_starts.drain(..).map(|p| PageStart { at: at.saturating_add(p.at), ..p }));
        self.notes.extend(other.notes.drain(..).map(|mut n| {
            n.range = Some(n.range.map_or((at, at), |(from, to)| (at.saturating_add(from), at.saturating_add(to))));
            n
        }));
    }

    /// A cursor over the revised spans, for walking the text from start to end.
    pub(crate) fn revision_cursor(&self) -> RevisionCursor<'_> {
        let mut spans: Vec<&Revised> = self.revised.iter().collect();
        spans.sort_by_key(|r| (r.from, r.to));
        RevisionCursor { spans, next: 0 }
    }

    /// The revision set the printed character at `at` was changed in.
    pub fn revision_at(&self, at: u32) -> Option<u32> {
        self.revised.iter().find(|r| r.from <= at && at < r.to).map(|r| r.set)
    }
}

/// Revised spans walked in order: asked about places that only move forward, it answers in
/// constant time overall.
pub(crate) struct RevisionCursor<'a> {
    spans: Vec<&'a Revised>,
    next: usize,
}

impl RevisionCursor<'_> {
    fn skip_to(&mut self, at: u32) {
        while self.spans.get(self.next).is_some_and(|r| r.to <= at) {
            self.next += 1;
        }
    }

    /// The set the character at `at` was changed in.
    pub(crate) fn set_at(&mut self, at: u32) -> Option<u32> {
        self.skip_to(at);
        self.spans.get(self.next).filter(|r| r.from <= at).map(|r| r.set)
    }

    /// The first place after `at` where revised text starts or ends.
    pub(crate) fn next_edge(&mut self, at: u32) -> Option<u32> {
        self.skip_to(at);
        self.spans.get(self.next).map(|r| if r.from > at { r.from } else { r.to })
    }
}

/// One edit between two versions of a paragraph's printed text, as the parts they share at the
/// start and at the end.
struct Edit {
    prefix: u32,
    old_tail: u32,
    new_tail: u32,
}

impl Edit {
    fn between(old: &str, new: &str) -> Edit {
        let (old, new): (Vec<char>, Vec<char>) = (old.chars().collect(), new.chars().collect());
        let prefix = old.iter().zip(&new).take_while(|(a, b)| a == b).count();
        let room = old.len().min(new.len()) - prefix;
        let suffix = old.iter().rev().zip(new.iter().rev()).take(room).take_while(|(a, b)| a == b).count();
        let count = |n: usize| u32::try_from(n).unwrap_or(u32::MAX);
        Edit { prefix: count(prefix), old_tail: count(old.len() - suffix), new_tail: count(new.len() - suffix) }
    }

    /// Where an anchor's start goes.
    fn start(&self, at: u32) -> u32 {
        if at <= self.prefix {
            at
        } else if at >= self.old_tail {
            (at - self.old_tail).saturating_add(self.new_tail)
        } else {
            self.prefix
        }
    }

    /// Where an anchor's end goes.
    fn end(&self, at: u32) -> u32 {
        if at <= self.prefix {
            at
        } else if at >= self.old_tail {
            (at - self.old_tail).saturating_add(self.new_tail)
        } else {
            self.new_tail
        }
    }
}

/// Marks `len` characters from `from` as revised in `set`, joining a span it continues.
pub(crate) fn mark_revised(spans: &mut Vec<Revised>, from: u32, len: u32, set: u32) {
    if set == 0 || len == 0 {
        return;
    }
    let to = from.saturating_add(len);
    match spans.last_mut() {
        Some(last) if last.set == set && last.to == from => last.to = to,
        _ => spans.push(Revised { from, to, set }),
    }
}

/// Character name without extensions such as `(V.O.)` or a trailing `^`.
pub fn character_name(cue: &str) -> &str {
    let cue = cue.trim().trim_end_matches('^').trim_end();
    match cue.find('(') {
        Some(at) => cue.get(..at).unwrap_or(cue).trim_end(),
        None => cue,
    }
}

/// One title-page field: a Fountain key and its (possibly multi-line) value.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TitleField {
    pub key: String,
    pub value: String,
}

/// Title-page fields in file order.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TitlePage {
    pub fields: Vec<TitleField>,
}

impl TitlePage {
    /// Standard keys offered by the title-page editor.
    pub const STANDARD_KEYS: [&'static str; 8] = ["Title", "Credit", "Author", "Source", "Draft date", "Contact", "Copyright", "Notes"];

    pub fn is_empty(&self) -> bool {
        self.fields.iter().all(|field| field.value.trim().is_empty())
    }

    /// Value of a key, compared case-insensitively. `Author` also matches `Authors`.
    pub fn get(&self, key: &str) -> Option<&str> {
        self.fields.iter().find(|field| keys_match(&field.key, key)).map(|field| field.value.as_str())
    }

    /// Set, replace or (with an empty value) remove a key.
    pub fn set(&mut self, key: &str, value: &str) {
        let value = value.trim_end_matches(['\n', '\r']).to_string();
        if value.trim().is_empty() {
            self.fields.retain(|field| !keys_match(&field.key, key));
            return;
        }
        match self.fields.iter_mut().find(|field| keys_match(&field.key, key)) {
            Some(field) => field.value = value,
            None => self.fields.push(TitleField { key: key.to_string(), value }),
        }
    }
}

fn keys_match(a: &str, b: &str) -> bool {
    let canon = |key: &str| {
        let key = key.trim().to_ascii_lowercase();
        match key.as_str() {
            "authors" => "author".to_string(),
            _ => key,
        }
    };
    canon(a) == canon(b)
}

/// A screenplay.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Script {
    pub title_page: TitlePage,
    pub elements: Vec<Element>,
    /// Revision sets and the active set.
    #[serde(default, skip_serializing_if = "Revisions::is_empty")]
    pub revisions: Revisions,
    /// Page layout and element settings from the file it came from.
    #[serde(default, skip_serializing_if = "crate::format::Format::is_empty")]
    pub format: crate::format::Format,
    /// Final Draft data from an imported `.fdx` that the model does not use; written back on FDX export.
    #[serde(skip)]
    pub fdx: Option<Box<FdxDocument>>,
    /// Open Screenplay Format data from an imported `.osf` or `.fadein`; written back on OSF export.
    #[serde(skip)]
    pub osf: Option<Box<OsfDocument>>,
    /// Fountain boneyard blocks after the last paragraph (see [`Element::boneyard`]).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub boneyard_end: Vec<String>,
    /// Words the writer told spell check to ignore in this script, as written and each once in
    /// any case (`.fdx` SpellCheckIgnoreLists, OSF `<spelling><ignored>`; Fountain has no place
    /// for them). Spell check compares them without case.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub spelling_ignored: Vec<String>,
}

impl Script {
    /// A new, empty script with one scene heading to start typing in.
    pub fn blank() -> Self {
        Script { elements: vec![Element::new(ElementKind::SceneHeading, "")], ..Script::default() }
    }

    /// The positions of the script's New Act lines, in order.
    pub fn acts(&self) -> Vec<usize> {
        self.elements.iter().enumerate().filter(|(_, e)| e.kind == ElementKind::NewAct).map(|(i, _)| i).collect()
    }

    /// The act an element `wanted` names: its position counted from 1 (`2`), or its name
    /// (`TAG`, `act two`, or just `two` for ACT TWO), in any case.
    pub fn find_act(&self, wanted: &str) -> Option<usize> {
        let acts = self.acts();
        let wanted = wanted.trim();
        if let Ok(n) = wanted.parse::<usize>() {
            return n.checked_sub(1).and_then(|n| acts.get(n).copied());
        }
        let wanted = wanted.to_uppercase();
        let name = |i: &usize| self.elements.get(*i).map(|e| e.text.trim().to_uppercase()).unwrap_or_default();
        acts.iter().find(|i| name(i) == wanted).or_else(|| acts.iter().find(|i| name(i) == format!("ACT {wanted}"))).copied()
    }

    /// End of Act lines kept in step with their acts (docs/spec.md §3). Each act named the
    /// usual way (TEASER, ACT TWO, TAG; see [`act_kind`]) has its last End of Act line renamed
    /// to `END OF <act>` when that line names an act too; a writer's own line, such as END OF
    /// SHOW, is left alone. `add_end_for`, a New Act line's position, gets an End of Act line
    /// at the end of its act when it has none. Returns whether anything changed, and where a
    /// line was added.
    pub fn keep_acts_in_step(&mut self, add_end_for: Option<usize>) -> (bool, Option<usize>) {
        let managed = |text: &str| {
            let line = text.trim().to_uppercase();
            line.strip_prefix("END OF ").is_some_and(|name| act_kind(name) == Some(ElementKind::NewAct))
        };
        let acts = self.acts();
        let mut changed = false;
        let mut add_at = None;
        for (k, &act) in acts.iter().enumerate() {
            let end = acts.get(k + 1).copied().unwrap_or(self.elements.len());
            let Some(name) = self.elements.get(act).map(|e| e.text.trim().to_uppercase()) else { continue };
            if act_kind(&name) != Some(ElementKind::NewAct) {
                continue;
            }
            let want = format!("END OF {name}");
            let last_end = (act + 1..end).rev().find(|&i| self.elements.get(i).is_some_and(|e| e.kind == ElementKind::EndOfAct));
            match last_end.and_then(|i| self.elements.get_mut(i)) {
                Some(line) => {
                    if managed(&line.text) && line.text != want {
                        line.text = want;
                        changed = true;
                    }
                }
                None if add_end_for == Some(act) => add_at = Some((end, want)),
                None => {}
            }
        }
        let added = add_at.map(|(at, text)| {
            self.elements.insert(at, Element::new(ElementKind::EndOfAct, text));
            at
        });
        (changed || added.is_some(), added)
    }

    /// A new television script laid out in acts: each act's line, an empty scene heading to
    /// start writing in, and the line that ends the act.
    pub fn tv(shape: TvShape) -> Self {
        let acts: &[&str] = match shape {
            TvShape::OneHour => &["TEASER", "ACT ONE", "ACT TWO", "ACT THREE", "ACT FOUR", "ACT FIVE", "TAG"],
            TvShape::HalfHour => &["COLD OPEN", "ACT ONE", "ACT TWO", "ACT THREE", "TAG"],
            TvShape::MultiCam => &["COLD OPEN", "ACT ONE", "ACT TWO", "TAG"],
        };
        let mut elements = Vec::with_capacity(acts.len() * 3);
        for (n, act) in acts.iter().enumerate() {
            elements.push(Element::new(ElementKind::NewAct, *act));
            let mut heading = Element::new(ElementKind::SceneHeading, "");
            if shape == TvShape::MultiCam {
                heading.scene_number = Some(scene_letter(n));
            }
            elements.push(heading);
            elements.push(Element::new(ElementKind::EndOfAct, format!("END OF {act}")));
        }
        let mut script = Script { elements, ..Script::default() };
        if shape == TvShape::MultiCam {
            script.format = multi_cam_format();
        }
        script
    }

    /// Whether the scenes are lettered (A, B … Z, AA), as a multi-camera script's are: some
    /// heading has a number, and every number is letters only.
    pub fn is_lettered(&self) -> bool {
        let mut numbers = self.elements.iter().filter(|e| e.kind == ElementKind::SceneHeading).filter_map(|e| e.scene_number.as_deref()).peekable();
        numbers.peek().is_some() && numbers.all(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_uppercase()))
    }

    /// In a lettered script, every scene heading lettered in order (a new scene gets its
    /// letter, a moved one its new one). Returns whether anything changed.
    pub fn keep_letters_in_step(&mut self) -> bool {
        if !self.is_lettered() {
            return false;
        }
        let mut changed = false;
        for (n, heading) in self.elements.iter_mut().filter(|e| e.kind == ElementKind::SceneHeading).enumerate() {
            let letter = scene_letter(n);
            if heading.scene_number.as_deref() != Some(letter.as_str()) {
                heading.scene_number = Some(letter);
                changed = true;
            }
        }
        changed
    }

    /// The characters who speak in the scene headed at `heading`, in the order they first
    /// speak, as a multi-camera script lists them under the heading.
    pub fn scene_cast(&self, heading: usize) -> Vec<String> {
        let mut cast: Vec<String> = Vec::new();
        for element in self.elements.iter().skip(heading + 1) {
            match element.kind {
                ElementKind::SceneHeading | ElementKind::NewAct => break,
                ElementKind::Character => {
                    let name = crate::inline::plain(character_name(&element.text)).trim().to_uppercase();
                    if !name.is_empty() && !cast.contains(&name) {
                        cast.push(name);
                    }
                }
                _ => {}
            }
        }
        cast
    }

    /// Normalize every element (see [`Element::normalize`]).
    pub fn normalize(&mut self) {
        for element in &mut self.elements {
            element.normalize();
        }
    }

    /// Indices of every scene heading.
    pub fn scene_indices(&self) -> Vec<usize> {
        self.elements.iter().enumerate().filter(|(_, element)| element.kind == ElementKind::SceneHeading).map(|(index, _)| index).collect()
    }

    /// Number every scene heading 1, 2, 3… When `keep_existing` is set, headings that
    /// already have a number keep it and new ones get letters (see [`Script::number_with`]).
    pub fn number_scenes(&mut self, keep_existing: bool) {
        self.number_with(Numbering { keep_existing, ..Numbering::default() });
    }

    /// Numbers the scene headings as `options` says (docs/production-spec.md §3.1): how many
    /// headings changed, and the new scenes left unnumbered because no number sits between
    /// their neighbours (a new scene between `2` and an existing `2A`).
    ///
    /// Without `keep_existing`, the headings from `from` on are numbered `start`, `start + 1` …
    /// With it, numbered headings keep their numbers; an unnumbered one after `12` becomes `12A`,
    /// then `12B`; one before the first numbered heading (`1`) becomes `A1`, then `B1`. A
    /// number the script already uses is never given twice: the next letter is tried, then a
    /// further one (`12AA`).
    pub fn number_with(&mut self, options: Numbering) -> Numbered {
        let headings: Vec<usize> = self.scene_indices();
        let mut changed = 0;
        let mut set = |elements: &mut Vec<Element>, at: usize, number: String| {
            if let Some(element) = elements.get_mut(at)
                && element.scene_number.as_deref() != Some(number.as_str())
            {
                element.scene_number = Some(number);
                changed += 1;
            }
        };
        if !options.keep_existing || headings.iter().all(|&h| self.elements.get(h).is_none_or(|e| e.scene_number.is_none())) {
            let mut next = options.start.max(1);
            for &h in headings.iter().filter(|&&h| h >= options.from) {
                set(&mut self.elements, h, next.to_string());
                next = next.saturating_add(1);
            }
            return Numbered { changed, unplaced: Vec::new() };
        }
        let mut unplaced = Vec::new();
        let mut taken: std::collections::HashSet<String> =
            headings.iter().filter_map(|&h| self.elements.get(h).and_then(|e| e.scene_number.clone())).collect();
        let mut previous: Option<String> = None;
        let mut position = 0;
        while let Some(&h) = headings.get(position) {
            if let Some(number) = self.elements.get(h).and_then(|e| e.scene_number.clone()) {
                previous = Some(number);
                position += 1;
                continue;
            }
            if h < options.from {
                position += 1;
                continue;
            }
            match &previous {
                // After a numbered scene: its number with the next letter.
                Some(before) => {
                    // The next numbered scene bounds it: a new number must sort between the two.
                    let after = headings.iter().skip(position + 1).find_map(|&k| self.elements.get(k).and_then(|e| e.scene_number.clone()));
                    let fits = |number: &str| {
                        !number.is_empty() && !taken.contains(number) && after.as_deref().is_none_or(|after| sorts_before(number, after))
                    };
                    match (0..INSERT_TRIES).map(|n| insert_after(before, n, options.skip_io)).find(|n| fits(n)) {
                        Some(number) => {
                            taken.insert(number.clone());
                            set(&mut self.elements, h, number.clone());
                            previous = Some(number);
                        }
                        None => unplaced.push(Unplaced { heading: h, before: before.clone(), after: after.unwrap_or_default() }),
                    }
                    position += 1;
                }
                // Before the first numbered scene: letters in front of its number.
                None => {
                    let first =
                        headings.iter().skip(position).find_map(|&k| self.elements.get(k).and_then(|e| e.scene_number.clone())).unwrap_or_default();
                    let mut letter = 0;
                    while let Some(&k) = headings.get(position).filter(|&&k| self.elements.get(k).is_some_and(|e| e.scene_number.is_none())) {
                        let number = free(&taken, |n| format!("{}{first}", letters(letter + n, options.skip_io)));
                        taken.insert(number.clone());
                        set(&mut self.elements, k, number);
                        letter += 1;
                        position += 1;
                    }
                }
            }
        }
        Numbered { changed, unplaced }
    }

    /// Omits the scene headed at `heading`: its heading reads `OMITTED`, keeps its number, and
    /// keeps the whole scene (as Fountain) for Unomit. Returns the elements taken out.
    pub fn omit_scene(&mut self, heading: usize) -> Result<usize, String> {
        let element = self.elements.get(heading).filter(|e| e.kind == ElementKind::SceneHeading).ok_or("that is not a scene heading")?;
        if element.is_omitted() {
            return Err("that scene is already omitted".into());
        }
        let end = self.elements.iter().enumerate().skip(heading + 1).find(|(_, e)| ends_scene(e.kind)).map_or(self.elements.len(), |(i, _)| i);
        let taken: Vec<Element> = self.elements.drain(heading..end).collect();
        let kept = Script { elements: taken.iter().map(|e| Element { scene_number: None, ..e.clone() }).collect(), ..Script::default() };
        let mut omitted = Element::new(ElementKind::SceneHeading, OMITTED);
        omitted.scene_number = taken.first().and_then(|e| e.scene_number.clone());
        omitted.scene = taken.first().map(|e| e.scene.clone()).unwrap_or_default();
        omitted.omitted = Some(crate::fountain::write(&kept));
        self.elements.insert(heading, omitted);
        Ok(taken.len())
    }

    /// Brings an omitted scene back where it was, its number on its heading again. Refuses
    /// when the file kept nothing to bring back.
    pub fn unomit_scene(&mut self, heading: usize) -> Result<usize, String> {
        let element = self.elements.get(heading).filter(|e| e.is_omitted()).ok_or("that scene is not omitted")?;
        let kept = element.omitted.clone().unwrap_or_default();
        if kept.trim().is_empty() {
            return Err("the file kept nothing of this scene to bring back".into());
        }
        let number = element.scene_number.clone();
        let mut back = crate::fountain::parse(&kept).map_err(|e| e.to_string())?.elements;
        if let Some(first) = back.first_mut().filter(|e| e.kind == ElementKind::SceneHeading) {
            first.scene_number = number;
            first.scene = element.scene.clone();
        }
        let count = back.len();
        self.elements.splice(heading..=heading, back);
        Ok(count)
    }

    /// Sets one scene's number by hand (empty clears it). Refuses a number another scene has,
    /// naming that scene's position.
    pub fn set_scene_number(&mut self, heading: usize, number: &str) -> Result<(), String> {
        let number = number.trim().to_uppercase();
        if !number.is_empty()
            && let Some(other) = self
                .scene_indices()
                .iter()
                .position(|&h| h != heading && self.elements.get(h).and_then(|e| e.scene_number.as_deref()) == Some(number.as_str()))
        {
            return Err(format!("scene {} already has the number {number}", other + 1));
        }
        match self.elements.get_mut(heading).filter(|e| e.kind == ElementKind::SceneHeading) {
            Some(element) => {
                element.scene_number = (!number.is_empty()).then_some(number);
                Ok(())
            }
            None => Err("that is not a scene heading".into()),
        }
    }

    /// Whether the script's pages are locked: some paragraph starts a locked page.
    pub fn pages_locked(&self) -> bool {
        self.elements.iter().any(|e| !e.page_starts.is_empty())
    }

    /// Remove every scene number.
    pub fn clear_scene_numbers(&mut self) {
        for element in &mut self.elements {
            element.scene_number = None;
        }
    }
}
