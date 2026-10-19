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
use crate::model::{
    Element, ElementKind, FdxDocument, FdxNote, FdxNoteForm, FdxParagraph, FdxSection, PageStart, Revised, RevisionSet, Revisions, Script,
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
fn fdx_type(element: &Element) -> String {
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
        // Files from earlier Method Storyteller builds.
        "Section" => ElementKind::Section,
        "Synopsis" | "Summary" | "Outline Body" => ElementKind::Synopsis,
        _ => ElementKind::Action,
    };
    (kind, None)
}

// ---------------------------------------------------------------------------------------------
// Export

pub(crate) fn escape(text: &str, out: &mut String) {
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            '\t' | '\n' => out.push(c),
            '\r' => {}
            c if (c as u32) < 0x20 || c == '\u{fffe}' || c == '\u{ffff}' => {}
            c => out.push(c),
        }
    }
}

fn style_attr(style: Style) -> String {
    let mut parts = Vec::new();
    if style.bold {
        parts.push("Bold");
    }
    if style.italic {
        parts.push("Italic");
    }
    if style.underline {
        parts.push("Underline");
    }
    parts.join("+")
}

fn write_text_runs(markup: &str, indent: &str, out: &mut String) {
    let runs = inline::parse(markup);
    if runs.is_empty() {
        out.push_str(indent);
        out.push_str("<Text></Text>\n");
        return;
    }
    for run in runs {
        out.push_str(indent);
        let style = style_attr(run.style);
        if style.is_empty() {
            out.push_str("<Text>");
        } else {
            out.push_str(&format!("<Text Style=\"{style}\">"));
        }
        // Notes have no inline equivalent in FDX; keep them visible as Fountain notes.
        let text = if run.note { format!("[[{}]]", run.text) } else { run.text };
        escape(&text, out);
        out.push_str("</Text>\n");
    }
}

fn write_run(text: &str, style: Style, revision: Option<u32>, indent: &str, out: &mut String) {
    out.push_str(indent);
    out.push_str("<Text");
    if let Some(revision) = revision {
        out.push_str(&format!(" RevisionID=\"{revision}\""));
    }
    let style = style_attr(style);
    if !style.is_empty() {
        out.push_str(&format!(" Style=\"{style}\""));
    }
    out.push('>');
    escape(text, out);
    out.push_str("</Text>\n");
}

/// A paragraph's text runs with its notes among them: a plain `[[note]]` where it sits in the
/// text, a script note kept inside the paragraph at the start of its range. Runs split where
/// revised text starts and ends, and revised runs carry their set's `RevisionID`.
fn write_body(element: &Element, indent: &str, out: &mut String) {
    let mut notes: Vec<(u32, &ScriptNote)> =
        element.notes.iter().filter(|n| note_form(n) == FdxNoteForm::Inline).map(|n| (n.range.map_or(0, |r| r.0), n)).collect();
    notes.sort_by_key(|(at, _)| *at);
    let mut pending = notes.into_iter().peekable();
    let mut at: u32 = 0;
    let mut wrote_text = false;
    let mut revisions = element.revision_cursor();
    for run in inline::parse(&element.text) {
        if run.note {
            out.push_str(indent);
            out.push_str(&plain_note_xml(&run.text));
            out.push('\n');
            continue;
        }
        let chars: Vec<char> = run.text.chars().collect();
        let len = u32::try_from(chars.len()).unwrap_or(u32::MAX);
        let mut from = 0u32;
        loop {
            while let Some((_, note)) = pending.next_if(|(pos, _)| *pos <= at.saturating_add(from)) {
                out.push_str(indent);
                out.push_str(&inline_note_xml(note));
                out.push('\n');
            }
            let here = at.saturating_add(from);
            let edge = [pending.peek().map(|(pos, _)| *pos), revisions.next_edge(here)].into_iter().flatten().min();
            let to = edge.map(|edge| edge.saturating_sub(at)).filter(|end| *end < len).unwrap_or(len);
            let piece: String = chars.get(from as usize..to as usize).unwrap_or_default().iter().collect();
            write_run(&piece, run.style, revisions.set_at(here), indent, out);
            wrote_text = true;
            from = to;
            if from >= len {
                break;
            }
        }
        at = at.saturating_add(len);
    }
    for (_, note) in pending {
        out.push_str(indent);
        out.push_str(&inline_note_xml(note));
        out.push('\n');
    }
    if !wrote_text {
        out.push_str(indent);
        out.push_str("<Text></Text>\n");
    }
}

fn write_attr(key: &str, value: &str, out: &mut String) {
    out.push(' ');
    out.push_str(key);
    out.push_str("=\"");
    // In an attribute XML turns literal line breaks and tabs into spaces, so they go as references.
    for c in value.chars() {
        match c {
            '\n' => out.push_str("&#10;"),
            '\t' => out.push_str("&#9;"),
            '\r' => out.push_str("&#13;"),
            c => escape(c.encode_utf8(&mut [0; 4]), out),
        }
    }
    out.push('"');
}

/// `tag` (a start tag such as `<SceneProperties Length="1" Page="3"/>`) with `name` set to `value`.
pub(crate) fn set_tag_attr(tag: &str, name: &str, value: &str) -> String {
    let key = format!(" {name}=\"");
    if let Some(at) = tag.find(&key) {
        let start = at + key.len();
        if let Some(len) = tag.get(start..).and_then(|rest| rest.find('"')) {
            return format!("{}{value}{}", tag.get(..start).unwrap_or_default(), tag.get(start + len..).unwrap_or_default());
        }
    }
    let close = if tag.ends_with("/>") { tag.len() - 2 } else { tag.len().saturating_sub(1) };
    format!("{} {name}=\"{value}\"{}", tag.get(..close).unwrap_or_default().trim_end(), tag.get(close..).unwrap_or_default())
}

/// A scene heading's children with its `SceneProperties` page and length set from our own
/// pagination: updated in place when the file had one, otherwise added first.
fn scene_children(children: &str, page: u32, eighths: u32) -> String {
    let length = paginate::format_eighths(eighths);
    let page = page.to_string();
    // Only the paragraph's own properties, which Final Draft writes first: an omitted scene keeps
    // its original heading, with properties of its own, nested further in.
    let start = children.len() - children.trim_start().len();
    if children.trim_start().starts_with("<SceneProperties")
        && let Some(len) = children.get(start..).and_then(|rest| rest.find('>'))
    {
        let end = start + len + 1;
        let tag = children.get(start..end).unwrap_or_default();
        let tag = set_tag_attr(&set_tag_attr(tag, "Length", &length), "Page", &page);
        return format!("{}{tag}{}", children.get(..start).unwrap_or_default(), children.get(end..).unwrap_or_default());
    }
    format!("<SceneProperties Length=\"{length}\" Page=\"{page}\" Title=\"\"/>{children}")
}

fn write_paragraph(element: &Element, scene: Option<(u32, u32)>, new_page: bool, indent: &str, out: &mut String) {
    out.push_str(indent);
    out.push_str("<Paragraph");
    write_attr("Type", &fdx_type(element), out);
    if element.kind == ElementKind::Centered {
        out.push_str(" Alignment=\"Center\"");
    }
    if let Some(fdx) = &element.fdx {
        for (key, value) in &fdx.attributes {
            write_attr(key, value, out);
        }
    }
    if let Some(number) = &element.scene_number {
        out.push_str(" Number=\"");
        escape(number, out);
        out.push('"');
    }
    if new_page {
        out.push_str(" StartsNewPage=\"Yes\"");
    }
    out.push_str(">\n");
    let kept = element.fdx.as_ref().map_or("", |f| f.children.as_str());
    let children = match scene {
        Some((page, eighths)) => scene_children(kept, page, eighths),
        None => kept.to_string(),
    };
    if !children.is_empty() {
        out.push_str(indent);
        out.push_str("  ");
        out.push_str(&children);
        out.push('\n');
    }
    write_body(element, &format!("{indent}  "), out);
    out.push_str(indent);
    out.push_str("</Paragraph>\n");
}

fn speech_end(elements: &[Element], cue: usize) -> usize {
    let mut end = cue + 1;
    while elements.get(end).is_some_and(|e| e.kind.is_dialogue_part() || e.kind == ElementKind::Lyrics) {
        end += 1;
    }
    end
}

