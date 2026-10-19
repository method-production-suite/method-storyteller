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
}

impl ElementKind {
    /// Every kind, in menu order.
    pub const ALL: [ElementKind; 12] = [
        ElementKind::SceneHeading,
        ElementKind::Action,
        ElementKind::Character,
        ElementKind::Parenthetical,
        ElementKind::Dialogue,
        ElementKind::Transition,
        ElementKind::Shot,
        ElementKind::Centered,
        ElementKind::Lyrics,
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
        }
    }

    /// Kinds whose text is always shown and saved in capitals.
    pub fn is_caps(self) -> bool {
        matches!(self, ElementKind::SceneHeading | ElementKind::Character | ElementKind::Transition | ElementKind::Shot)
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
    /// The page layout and element settings as read; their sections are written back as they
    /// were while they are the same.
    pub format: Option<crate::format::Format>,
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
}

impl Script {
    /// A new, empty script with one scene heading to start typing in.
    pub fn blank() -> Self {
        Script { elements: vec![Element::new(ElementKind::SceneHeading, "")], ..Script::default() }
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
    /// already have a number keep it.
    pub fn number_scenes(&mut self, keep_existing: bool) {
        let mut next: u32 = 1;
        for element in &mut self.elements {
            if element.kind != ElementKind::SceneHeading {
                continue;
            }
            if keep_existing && let Some(existing) = &element.scene_number {
                if let Ok(value) = existing.trim().parse::<u32>() {
                    next = value.saturating_add(1);
                }
                continue;
            }
            element.scene_number = Some(next.to_string());
            next = next.saturating_add(1);
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
