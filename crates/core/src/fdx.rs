//! Final Draft XML (`.fdx`) import and export.
//!
//! Clean-room: built from the publicly known structure of the format (`FinalDraft` →
//! `Content` → `Paragraph[@Type]` → `Text[@Style]`, `DualDialogue`, `TitlePage`), from public
//! sample files and from files this module writes (see docs/research/v1-notes/fdx-format.md).
//!
//! Everything we do not model is kept and written back: other top-level sections (element
//! settings, page layout, revisions, tags …) verbatim in [`FdxDocument`], and each paragraph's
//! own type name, extra attributes and non-text children (scene properties, notes, omitted
//! scenes) in [`FdxParagraph`]. A run's `RevisionID` becomes the paragraph's revised spans and the
//! `Revisions` section the script's revision sets; other run attributes (font, size, colour) are
//! not kept yet.
//!
//! Script notes come in three forms (docs/research/v1-notes/fdx-format.md): a `ScriptNote` inside
//! its paragraph among the text runs, the top-level `ScriptNotes` list placed by `Range`
//! (Version 5 and later: character offsets into the script, each paragraph break counting one),
//! and the older top-level `UnanchoredScriptNotes`. All three become [`ScriptNote`]s on their
//! paragraph and are written back in the form they came in.
//!
//! `LockedPages` entries (`PageNumber`, `Position`) become [`PageStart`]s on the paragraph each
//! page starts in. `Position` counts like `ScriptNotes@Range`.

use quick_xml::XmlVersion;
use quick_xml::events::{BytesRef, BytesStart, Event};

use crate::inline::{self, Run, Style};
use crate::layout::{Align, ElementStyle, Layout, chars_per_line};
use crate::model::SceneMeta;
use crate::model::{
    Deletion, Element, ElementKind, FdxDocument, FdxNote, FdxNoteForm, FdxParagraph, FdxSection, PageStart, Revised, RevisionSet, Revisions, Script,
    ScriptNote, TitleField, TitlePage, mark_revised,
};
use crate::paginate;
use crate::{CoreError, MAX_ELEMENTS, MAX_INPUT_BYTES};

/// Deepest element nesting accepted.
const MAX_DEPTH: usize = 128;
/// Our own marker for which title-page field a paragraph holds (ignored by other apps).
const FIELD_ATTR: &str = "StorytellerField";
/// Our own marker on a script note written for a plain `[[note]]`, so it reads back as one.
const INLINE_NOTE_ATTR: &str = "StorytellerInline";

/// The paragraph type written for an element: its imported type while its kind is unchanged,
/// otherwise Final Draft's name for the kind. Sections and synopses use Final Draft's
/// `Outline 1`–`Outline 4` and `Summary`.
pub(crate) fn fdx_type(element: &Element) -> String {
    if let Some(fdx) = &element.fdx
        && fdx.kind == element.kind
    {
        return fdx.type_name.clone();
    }
    match element.kind {
        ElementKind::SceneHeading => "Scene Heading".into(),
        ElementKind::Action | ElementKind::Centered | ElementKind::PageBreak => "Action".into(),
        ElementKind::Character => "Character".into(),
        ElementKind::Parenthetical => "Parenthetical".into(),
        ElementKind::Dialogue => "Dialogue".into(),
        ElementKind::Transition => "Transition".into(),
        ElementKind::Shot => "Shot".into(),
        ElementKind::Lyrics => "Lyrics".into(),
        ElementKind::Section => format!("Outline {}", element.level.clamp(1, 4)),
        ElementKind::Synopsis => "Summary".into(),
        ElementKind::NewAct => "New Act".into(),
        ElementKind::EndOfAct => "End of Act".into(),
    }
}

/// The kind for a Final Draft paragraph type, and the outline level for `Outline N`. Types the
/// model has no kind for print as action; [`apply_hidden_types`] then hides the ones the file hides.
fn kind_from_fdx(name: &str) -> (ElementKind, Option<u8>) {
    let name = name.trim();
    if let Some(level) = name.strip_prefix("Outline ").and_then(|n| n.trim().parse::<u8>().ok()) {
        return (ElementKind::Section, Some(level));
    }
    let kind = match name {
        "Scene Heading" => ElementKind::SceneHeading,
        "Character" => ElementKind::Character,
        "Parenthetical" => ElementKind::Parenthetical,
        "Dialogue" => ElementKind::Dialogue,
        "Transition" => ElementKind::Transition,
        "Shot" => ElementKind::Shot,
        "Lyrics" => ElementKind::Lyrics,
        "New Act" => ElementKind::NewAct,
        "End of Act" => ElementKind::EndOfAct,
        // Files from earlier Method Storyteller builds.
        "Section" => ElementKind::Section,
        "Synopsis" | "Summary" | "Outline Body" => ElementKind::Synopsis,
        _ => ElementKind::Action,
    };
    (kind, None)
}

#[path = "fdx_layout.rs"]
mod fdx_layout;
#[path = "fdx_notes.rs"]
mod fdx_notes;
#[path = "fdx_pages.rs"]
mod fdx_pages;
#[path = "fdx_read.rs"]
mod fdx_read;
#[path = "fdx_revisions.rs"]
mod fdx_revisions;
#[path = "fdx_spelling.rs"]
mod fdx_spelling;
#[path = "fdx_write.rs"]
mod fdx_write;

pub use fdx_layout::*;
pub(crate) use fdx_notes::*;
pub(crate) use fdx_pages::*;
pub use fdx_read::*;
pub(crate) use fdx_revisions::*;
pub(crate) use fdx_spelling::*;
pub use fdx_write::*;