/// Write a script as Final Draft XML, paginated with [`Layout::for_script`].
pub fn write(script: &Script) -> String {
    write_with_layout(script, &Layout::for_script(script))
}

/// Write a script as Final Draft XML. Each scene heading's `SceneProperties` gets the page it
/// starts on and its length in eighths from paginating with `layout`, as Final Draft records them.
pub fn write_with_layout(script: &Script, layout: &Layout) -> String {
    let paginated = paginate::paginate(script, layout);
    let scenes: std::collections::HashMap<usize, (u32, u32)> =
        paginate::scene_lengths(script, &paginated, layout).into_iter().map(|s| (s.element, (s.page, s.eighths))).collect();
    let mut out = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"no\" ?>\n");
    match script.fdx.as_ref().filter(|d| !d.root_attributes.is_empty()) {
        Some(document) => {
            out.push_str("<FinalDraft");
            for (key, value) in &document.root_attributes {
                write_attr(key, value, &mut out);
            }
            out.push_str(">\n  <Content>\n");
        }
        None => out.push_str("<FinalDraft DocumentType=\"Script\" Template=\"No\" Version=\"5\">\n  <Content>\n"),
    }
    let elements = &script.elements;
    let mut new_page = false;
    let mut index = 0;
    while let Some(element) = elements.get(index) {
        if element.kind == ElementKind::PageBreak {
            new_page = true;
            index += 1;
            continue;
        }
        if element.kind == ElementKind::Character && !element.dual {
            let first_end = speech_end(elements, index);
            if elements.get(first_end).is_some_and(|e| e.kind == ElementKind::Character && e.dual) {
                let second_end = speech_end(elements, first_end);
                out.push_str("    <Paragraph");
                if new_page {
                    out.push_str(" StartsNewPage=\"Yes\"");
                }
                out.push_str(">\n      <DualDialogue>\n");
                for part in elements.get(index..second_end).unwrap_or(&[]) {
                    write_paragraph(part, None, false, "        ", &mut out);
                }
                out.push_str("      </DualDialogue>\n    </Paragraph>\n");
                new_page = false;
                index = second_end;
                continue;
            }
        }
        write_paragraph(element, scenes.get(&index).copied(), new_page, "    ", &mut out);
        new_page = false;
        index += 1;
    }
    out.push_str("  </Content>\n");
    let sections = script.fdx.as_ref().map(|d| d.sections.as_slice()).unwrap_or_default();
    let ranged = ranged_notes(elements);
    let unanchored: Vec<&ScriptNote> = elements.iter().flat_map(|e| &e.notes).filter(|n| note_form(n) == FdxNoteForm::Unanchored).collect();
    let (read_ranged, read_unanchored) = match script.fdx.as_deref() {
        Some(document) => (document.ranged_notes.as_slice(), document.unanchored_notes.as_slice()),
        None => (&[][..], &[][..]),
    };
    let ranged_same =
        ranged.len() == read_ranged.len() && ranged.iter().zip(read_ranged).all(|((s, e, n), (rs, re, rn))| s == rs && e == re && *n == rn);
    let unanchored_same = unanchored.len() == read_unanchored.len() && unanchored.iter().zip(read_unanchored).all(|(n, r)| *n == r);
    let (mut ranged_written, mut unanchored_written, mut revisions_written, mut locks_written) = (false, false, false, false);
    let locks = locked_pages(elements);
    let read_locks = script.fdx.as_deref().map(|d| d.locked_pages.as_slice()).unwrap_or_default();
    let locks_same = locks.len() == read_locks.len() && locks.iter().zip(read_locks).all(|((p, s), (rp, rs))| p == rp && *s == rs);
    let mut title_written = false;
    for section in sections {
        match section {
            FdxSection::Raw(xml) if section_is(xml, "ScriptNotes") => {
                if ranged_same {
                    out.push_str("  ");
                    out.push_str(xml);
                    out.push('\n');
                } else {
                    write_note_list("ScriptNotes", ranged.iter().map(|(s, e, n)| ranged_note_xml(n, *s, *e)), &mut out);
                }
                ranged_written = true;
            }
            FdxSection::Raw(xml) if section_is(xml, "Revisions") && !revisions_written => {
                if script.fdx.as_ref().and_then(|d| d.revisions.as_ref()) == Some(&script.revisions) {
                    out.push_str("  ");
                    out.push_str(xml);
                    out.push('\n');
                } else {
                    write_revisions(&script.revisions, &mut out);
                }
                revisions_written = true;
            }
            FdxSection::Raw(xml) if section_is(xml, "LockedPages") && !locks_written => {
                if locks_same {
                    out.push_str("  ");
                    out.push_str(xml);
                    out.push('\n');
                } else {
                    let root = tags(xml).into_iter().next().map(|(_, a)| a).unwrap_or_default();
                    write_locked_pages(&root, &locks, &mut out);
                }
                locks_written = true;
            }
            FdxSection::Raw(xml) if section_is(xml, "UnanchoredScriptNotes") => {
                if unanchored_same {
                    out.push_str("  ");
                    out.push_str(xml);
                    out.push('\n');
                } else {
                    write_note_list("UnanchoredScriptNotes", unanchored.iter().map(|n| inline_note_xml(n)), &mut out);
                }
                unanchored_written = true;
            }
            FdxSection::Raw(xml) => {
                out.push_str("  ");
                out.push_str(xml);
                out.push('\n');
            }
            FdxSection::TitlePage { raw, parsed } if !title_written => {
                // An unchanged title page goes back exactly as Final Draft laid it out.
                if *parsed == script.title_page {
                    out.push_str("  ");
                    out.push_str(raw);
                    out.push('\n');
                } else {
                    write_title_page(&script.title_page, &mut out);
                }
                title_written = true;
            }
            FdxSection::TitlePage { .. } => {}
        }
    }
    if !title_written {
        write_title_page(&script.title_page, &mut out);
    }
    if !locks_written && !locks.is_empty() {
        write_locked_pages(&[], &locks, &mut out);
    }
    if !revisions_written && !script.revisions.is_empty() {
        write_revisions(&script.revisions, &mut out);
    }
    if !ranged_written && !ranged.is_empty() {
        write_note_list("ScriptNotes", ranged.iter().map(|(s, e, n)| ranged_note_xml(n, *s, *e)), &mut out);
    }
    if !unanchored_written && !unanchored.is_empty() {
        write_note_list("UnanchoredScriptNotes", unanchored.iter().map(|n| inline_note_xml(n)), &mut out);
    }
    out.push_str("</FinalDraft>\n");
    out
}

fn write_title_page(title: &TitlePage, out: &mut String) {
    if title.fields.is_empty() {
        return;
    }
    out.push_str("  <TitlePage>\n    <Content>\n");
    for field in &title.fields {
        let key = field.key.to_ascii_lowercase();
        let alignment = match key.as_str() {
            "contact" | "copyright" | "notes" => "Left",
            "draft date" | "date" => "Right",
            _ => "Center",
        };
        for line in field.value.split('\n') {
            out.push_str(&format!("      <Paragraph Alignment=\"{alignment}\" Type=\"Title Page\" {FIELD_ATTR}=\""));
            escape(&field.key, out);
            out.push_str("\">\n");
            write_text_runs(line, "        ", out);
            out.push_str("      </Paragraph>\n");
        }
    }
    out.push_str("    </Content>\n  </TitlePage>\n");
}

// ---------------------------------------------------------------------------------------------
// Import

/// Where an open script or title-page paragraph sits.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Region {
    Script,
    Title,
}

struct ParagraphState {
    region: Region,
    /// Stack depth of the `Paragraph` element itself.
    depth: usize,
    kind: Option<String>,
    alignment: Option<String>,
    number: Option<String>,
    level: u8,
    new_page: bool,
    field: Option<String>,
    attributes: Vec<(String, String)>,
    children: String,
    runs: Vec<Run>,
    notes: Vec<ScriptNote>,
    revised: Vec<Revised>,
    /// Characters of printed text read so far.
    chars: u32,
    has_text: bool,
    /// The paragraph wraps a dual-dialogue block; it is not an element itself.
    wraps_dual: bool,
}

