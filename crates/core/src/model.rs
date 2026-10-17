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
    /// Final Draft data this paragraph carried that the model does not use (see [`FdxParagraph`]).
    #[serde(skip)]
    pub fdx: Option<Box<FdxParagraph>>,
    /// Open Screenplay Format data this paragraph carried that the model does not use.
    #[serde(skip)]
    pub osf: Option<Box<OsfParagraph>>,
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
        let mut element = Element { kind, text: text.into(), scene_number: None, dual: false, level: 0, fdx: None, osf: None };
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
        Script { title_page: TitlePage::default(), elements: vec![Element::new(ElementKind::SceneHeading, "")], fdx: None, osf: None }
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

    /// Remove every scene number.
    pub fn clear_scene_numbers(&mut self) {
        for element in &mut self.elements {
            element.scene_number = None;
        }
    }
}