impl ParagraphState {
    /// Characters of printed text read so far.
    fn printed_chars(&self) -> u32 {
        self.chars
    }
}

struct TitleParagraph {
    field: Option<String>,
    alignment: String,
    text: String,
}

/// A child element being copied verbatim, from its start tag to its end tag.
struct Capture {
    depth: usize,
    start: usize,
    into: CaptureInto,
}

#[derive(Clone, Copy)]
enum CaptureInto {
    Section,
    Paragraph,
    /// A `ScriptNote` inside a script paragraph.
    Note,
}

/// The text style of the `Text` run being read: our style plus Final Draft's `AllCaps`.
#[derive(Clone, Copy)]
struct RunStyle {
    style: Style,
    all_caps: bool,
    revision: u32,
}

/// Attributes the model holds itself, so they are not kept in [`FdxParagraph::attributes`].
const MODELLED_ATTRIBUTES: [&str; 5] = ["Type", "Number", "StartsNewPage", "Level", FIELD_ATTR];

fn attr(element: &BytesStart<'_>, name: &str) -> Option<String> {
    element
        .attributes()
        .flatten()
        .find(|a| a.key.as_ref() == name.as_bytes())
        .and_then(|a| a.normalized_value(XmlVersion::Implicit1_0).ok())
        .map(|v| v.into_owned())
}

pub(crate) fn all_attrs(element: &BytesStart<'_>) -> Vec<(String, String)> {
    element
        .attributes()
        .flatten()
        .filter_map(|a| {
            let value = a.normalized_value(XmlVersion::Implicit1_0).ok()?;
            Some((String::from_utf8_lossy(a.key.as_ref()).into_owned(), value.into_owned()))
        })
        .collect()
}

fn parse_style(value: Option<String>) -> RunStyle {
    let mut run = RunStyle { style: Style::default(), all_caps: false, revision: 0 };
    for part in value.unwrap_or_default().split('+') {
        match part.trim() {
            "Bold" => run.style.bold = true,
            "Italic" => run.style.italic = true,
            "Underline" => run.style.underline = true,
            "AllCaps" => run.all_caps = true,
            _ => {}
        }
    }
    run
}

fn xml_error(error: impl std::fmt::Display) -> CoreError {
    CoreError::Xml(error.to_string().chars().take(200).collect())
}

/// The text an entity or character reference stands for.
fn reference_text(reference: &BytesRef<'_>) -> Result<String, CoreError> {
    Ok(match reference.resolve_char_ref().map_err(xml_error)? {
        Some(c) => c.to_string(),
        None => {
            let name = reference.decode().map_err(xml_error)?;
            quick_xml::escape::resolve_predefined_entity(&name).unwrap_or("").to_string()
        }
    })
}

fn names(stack: &[String]) -> Vec<&str> {
    stack.iter().map(String::as_str).collect()
}

/// The region of a `Paragraph` at the top of `stack`, if it is a script or title-page paragraph.
/// Only direct children of `Content` (or of a dual-dialogue block) count: paragraphs nested in
/// scene properties, notes or omitted scenes belong to their parent and are kept verbatim.
fn paragraph_region(stack: &[String]) -> Option<Region> {
    match names(stack).as_slice() {
        ["FinalDraft", "Content", "Paragraph"] | ["FinalDraft", "Content", "Paragraph", "DualDialogue", "Paragraph"] => Some(Region::Script),
        ["FinalDraft", "TitlePage", "Content", "Paragraph"] => Some(Region::Title),
        _ => None,
    }
}

/// Everything collected while reading, finished into a [`Script`] at the end.
#[derive(Default)]
struct Importer {
    stack: Vec<String>,
    paragraphs: Vec<ParagraphState>,
    run: Option<RunStyle>,
    in_dual: bool,
    dual_cues: usize,
    elements: Vec<Element>,
    title: Vec<TitleParagraph>,
    pending_break: bool,
    document: FdxDocument,
    title_start: Option<usize>,
    /// Where the next script paragraph starts in the script's text, as `ScriptNotes@Range` counts.
    offset: u32,
    /// Where each element starts in the script's text (`None` for page breaks).
    starts: Vec<Option<u32>>,
    notes: usize,
}

impl Importer {
    fn push(&mut self, name: &str) -> Result<(), CoreError> {
        if self.stack.len() >= MAX_DEPTH {
            return Err(CoreError::Xml("elements are nested too deeply".into()));
        }
        self.stack.push(name.to_string());
        Ok(())
    }

    /// The open paragraph whose direct child is the element at the top of the stack.
    fn parent_paragraph(&mut self) -> Option<&mut ParagraphState> {
        let depth = self.stack.len();
        self.paragraphs.last_mut().filter(|p| p.depth + 1 == depth)
    }

    /// Handles a start tag already pushed on the stack. Returns where to copy it verbatim instead.
    fn open(&mut self, name: &str, start: &BytesStart<'_>, at: usize) -> Option<CaptureInto> {
        let depth = self.stack.len();
        if depth == 1 {
            self.document.root_attributes = all_attrs(start);
            return None;
        }
        if depth == 2 {
            return match name {
                "Content" => None,
                "TitlePage" => {
                    self.title_start = Some(at);
                    None
                }
                _ => Some(CaptureInto::Section),
            };
        }
        if name == "Paragraph"
            && let Some(region) = paragraph_region(&self.stack)
        {
            let level = attr(start, "Level").and_then(|v| v.trim().parse::<u8>().ok()).unwrap_or(1);
            let attributes = all_attrs(start).into_iter().filter(|(key, _)| !MODELLED_ATTRIBUTES.contains(&key.as_str())).collect();
            let new_page = attr(start, "StartsNewPage").is_some_and(|v| v.eq_ignore_ascii_case("yes"));
            // At the start tag, so a dual-dialogue wrapper's break comes before its speeches.
            if new_page && region == Region::Script {
                self.pending_break = true;
            }
            self.paragraphs.push(ParagraphState {
                region,
                depth,
                kind: attr(start, "Type"),
                alignment: attr(start, "Alignment"),
                number: attr(start, "Number").filter(|n| !n.trim().is_empty()),
                level,
                new_page,
                field: attr(start, FIELD_ATTR),
                attributes,
                children: String::new(),
                runs: Vec::new(),
                notes: Vec::new(),
                revised: Vec::new(),
                chars: 0,
                has_text: false,
                wraps_dual: false,
            });
            return None;
        }
        let parent = self.parent_paragraph().map(|p| p.region);
        match (name, parent) {
            ("Text", Some(_)) => {
                let mut run = parse_style(attr(start, "Style"));
                run.revision = attr(start, "RevisionID").and_then(|r| r.trim().parse().ok()).unwrap_or(0);
                self.run = Some(run);
                if let Some(paragraph) = self.parent_paragraph() {
                    paragraph.has_text = true;
                }
                None
            }
            ("DualDialogue", Some(Region::Script)) => {
                self.in_dual = true;
                self.dual_cues = 0;
                if let Some(paragraph) = self.parent_paragraph() {
                    paragraph.wraps_dual = true;
                }
                None
            }
            ("ScriptNote", Some(Region::Script)) => Some(CaptureInto::Note),
            (_, Some(Region::Script)) => Some(CaptureInto::Paragraph),
            _ => None,
        }
    }

    fn text(&mut self, text: &str) {
        let Some(run) = self.run else { return };
        let Some(paragraph) = self.paragraphs.last_mut() else { return };
        let text = if run.all_caps { text.to_uppercase() } else { text.to_string() };
        let at = paragraph.chars;
        let len = u32::try_from(text.chars().count()).unwrap_or(u32::MAX);
        paragraph.chars = at.saturating_add(len);
        mark_revised(&mut paragraph.revised, at, len, run.revision);
        match paragraph.runs.last_mut() {
            Some(last) if last.style == run.style && !last.note => last.text.push_str(&text),
            _ => paragraph.runs.push(Run { text, style: run.style, note: false }),
        }
    }

    fn captured(&mut self, into: CaptureInto, xml: &str) -> Result<(), CoreError> {
        match into {
            CaptureInto::Note => {
                self.notes += 1;
                if self.notes > MAX_ELEMENTS {
                    return Err(CoreError::TooManyElements);
                }
                let Some(read) = read_note(xml, FdxNoteForm::Inline) else { return Ok(()) };
                let Some(paragraph) = self.parent_paragraph() else { return Ok(()) };
                if read.plain {
                    paragraph.runs.push(Run { text: read.note.text, style: Style::default(), note: true });
                } else {
                    let mut note = read.note;
                    let at = paragraph.printed_chars();
                    note.range = (at > 0).then_some((at, at));
                    paragraph.notes.push(note);
                }
            }
            CaptureInto::Section => self.document.sections.push(FdxSection::Raw(xml.to_string())),
            CaptureInto::Paragraph => {
                if let Some(paragraph) = self.parent_paragraph() {
                    paragraph.children.push_str(xml);
                }
            }
        }
        Ok(())
    }

    /// Handles an end tag; the element is still on top of the stack.
    fn close(&mut self, name: &str, source: &str, at: usize) -> Result<(), CoreError> {
        let depth = self.stack.len();
        match name {
            "Text" => self.run = None,
            "DualDialogue" if self.parent_paragraph().is_some() => self.in_dual = false,
            "TitlePage" if depth == 2 => {
                if let Some(start) = self.title_start.take() {
                    let raw = source.get(start..at).unwrap_or_default().to_string();
                    let parsed = title_page_from(std::mem::take(&mut self.title));
                    self.document.sections.push(FdxSection::TitlePage { raw, parsed });
                }
            }
            "Paragraph" if self.paragraphs.last().is_some_and(|p| p.depth == depth) => {
                if let Some(paragraph) = self.paragraphs.pop() {
                    self.finish(paragraph)?;
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn finish(&mut self, paragraph: ParagraphState) -> Result<(), CoreError> {
        if paragraph.new_page && paragraph.region == Region::Title {
            self.pending_break = true;
        }
        let start = self.offset;
        if paragraph.region == Region::Script {
            self.offset = start.saturating_add(paragraph.printed_chars()).saturating_add(1);
        }
        // A dual-dialogue wrapper (Final Draft types it `General`) holds the speeches, not text.
        if paragraph.wraps_dual || (!paragraph.has_text && paragraph.kind.is_none()) {
            return Ok(());
        }
        // Runs were collected verbatim; the model stores Fountain markup.
        let markup = inline::write(&paragraph.runs);
        if paragraph.region == Region::Title {
            self.title.push(TitleParagraph { field: paragraph.field, alignment: paragraph.alignment.unwrap_or_else(|| "Left".into()), text: markup });
            return Ok(());
        }
        if self.elements.len() >= MAX_ELEMENTS {
            return Err(CoreError::TooManyElements);
        }
        if std::mem::take(&mut self.pending_break) && !self.elements.is_empty() {
            self.elements.push(Element::new(ElementKind::PageBreak, ""));
            self.starts.push(None);
        }
        let type_name = paragraph.kind.unwrap_or_else(|| "Action".into());
        let (mut kind, level) = kind_from_fdx(&type_name);
        let centered = paragraph.alignment.as_deref().is_some_and(|a| a.eq_ignore_ascii_case("center"));
        if kind == ElementKind::Action && centered && type_name.trim() == "Action" {
            kind = ElementKind::Centered;
        }
        let mut element = Element::new(kind, markup);
        element.notes = paragraph.notes;
        element.revised = paragraph.revised;
        if kind == ElementKind::SceneHeading {
            element.scene_number = paragraph.number;
        }
        if kind == ElementKind::Section {
            element.level = level.unwrap_or(paragraph.level).clamp(1, 6);
        }
        if kind == ElementKind::Character && self.in_dual {
            element.dual = self.dual_cues >= 1;
            self.dual_cues += 1;
        }
        let mut attributes = paragraph.attributes;
        if kind == ElementKind::Centered {
            attributes.retain(|(key, _)| key != "Alignment");
        }
        if type_name != fdx_type(&element) || !attributes.is_empty() || !paragraph.children.is_empty() {
            element.fdx = Some(Box::new(FdxParagraph { type_name, kind, attributes, children: paragraph.children }));
        }
        self.elements.push(element);
        self.starts.push(Some(start));
        Ok(())
    }

    /// Reads the `LockedPages` list onto the paragraphs each page starts in.
    fn place_locked_pages(&mut self) -> Result<(), CoreError> {
        let Some(xml) = self.document.sections.iter().find_map(|s| match s {
            FdxSection::Raw(xml) if section_is(xml, "LockedPages") => Some(xml.clone()),
            _ => None,
        }) else {
            return Ok(());
        };
        let starts: Vec<(usize, u32)> = self.starts.iter().enumerate().filter_map(|(i, s)| s.map(|s| (i, s))).collect();
        let entries: Vec<Vec<(String, String)>> = tags(&xml).into_iter().skip(1).filter(|(name, _)| name == "LockedPage").map(|(_, a)| a).collect();
        if entries.len() > MAX_ELEMENTS {
            return Err(CoreError::TooManyElements);
        }
        // A page without a position starts where the one before it does.
        let mut previous = 0;
        let mut placed: Vec<(u32, Vec<(String, String)>)> = entries
            .into_iter()
            .map(|attributes| {
                previous = number(get(&attributes, "Position")).unwrap_or(previous);
                (previous, attributes)
            })
            .collect();
        placed.sort_by_key(|(position, _)| *position);
        for (position, attributes) in placed {
            let mut kept = attributes.clone();
            for (key, value) in &mut kept {
                if key == "Position" {
                    value.clear();
                }
            }
            let after = starts.partition_point(|(_, s)| *s <= position);
            let Some(&(index, start)) = starts.get(after.saturating_sub(1)) else { continue };
            let page = PageStart {
                at: position.saturating_sub(start),
                label: get(&attributes, "PageNumber").unwrap_or_default().to_string(),
                fdx_attributes: kept,
            };
            self.document.locked_pages.push((position, page.clone()));
            if let Some(element) = self.elements.get_mut(index) {
                element.page_starts.push(page);
            }
        }
        Ok(())
    }

    /// Reads the top-level note lists onto the paragraphs they belong to.
    fn place_listed_notes(&mut self) -> Result<(), CoreError> {
        let mut ranged = Vec::new();
        let mut unanchored = Vec::new();
        for section in &self.document.sections {
            let FdxSection::Raw(xml) = section else { continue };
            let form = if section_is(xml, "ScriptNotes") {
                FdxNoteForm::Ranged
            } else if section_is(xml, "UnanchoredScriptNotes") {
                FdxNoteForm::Unanchored
            } else {
                continue;
            };
            let children = child_elements(xml);
            if children.len() > MAX_ELEMENTS {
                return Err(CoreError::TooManyElements);
            }
            for child in children {
                let Some(read) = read_note(child, form) else { continue };
                match form {
                    FdxNoteForm::Ranged => ranged.push((read.range.unwrap_or_default(), read.note)),
                    _ => unanchored.push(read.note),
                }
            }
        }
        // Paragraph starts only grow, so each note's paragraph is found by binary search.
        let starts: Vec<(usize, u32)> = self.starts.iter().enumerate().filter_map(|(i, s)| s.map(|s| (i, s))).collect();
        for ((from, to), mut note) in ranged {
            let after = starts.partition_point(|(_, s)| *s <= from);
            let Some(&(index, start)) = starts.get(after.saturating_sub(1)) else { continue };
            let from_here = from.saturating_sub(start);
            note.range = Some((from_here, to.max(from).saturating_sub(start)));
            self.document.ranged_notes.push((from, to.max(from), note.clone()));
            if let Some(element) = self.elements.get_mut(index) {
                element.notes.push(note);
            }
        }
        for note in unanchored {
            if let Some(element) = self.elements.first_mut() {
                self.document.unanchored_notes.push(note.clone());
                element.notes.push(note);
            }
        }
        self.document.ranged_notes.sort_by_key(|(from, to, _)| (*from, *to));
        Ok(())
    }

    fn into_script(mut self) -> Result<Script, CoreError> {
        self.place_listed_notes()?;
        let revisions = self.document.sections.iter().find_map(|s| match s {
            FdxSection::Raw(xml) if section_is(xml, "Revisions") => Some(read_revisions(xml)),
            _ => None,
        });
        self.document.revisions = revisions.clone();
        self.place_locked_pages()?;
        apply_hidden_types(&mut self.elements, &self.document.sections);
        let title_page = self
            .document
            .sections
            .iter()
            .find_map(|s| match s {
                FdxSection::TitlePage { parsed, .. } => Some(parsed.clone()),
                FdxSection::Raw(_) => None,
            })
            .unwrap_or_default();
        Ok(Script {
            title_page,
            elements: self.elements,
            revisions: revisions.unwrap_or_default(),
            fdx: Some(Box::new(self.document)),
            ..Script::default()
        })
    }
}

/// Parse a Final Draft XML document.
pub fn parse(source: &str) -> Result<Script, CoreError> {
    if source.len() > MAX_INPUT_BYTES {
        return Err(CoreError::TooLarge);
    }
    let mut reader = quick_xml::Reader::from_str(source);
    reader.config_mut().trim_text(false);
    reader.config_mut().check_end_names = true;

    let mut importer = Importer::default();
    let mut capture: Option<Capture> = None;
    let mut saw_root = false;

    loop {
        let before = usize::try_from(reader.buffer_position()).unwrap_or(usize::MAX);
        let event = reader.read_event().map_err(xml_error)?;
        let after = usize::try_from(reader.buffer_position()).unwrap_or(usize::MAX);
        if let Some(open) = &capture {
            match event {
                Event::Start(start) => importer.push(&String::from_utf8_lossy(start.local_name().as_ref()))?,
                Event::End(_) => {
                    if importer.stack.len() == open.depth {
                        let xml = source.get(open.start..after).unwrap_or_default();
                        let into = open.into;
                        capture = None;
                        importer.captured(into, xml)?;
                    }
                    importer.stack.pop();
                }
                Event::Eof => break,
                _ => {}
            }
            continue;
        }
        match event {
            Event::Start(start) => {
                let name = String::from_utf8_lossy(start.local_name().as_ref()).into_owned();
                if importer.stack.is_empty() {
                    if name != "FinalDraft" {
                        return Err(CoreError::NotFdx(format!("the root element is <{name}>")));
                    }
                    saw_root = true;
                }
                importer.push(&name)?;
                if let Some(into) = importer.open(&name, &start, before) {
                    capture = Some(Capture { depth: importer.stack.len(), start: before, into });
                }
            }
            Event::Empty(start) => {
                let name = String::from_utf8_lossy(start.local_name().as_ref()).into_owned();
                if importer.stack.is_empty() {
                    return Err(CoreError::NotFdx(format!("the root element is <{name}/>")));
                }
                importer.push(&name)?;
                // Final Draft 13 writes empty self-closing paragraphs next to real ones; they hold nothing.
                let region = if name == "Paragraph" { paragraph_region(&importer.stack) } else { None };
                if region == Some(Region::Script) {
                    importer.offset = importer.offset.saturating_add(1);
                }
                if region.is_none() {
                    match importer.open(&name, &start, before) {
                        Some(into) => importer.captured(into, source.get(before..after).unwrap_or_default())?,
                        None => importer.close(&name, source, after)?,
                    }
                }
                importer.stack.pop();
            }
            Event::End(_) => {
                let name = importer.stack.last().cloned().unwrap_or_default();
                importer.close(&name, source, after)?;
                importer.stack.pop();
            }
            Event::Text(text) => {
                let content = text.xml_content(XmlVersion::Implicit1_0).map_err(xml_error)?;
                importer.text(&content);
            }
            Event::CData(data) => {
                let content = data.decode().map_err(xml_error)?;
                importer.text(&content);
            }
            Event::GeneralRef(reference) => importer.text(&reference_text(&reference)?),
            Event::Eof => break,
            _ => {}
        }
    }
    if !saw_root {
        return Err(CoreError::NotFdx("no <FinalDraft> root element".into()));
    }
    importer.into_script()
}

/// Which paragraph types the file's `ElementSettings` mark as hidden (`HiddenText` in the font
/// style). `None` when the file has no element settings.
fn hidden_types(sections: &[FdxSection]) -> Option<Vec<(String, bool)>> {
    let mut found: Option<Vec<(String, bool)>> = None;
    for section in sections {
        let FdxSection::Raw(xml) = section else { continue };
        if !xml.starts_with("<ElementSettings") {
            continue;
        }
        let mut reader = quick_xml::Reader::from_str(xml);
        let mut current: Option<(String, bool)> = None;
        while let Ok(event) = reader.read_event() {
            match event {
                Event::Start(start) | Event::Empty(start) => match start.local_name().as_ref() {
                    b"ElementSettings" => current = Some((attr(&start, "Type").unwrap_or_default(), false)),
                    b"FontSpec" => {
                        if let Some((_, hidden)) = current.as_mut() {
                            *hidden |= attr(&start, "Style").is_some_and(|s| s.split('+').any(|p| p.trim() == "HiddenText"));
                        }
                    }
                    _ => {}
                },
                Event::Eof => break,
                _ => {}
            }
        }
        if let Some(entry) = current {
            found.get_or_insert_with(Vec::new).push(entry);
        }
    }
    found
}

/// Applies the file's own idea of which types print. Outline and summary types are hidden unless
/// the file says otherwise; any other type the file hides is kept as a non-printing synopsis.
fn apply_hidden_types(elements: &mut [Element], sections: &[FdxSection]) {
    let Some(settings) = hidden_types(sections) else { return };
    for element in elements {
        let type_name = fdx_type(element);
        let Some(&(_, hidden)) = settings.iter().find(|(name, _)| *name == type_name) else { continue };
        let new_kind = match (element.kind, hidden) {
            (ElementKind::Action, true) => ElementKind::Synopsis,
            (ElementKind::Section | ElementKind::Synopsis, false) => ElementKind::Action,
            (kind, _) => kind,
        };
        if new_kind == element.kind {
            continue;
        }
        element.kind = new_kind;
        element.level = 0;
        // Keep the file's type name, which no longer matches the kind.
        let fdx = element.fdx.get_or_insert_with(|| {
            Box::new(FdxParagraph { type_name: type_name.clone(), kind: new_kind, attributes: Vec::new(), children: String::new() })
        });
        fdx.type_name = type_name;
        fdx.kind = new_kind;
    }
}

fn title_page_from(paragraphs: Vec<TitleParagraph>) -> TitlePage {
    let mut page = TitlePage::default();
    fn push(page: &mut TitlePage, key: &str, line: &str) {
        match page.fields.last_mut() {
            Some(field) if field.key == key => {
                field.value.push('\n');
                field.value.push_str(line);
            }
            _ => page.fields.push(TitleField { key: key.to_string(), value: line.to_string() }),
        }
    }
    // Paragraphs we wrote carry their field; others are mapped by position and alignment.
    let mut seen_credit = false;
    let mut seen_author = false;
    for paragraph in paragraphs {
        let text = paragraph.text.trim();
        if let Some(field) = &paragraph.field {
            push(&mut page, field, &paragraph.text);
            continue;
        }
        if text.is_empty() {
            continue;
        }
        let lower = text.to_lowercase();
        let key = match paragraph.alignment.to_ascii_lowercase().as_str() {
            "center" => {
                if lower == "by" || lower.starts_with("written by") || lower.starts_with("screenplay by") || lower.starts_with("story by") {
                    seen_credit = true;
                    "Credit"
                } else if !seen_credit {
                    "Title"
                } else if !seen_author || page.fields.last().is_some_and(|f| f.key == "Author") {
                    seen_author = true;
                    "Author"
                } else {
                    "Source"
                }
            }
            "right" => "Draft date",
            _ => "Contact",
        };
        push(&mut page, key, text);
    }
    page
}

// ---------------------------------------------------------------------------------------------
// Script notes

/// A top-level section's raw XML is the element `name`.
fn section_is(xml: &str, name: &str) -> bool {
    xml.strip_prefix('<').and_then(|rest| rest.strip_prefix(name)).is_some_and(|rest| rest.starts_with([' ', '>', '/', '\n', '\t', '\r']))
}

/// The raw XML of each direct child element of a fragment's root.
fn child_elements(xml: &str) -> Vec<&str> {
    let mut reader = quick_xml::Reader::from_str(xml);
    let mut depth = 0usize;
    let mut start = 0usize;
    let mut out = Vec::new();
    loop {
        let before = usize::try_from(reader.buffer_position()).unwrap_or(usize::MAX);
        let Ok(event) = reader.read_event() else { break };
        let after = usize::try_from(reader.buffer_position()).unwrap_or(usize::MAX);
        match event {
            Event::Start(_) => {
                depth += 1;
                if depth == 2 {
                    start = before;
                }
            }
            Event::End(_) => {
                if depth == 2 {
                    out.push(xml.get(start..after).unwrap_or_default());
                }
                depth = depth.saturating_sub(1);
            }
            Event::Empty(_) if depth == 1 => out.push(xml.get(before..after).unwrap_or_default()),
            Event::Eof => break,
            _ => {}
        }
    }
    out
}

/// `#RRGGBB` for a Final Draft colour (`#RRRRGGGGBBBB`, or already `#RRGGBB`).
pub(crate) fn rgb_from_fdx(value: &str) -> Option<String> {
    let hex = value.trim().strip_prefix('#')?;
    if !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let rgb = match hex.len() {
        6 => hex.to_string(),
        12 => [0..2, 4..6, 8..10].into_iter().filter_map(|r| hex.get(r)).collect(),
        _ => return None,
    };
    Some(format!("#{}", rgb.to_ascii_uppercase()))
}

/// Final Draft's 48-bit spelling of `#RRGGBB`.
pub(crate) fn rgb_to_fdx(rgb: &str) -> Option<String> {
    let hex = rgb.trim().strip_prefix('#').filter(|h| h.len() == 6 && h.chars().all(|c| c.is_ascii_hexdigit()))?;
    Some(format!("#{}", [0..2, 2..4, 4..6].into_iter().filter_map(|r| hex.get(r)).map(|b| b.repeat(2)).collect::<String>().to_ascii_uppercase()))
}

/// A note as read from a `ScriptNote` element.
struct ReadNote {
    note: ScriptNote,
    /// It was written for a plain `[[note]]`.
    plain: bool,
    /// Its `Range`, for a listed note.
    range: Option<(u32, u32)>,
}

fn read_note(xml: &str, form: FdxNoteForm) -> Option<ReadNote> {
    let mut reader = quick_xml::Reader::from_str(xml);
    reader.config_mut().trim_text(false);
    let mut attributes: Option<Vec<(String, String)>> = None;
    let mut paragraphs: Vec<String> = Vec::new();
    let mut depth = 0usize;
    let mut in_text = false;
    loop {
        let event = reader.read_event().ok()?;
        let content = match event {
            Event::Start(start) => {
                depth += 1;
                match (depth, start.local_name().as_ref()) {
                    (1, _) => attributes = Some(all_attrs(&start)),
                    (2, b"Paragraph") => paragraphs.push(String::new()),
                    (3, b"Text") => in_text = true,
                    _ => {}
                }
                continue;
            }
            Event::Empty(start) => {
                match (depth, start.local_name().as_ref()) {
                    (0, _) => attributes = Some(all_attrs(&start)),
                    (1, b"Paragraph") => paragraphs.push(String::new()),
                    _ => {}
                }
                continue;
            }
            Event::End(_) => {
                if depth == 3 {
                    in_text = false;
                }
                depth = depth.saturating_sub(1);
                continue;
            }
            Event::Text(text) if in_text => text.xml_content(XmlVersion::Implicit1_0).ok()?.into_owned(),
            Event::CData(data) if in_text => data.decode().ok()?.into_owned(),
            Event::GeneralRef(reference) if in_text => reference_text(&reference).ok()?,
            Event::Eof => break,
            _ => continue,
        };
        if let Some(paragraph) = paragraphs.last_mut() {
            paragraph.push_str(&content);
        }
    }
    let mut attributes = attributes?;
    let plain = get(&attributes, INLINE_NOTE_ATTR).is_some();
    let range = get(&attributes, "Range").and_then(|r| {
        let (from, to) = r.split_once(',')?;
        Some((from.trim().parse().ok()?, to.trim().parse().ok()?))
    });
    for (key, value) in &mut attributes {
        if key == "Range" {
            value.clear();
        }
    }
    let author = get(&attributes, "Author").or_else(|| get(&attributes, "WriterName")).filter(|a| !a.trim().is_empty()).map(str::to_string);
    let mut note = ScriptNote {
        text: paragraphs.join("\n"),
        range: None,
        color: get(&attributes, "Color").and_then(rgb_from_fdx),
        author,
        unanchored: form == FdxNoteForm::Unanchored,
        fdx: None,
    };
    let as_read = Some(Box::new(note.clone()));
    note.fdx = Some(Box::new(FdxNote { form, attributes, raw: xml.to_string(), as_read }));
    Some(ReadNote { note, plain, range })
}

/// The form a note is written in: the one it came in, otherwise by what it covers.
pub(crate) fn note_form(note: &ScriptNote) -> FdxNoteForm {
    if note.unanchored {
        return FdxNoteForm::Unanchored;
    }
    match &note.fdx {
        Some(fdx) if fdx.form != FdxNoteForm::Unanchored => fdx.form,
        _ => default_note_form(note),
    }
}

/// The form a note with no Final Draft data of its own is written in.
pub(crate) fn default_note_form(note: &ScriptNote) -> FdxNoteForm {
    if note.unanchored {
        FdxNoteForm::Unanchored
    } else if note.range.is_some_and(|(from, to)| to > from) {
        FdxNoteForm::Ranged
    } else {
        FdxNoteForm::Inline
    }
}

fn set_or_remove(attributes: &mut Vec<(String, String)>, key: &str, value: Option<String>) {
    match (attributes.iter_mut().find(|(k, _)| k == key), value) {
        (Some((_, slot)), Some(value)) => *slot = value,
        (Some(_), None) => attributes.retain(|(k, _)| k != key),
        (None, Some(value)) => attributes.push((key.to_string(), value)),
        (None, None) => {}
    }
}

/// The attributes a note is written with: the ones it came with, with colour, author and range
/// brought up to date. `range` is the note's place in the whole script, for a listed note.
pub(crate) fn note_attributes(note: &ScriptNote, form: FdxNoteForm, range: Option<(u32, u32)>) -> Vec<(String, String)> {
    let mut attributes = note.fdx.as_ref().map(|f| f.attributes.clone()).unwrap_or_default();
    // A colour or author spelled differently but meaning the same is kept as written.
    let color = get(&attributes, "Color")
        .filter(|c| rgb_from_fdx(c) == note.color)
        .map(str::to_string)
        .or_else(|| note.color.as_deref().and_then(rgb_to_fdx));
    set_or_remove(&mut attributes, "Color", color);
    let author_key = if get(&attributes, "WriterName").is_some() || (get(&attributes, "Author").is_none() && form == FdxNoteForm::Ranged) {
        "WriterName"
    } else {
        "Author"
    };
    let author =
        get(&attributes, author_key).filter(|a| a.trim().is_empty() && note.author.is_none()).map(str::to_string).or_else(|| note.author.clone());
    set_or_remove(&mut attributes, author_key, author);
    match form {
        FdxNoteForm::Ranged => set_or_remove(&mut attributes, "Range", Some(range.map(|(from, to)| format!("{from},{to}")).unwrap_or_default())),
        _ => attributes.retain(|(k, _)| k != "Range"),
    }
    if note.fdx.is_none() {
        attributes.sort();
    }
    attributes
}

/// The note was read from a file and has not changed since.
fn note_unchanged(note: &ScriptNote) -> Option<&str> {
    let fdx = note.fdx.as_deref()?;
    let read = fdx.as_read.as_deref()?;
    (!fdx.raw.is_empty() && read.text == note.text && read.color == note.color && read.author == note.author).then_some(fdx.raw.as_str())
}

fn note_xml(note: &ScriptNote, attributes: &[(String, String)]) -> String {
    let mut out = String::from("<ScriptNote");
    for (key, value) in attributes {
        write_attr(key, value, &mut out);
    }
    out.push('>');
    for line in note.text.split('\n') {
        out.push_str("<Paragraph><Text>");
        escape(line, &mut out);
        out.push_str("</Text></Paragraph>");
    }
    out.push_str("</ScriptNote>");
    out
}

/// A note kept in its paragraph, or in the unanchored list.
fn inline_note_xml(note: &ScriptNote) -> String {
    match note_unchanged(note) {
        Some(raw) => raw.to_string(),
        None => note_xml(note, &note_attributes(note, note_form(note), None)),
    }
}

/// A note of the `ScriptNotes` list, covering `from..to` of the whole script.
fn ranged_note_xml(note: &ScriptNote, from: u32, to: u32) -> String {
    let attributes = note_attributes(note, FdxNoteForm::Ranged, Some((from, to)));
    // An unchanged note keeps its body as read, under a start tag rebuilt with the new range.
    if let Some(raw) = note_unchanged(note) {
        let mut reader = quick_xml::Reader::from_str(raw);
        if let Ok(Event::Start(_)) = reader.read_event() {
            let body = raw.get(usize::try_from(reader.buffer_position()).unwrap_or(usize::MAX)..).unwrap_or_default();
            let mut out = String::from("<ScriptNote");
            for (key, value) in &attributes {
                write_attr(key, value, &mut out);
            }
            out.push('>');
            out.push_str(body);
            return out;
        }
    }
    note_xml(note, &attributes)
}

/// The note written for a plain `[[note]]`.
fn plain_note_xml(text: &str) -> String {
    note_xml(&ScriptNote { text: text.to_string(), ..ScriptNote::default() }, &[(INLINE_NOTE_ATTR.to_string(), "Yes".to_string())])
}

fn write_note_list<I: IntoIterator<Item = String>>(name: &str, notes: I, out: &mut String) {
    out.push_str(&format!("  <{name}>\n"));
    for note in notes {
        out.push_str("    ");
        out.push_str(&note);
        out.push('\n');
    }
    out.push_str(&format!("  </{name}>\n"));
}

/// Printed characters of an element, as Final Draft counts them for `ScriptNotes@Range`.
fn printed_len(element: &Element) -> u32 {
    u32::try_from(inline::plain(&element.text).chars().count()).unwrap_or(u32::MAX)
}

/// Where each element starts in the script's text as [`write`] lays it out: one character per
/// paragraph break, a dual-dialogue block's wrapper counted after its speeches.
pub(crate) fn text_starts(elements: &[Element]) -> Vec<Option<u32>> {
    let mut starts = vec![None; elements.len()];
    let mut offset = 0u32;
    let mut place = |index: usize, offset: &mut u32| {
        if let (Some(slot), Some(element)) = (starts.get_mut(index), elements.get(index)) {
            *slot = Some(*offset);
            *offset = offset.saturating_add(printed_len(element)).saturating_add(1);
        }
    };
    let mut index = 0;
    while let Some(element) = elements.get(index) {
        if element.kind == ElementKind::PageBreak {
            index += 1;
            continue;
        }
        if element.kind == ElementKind::Character && !element.dual {
            let first_end = speech_end(elements, index);
            if elements.get(first_end).is_some_and(|e| e.kind == ElementKind::Character && e.dual) {
                let second_end = speech_end(elements, first_end);
                for part in index..second_end {
                    place(part, &mut offset);
                }
                offset = offset.saturating_add(1);
                index = second_end;
                continue;
            }
        }
        place(index, &mut offset);
        index += 1;
    }
    starts
}

/// The script's listed notes with their place in the whole script, in order.
fn ranged_notes(elements: &[Element]) -> Vec<(u32, u32, &ScriptNote)> {
    let starts = text_starts(elements);
    let mut out = Vec::new();
    for (element, start) in elements.iter().zip(starts) {
        let Some(start) = start else { continue };
        for note in element.notes.iter().filter(|n| note_form(n) == FdxNoteForm::Ranged) {
            let (from, to) = note.range.unwrap_or((0, printed_len(element)));
            out.push((start.saturating_add(from), start.saturating_add(to.max(from)), note));
        }
    }
    out.sort_by_key(|(from, to, _)| (*from, *to));
    out
}

// ---------------------------------------------------------------------------------------------
// Revisions

fn number(value: Option<&str>) -> Option<u32> {
    value.and_then(|v| v.trim().parse().ok())
}

/// The revision sets of a `Revisions` section.
fn read_revisions(xml: &str) -> Revisions {
    let tags = tags(xml);
    let mut revisions = Revisions::default();
    if let Some((_, root)) = tags.first() {
        revisions.active = number(get(root, "ActiveSet"));
        revisions.fdx_attributes = root.clone();
    }
    for (_, attributes) in tags.iter().skip(1).filter(|(name, _)| name == "Revision").take(MAX_ELEMENTS) {
        revisions.sets.push(RevisionSet {
            id: number(get(attributes, "ID")).unwrap_or(0),
            name: get(attributes, "Name").unwrap_or_default().to_string(),
            mark: get(attributes, "Mark").unwrap_or_default().to_string(),
            color: get(attributes, "Color").and_then(rgb_from_fdx),
            page_color: get(attributes, "PageColor").and_then(rgb_from_fdx),
            fdx_attributes: attributes.clone(),
        });
    }
    revisions
}

/// `attributes` with `key` set to `value`, keeping a value already there that means the same.
fn keep_or_set(attributes: &mut Vec<(String, String)>, key: &str, value: Option<String>, same: impl Fn(&str) -> bool) {
    if get(attributes, key).is_some_and(&same) {
        return;
    }
    set_or_remove(attributes, key, value);
}

/// The `Revision` attributes for a set: the ones it came with, brought up to date.
pub(crate) fn revision_set_attributes(set: &RevisionSet) -> Vec<(String, String)> {
    let mut attributes = set.fdx_attributes.clone();
    keep_or_set(&mut attributes, "Color", set.color.as_deref().and_then(rgb_to_fdx), |c| rgb_from_fdx(c) == set.color);
    keep_or_set(&mut attributes, "ID", Some(set.id.to_string()), |v| number(Some(v)) == Some(set.id));
    keep_or_set(&mut attributes, "Mark", Some(set.mark.clone()), |v| v == set.mark);
    keep_or_set(&mut attributes, "Name", Some(set.name.clone()), |v| v == set.name);
    keep_or_set(&mut attributes, "PageColor", set.page_color.as_deref().and_then(rgb_to_fdx), |c| rgb_from_fdx(c) == set.page_color);
    if set.fdx_attributes.is_empty() {
        attributes.sort();
    }
    attributes
}

/// The `Revisions` element's own attributes, brought up to date.
pub(crate) fn revisions_attributes(revisions: &Revisions) -> Vec<(String, String)> {
    let mut attributes = revisions.fdx_attributes.clone();
    keep_or_set(&mut attributes, "ActiveSet", revisions.active.map(|a| a.to_string()), |v| number(Some(v)) == revisions.active);
    attributes
}

fn write_revisions(revisions: &Revisions, out: &mut String) {
    out.push_str("  <Revisions");
    for (key, value) in revisions_attributes(revisions) {
        write_attr(&key, &value, out);
    }
    if revisions.sets.is_empty() {
        out.push_str("/>\n");
        return;
    }
    out.push_str(">\n");
    for set in &revisions.sets {
        out.push_str("    <Revision");
        for (key, value) in revision_set_attributes(set) {
            write_attr(&key, &value, out);
        }
        out.push_str("/>\n");
    }
    out.push_str("  </Revisions>\n");
}

// ---------------------------------------------------------------------------------------------
// Locked pages

/// Every page start with its `Position` in the script, in order.
fn locked_pages(elements: &[Element]) -> Vec<(u32, &PageStart)> {
    let mut out: Vec<(u32, &PageStart)> = elements
        .iter()
        .zip(text_starts(elements))
        .filter_map(|(element, start)| start.map(|start| element.page_starts.iter().map(move |p| (start.saturating_add(p.at), p))))
        .flatten()
        .collect();
    out.sort_by_key(|(position, _)| *position);
    out
}

/// The `LockedPage` attributes for a page start: the ones it came with, brought up to date.
/// `position` is its place in the whole script (`None` leaves a place holder).
pub(crate) fn locked_page_attributes(page: &PageStart, position: Option<u32>) -> Vec<(String, String)> {
    let mut attributes = page.fdx_attributes.clone();
    keep_or_set(&mut attributes, "PageNumber", Some(page.label.clone()), |v| v == page.label);
    // A page that came without a position is written without one.
    if page.fdx_attributes.is_empty() || get(&attributes, "Position").is_some() {
        set_or_remove(&mut attributes, "Position", Some(position.map(|p| p.to_string()).unwrap_or_default()));
    }
    if page.fdx_attributes.is_empty() {
        attributes.sort();
    }
    attributes
}

fn write_locked_pages(root: &[(String, String)], pages: &[(u32, &PageStart)], out: &mut String) {
    out.push_str("  <LockedPages");
    for (key, value) in root {
        write_attr(key, value, out);
    }
    if pages.is_empty() {
        out.push_str("/>\n");
        return;
    }
    out.push_str(">\n");
    for (position, page) in pages {
        out.push_str("    <LockedPage");
        for (key, value) in locked_page_attributes(page, Some(*position)) {
            write_attr(&key, &value, out);
        }
        out.push_str("/>\n");
    }
    out.push_str("  </LockedPages>\n");
}

// ---------------------------------------------------------------------------------------------
// Layout

/// An indent in inches.
fn inches(value: &str) -> Option<f32> {
    let value: f32 = value.trim().parse().ok()?;
    (value.is_finite() && (-10.0..=20.0).contains(&value)).then_some(value)
}

/// Inches to character columns (10 per inch).
fn columns(inches: f32) -> u16 {
    (inches.max(0.0) * 10.0).round() as u16
}

/// Points to lines (12 points each).
fn lines(points: &str) -> Option<u16> {
    let value: f32 = points.trim().parse().ok()?;
    (value.is_finite() && (0.0..=720.0).contains(&value)).then(|| (value / 12.0).round() as u16)
}

/// One element's settings as Final Draft wrote them.
#[derive(Default)]
struct ElementSpec {
    name: String,
    left: Option<f32>,
    first_indent: Option<f32>,
    right: Option<f32>,
    space_before: Option<u16>,
    align: Option<Align>,
}

impl ElementSpec {
    /// Applies the spec over `style`. Final Draft's first indent is relative to the left
    /// indent; a negative one (parentheticals) hangs the first line to the left.
    fn apply(&self, style: &mut ElementStyle) {
        if let (Some(left), Some(right)) = (self.left, self.right) {
            let first = self.first_indent.unwrap_or(0.0).min(0.0);
            style.left = columns(left + first);
            style.width = chars_per_line(right - left);
        }
        if let Some(space) = self.space_before {
            style.space_before = space;
        }
        if let Some(align) = self.align {
            style.align = align;
        }
    }
}

fn align_from(value: &str) -> Option<Align> {
    match value.trim().to_ascii_lowercase().as_str() {
        "left" => Some(Align::Left),
        "right" => Some(Align::Right),
        "center" => Some(Align::Center),
        _ => None,
    }
}

/// An imported paragraph's own formatting (Final Draft writes `LeftIndent`, `SpaceBefore` and so
/// on on paragraphs that override their element settings) applied over `base`.
pub fn paragraph_style(fdx: &FdxParagraph, base: ElementStyle) -> ElementStyle {
    let attributes = &fdx.attributes;
    if !attributes.iter().any(|(key, _)| matches!(key.as_str(), "LeftIndent" | "RightIndent" | "SpaceBefore" | "Alignment")) {
        return base;
    }
    let spec = ElementSpec {
        name: String::new(),
        left: get(attributes, "LeftIndent").and_then(inches),
        right: get(attributes, "RightIndent").and_then(inches),
        first_indent: get(attributes, "FirstIndent").and_then(inches),
        space_before: get(attributes, "SpaceBefore").and_then(lines),
        align: get(attributes, "Alignment").and_then(align_from),
    };
    let mut style = base;
    spec.apply(&mut style);
    style
}

/// Reads one top-level section's start tags: `(tag name, attributes)` in order.
fn tags(xml: &str) -> Vec<(String, Vec<(String, String)>)> {
    let mut reader = quick_xml::Reader::from_str(xml);
    let mut out = Vec::new();
    while let Ok(event) = reader.read_event() {
        match event {
            Event::Start(start) | Event::Empty(start) => {
                out.push((String::from_utf8_lossy(start.local_name().as_ref()).into_owned(), all_attrs(&start)));
            }
            Event::Eof => break,
            _ => {}
        }
    }
    out
}

fn get<'a>(attributes: &'a [(String, String)], key: &str) -> Option<&'a str> {
    attributes.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str())
}

/// Applies an imported file's page layout, element settings and mores-and-continueds to `layout`.
pub fn apply_layout(document: &FdxDocument, layout: &mut Layout) {
    for section in &document.sections {
        let FdxSection::Raw(xml) = section else { continue };
        let tags = tags(xml);
        let Some((root, root_attrs)) = tags.first() else { continue };
        match root.as_str() {
            "ElementSettings" => {
                let mut spec = ElementSpec { name: get(root_attrs, "Type").unwrap_or_default().to_string(), ..ElementSpec::default() };
                if let Some((_, paragraph)) = tags.iter().find(|(name, _)| name == "ParagraphSpec") {
                    spec.left = get(paragraph, "LeftIndent").and_then(inches);
                    spec.right = get(paragraph, "RightIndent").and_then(inches);
                    spec.first_indent = get(paragraph, "FirstIndent").and_then(inches);
                    spec.space_before = get(paragraph, "SpaceBefore").and_then(lines);
                    spec.align = get(paragraph, "Alignment").and_then(align_from);
                }
                let target = match spec.name.as_str() {
                    "Scene Heading" => Some(&mut layout.scene_heading),
                    "Action" => Some(&mut layout.action),
                    "Character" => Some(&mut layout.character),
                    "Parenthetical" => Some(&mut layout.parenthetical),
                    "Dialogue" => Some(&mut layout.dialogue),
                    "Transition" => Some(&mut layout.transition),
                    "Shot" => Some(&mut layout.shot),
                    "Lyrics" => Some(&mut layout.lyrics),
                    _ => None,
                };
                match target {
                    Some(style) => spec.apply(style),
                    None if !spec.name.is_empty() => {
                        let mut style = layout.action;
                        spec.apply(&mut style);
                        layout.custom.retain(|(name, _)| *name != spec.name);
                        layout.custom.push((spec.name.clone(), style));
                    }
                    None => {}
                }
            }
            "PageLayout" => {
                let height =
                    tags.iter().find(|(name, _)| name == "PageSize").and_then(|(_, a)| get(a, "Height")).and_then(|h| h.trim().parse::<f32>().ok());
                let width =
                    tags.iter().find(|(name, _)| name == "PageSize").and_then(|(_, a)| get(a, "Width")).and_then(|w| w.trim().parse::<f32>().ok());
                if let Some(height) = height.filter(|h| (5.0..=20.0).contains(h)) {
                    layout.page_rows = (height * 6.0).round() as u16;
                }
                if let Some(width) = width.filter(|w| (4.0..=15.0).contains(w)) {
                    layout.page_columns = (width * 10.0).round() as u16;
                }
                let top = get(root_attrs, "TopMargin").and_then(lines);
                let bottom = get(root_attrs, "BottomMargin").and_then(lines);
                if let Some(top) = top {
                    layout.body_top = top;
                }
                // "Normal" document leading fills the page between the margins at 6 lines per inch.
                let (top, bottom) = (top.unwrap_or(6), bottom.unwrap_or(6));
                layout.lines_per_page = layout.page_rows.saturating_sub(top).saturating_sub(bottom).max(10);
            }
            "MoresAndContinueds" => {
                if let Some((_, breaks)) = tags.iter().find(|(name, _)| name == "DialogueBreaks") {
                    if let Some(more) = get(breaks, "DialogueBottom").filter(|v| !v.trim().is_empty()) {
                        layout.more = more.to_string();
                    }
                    if let Some(contd) = get(breaks, "DialogueTop").filter(|v| !v.trim().is_empty()) {
                        layout.contd = contd.to_string();
                    }
                    if let Some(auto) = get(breaks, "AutomaticCharacterContinueds") {
                        layout.auto_contd = auto.trim().eq_ignore_ascii_case("yes") || auto.trim() == "1" || auto.trim().eq_ignore_ascii_case("true");
                    }
                }
            }
            _ => {}
        }
    }
}
