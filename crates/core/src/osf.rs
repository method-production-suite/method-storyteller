//! Open Screenplay Format (`.osf`, and the `document.xml` inside a `.fadein`) import and export.
//!
//! Clean-room: OSF has no written specification, so this follows the format's published example
//! documents (versions 1.2, 2.0, 3.0 and 4.0) and permissively licensed sample files. Names are
//! matched loosely, because versions spell them differently (`basestylename`, `basestyle`,
//! `baseStyleName`; `scene_number`, `number`, `sceneNumber`).
//!
//! Everything the model does not use is kept and written back: other top-level sections
//! (settings, styles, lists …) verbatim in [`OsfDocument`], and each paragraph's style name,
//! other attributes, style overrides and other children in [`OsfParagraph`]. A run's `revision`
//! becomes the paragraph's revised spans, and `<lists><revision_colors>` with `settings@revision`
//! the script's revision sets; a set's mark colour and page colour, which OSF has no place for,
//! ride on `method_storyteller_color` and `method_storyteller_page_color`. Other text-run
//! attributes (strikethrough, colour) are not kept yet.
//!
//! A paragraph's `note` holds plain text only. Script notes with a range, colour or author (from
//! an `.fdx`) are written into `note` as well, and also as `<method_storyteller_note …/>` children
//! carrying the rest, so our own round trip is exact while other apps read the plain note.
//!
//! Locked pages are `settings@pages_locked` with each paragraph's `page_number`; a page that
//! starts inside a paragraph, or carries Final Draft data, also has a
//! `<method_storyteller_page at="…" label="…"/>` child.
//!
//! Sections and synopses have no OSF equivalent of their own. A synopsis rides on the
//! paragraph before it (`synopsis`); otherwise both are written as paragraphs carrying a
//! `<method_storyteller kind="…"/>` child, which other apps ignore.

use std::collections::HashMap;

use quick_xml::XmlVersion;
use quick_xml::events::Event;

use crate::fdx::{all_attrs, default_note_form, escape, locked_page_attributes, note_attributes, revision_set_attributes, revisions_attributes};
use crate::inline::{self, Run, Style};
use crate::model::{
    Element, ElementKind, FdxNote, FdxNoteForm, OsfDocument, OsfParagraph, OsfSection, PageStart, Revised, RevisionSet, Revisions, Script,
    ScriptNote, TitleField, TitlePage, mark_revised,
};
use crate::{CoreError, MAX_ELEMENTS, MAX_INPUT_BYTES};

/// Deepest element nesting accepted.
const MAX_DEPTH: usize = 128;
/// Our marker for paragraphs OSF has no kind for.
const MARKER: &str = "method_storyteller";
/// Our element for a script note's colour, author, range and Final Draft data.
const NOTE_MARKER: &str = "method_storyteller_note";
/// Our element for a locked page that starts inside a paragraph, or carries Final Draft data.
const PAGE_MARKER: &str = "method_storyteller_page";
/// Our element for a scene heading's title, summary and colour, which OSF has no place for.
const SCENE_MARKER: &str = "method_storyteller_scene";

/// The element name a raw section starts with, compared loosely.
fn top_name(raw: &str) -> String {
    key(raw.trim_start().trim_start_matches('<').split(|c: char| c.is_whitespace() || c == '>' || c == '/').next().unwrap_or_default())
}

/// An attribute name compared loosely: lower case, without underscores.
fn key(name: &str) -> String {
    name.chars().filter(|c| *c != '_').flat_map(char::to_lowercase).collect()
}

fn get<'a>(attributes: &'a [(String, String)], name: &str) -> Option<&'a str> {
    let wanted = key(name);
    attributes.iter().find(|(k, _)| key(k) == wanted).map(|(_, v)| v.as_str())
}

fn truthy(value: Option<&str>) -> bool {
    matches!(value.map(str::trim), Some("1" | "true" | "yes" | "True" | "TRUE"))
}

fn xml_error(error: impl std::fmt::Display) -> CoreError {
    CoreError::Xml(error.to_string().chars().take(200).collect())
}

/// The spellings a document uses for the style reference and the scene number.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Spelling {
    /// `basestylename`, `scene_number` (versions up to 3.0).
    Snake,
    /// `basestyle`, `number` (version 4.0).
    V4,
    /// `baseStyleName`, `sceneNumber` (the unofficial "2.1").
    Camel,
}

impl Spelling {
    fn of(version: &str) -> Self {
        match version.trim().parse::<u32>().unwrap_or(30) {
            21 => Spelling::Camel,
            v if v >= 40 => Spelling::V4,
            _ => Spelling::Snake,
        }
    }

    fn base_style(self) -> &'static str {
        match self {
            Spelling::Snake => "basestylename",
            Spelling::V4 => "basestyle",
            Spelling::Camel => "baseStyleName",
        }
    }

    fn scene_number(self) -> &'static str {
        match self {
            Spelling::Snake => "scene_number",
            Spelling::V4 => "number",
            Spelling::Camel => "sceneNumber",
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Reading

/// A top-level child of `<document>`: its name and raw XML.
struct TopLevel {
    name: String,
    raw: String,
}

/// Attribute name and value pairs, in file order.
type Attributes = Vec<(String, String)>;

/// Splits a document into its root attributes and top-level children.
fn split(source: &str) -> Result<(Attributes, Vec<TopLevel>), CoreError> {
    let mut reader = quick_xml::Reader::from_str(source);
    reader.config_mut().trim_text(false);
    reader.config_mut().check_end_names = true;
    let mut depth = 0usize;
    let mut root = None;
    let mut children = Vec::new();
    let mut open: Option<(String, usize)> = None;
    loop {
        let before = usize::try_from(reader.buffer_position()).unwrap_or(usize::MAX);
        let event = reader.read_event().map_err(xml_error)?;
        let after = usize::try_from(reader.buffer_position()).unwrap_or(usize::MAX);
        match event {
            Event::Start(start) => {
                depth += 1;
                if depth > MAX_DEPTH {
                    return Err(CoreError::Xml("elements are nested too deeply".into()));
                }
                let name = start.local_name().as_ref().to_string();
                if depth == 1 {
                    if !name.eq_ignore_ascii_case("document") {
                        return Err(CoreError::NotOsf(format!("the root element is <{name}>")));
                    }
                    root = Some(all_attrs(&start));
                } else if depth == 2 {
                    open = Some((name, before));
                }
            }
            Event::Empty(start) => {
                let name = start.local_name().as_ref().to_string();
                if depth == 0 {
                    return Err(CoreError::NotOsf(format!("the root element is <{name}/>")));
                }
                if depth == 1 {
                    children.push(TopLevel { name, raw: source.get(before..after).unwrap_or_default().to_string() });
                }
            }
            Event::End(_) => {
                if depth == 2
                    && let Some((name, start)) = open.take()
                {
                    children.push(TopLevel { name, raw: source.get(start..after).unwrap_or_default().to_string() });
                }
                depth = depth.saturating_sub(1);
            }
            Event::Eof => break,
            _ => {}
        }
    }
    let root = root.ok_or_else(|| CoreError::NotOsf("no <document> root element".into()))?;
    Ok((root, children))
}

/// Every start tag in a fragment with its attributes, in order (for settings and styles).
fn tags(xml: &str) -> Vec<(String, Vec<(String, String)>)> {
    let mut reader = quick_xml::Reader::from_str(xml);
    let mut out = Vec::new();
    while let Ok(event) = reader.read_event() {
        match event {
            Event::Start(start) | Event::Empty(start) => out.push((start.local_name().as_ref().to_string(), all_attrs(&start))),
            Event::Eof => break,
            _ => {}
        }
    }
    out
}

/// Style name → the built-in style it comes from (following `basestylename` chains).
struct Styles {
    base: HashMap<String, String>,
}

/// The built-in style names and the kinds they import as.
fn builtin_kind(name: &str) -> Option<ElementKind> {
    Some(match name.trim().to_ascii_lowercase().as_str() {
        "scene heading" => ElementKind::SceneHeading,
        "action" | "normal text" | "general" => ElementKind::Action,
        "character" => ElementKind::Character,
        "parenthetical" => ElementKind::Parenthetical,
        "dialogue" => ElementKind::Dialogue,
        "transition" => ElementKind::Transition,
        "shot" => ElementKind::Shot,
        "new act" => ElementKind::NewAct,
        "end of act" => ElementKind::EndOfAct,
        _ => return None,
    })
}

impl Styles {
    fn read(sections: &[TopLevel]) -> Self {
        let mut base = HashMap::new();
        for section in sections.iter().filter(|s| s.name.eq_ignore_ascii_case("styles")) {
            for (tag, attributes) in tags(&section.raw) {
                if !tag.eq_ignore_ascii_case("style") {
                    continue;
                }
                let Some(name) = get(&attributes, "name") else { continue };
                if let Some(parent) = get(&attributes, "basestylename").or_else(|| get(&attributes, "basestyle")) {
                    base.insert(name.to_string(), parent.to_string());
                }
            }
        }
        Styles { base }
    }

    /// The kind a style imports as: its own built-in name, or the nearest built-in ancestor's.
    fn kind(&self, name: &str) -> ElementKind {
        if name.eq_ignore_ascii_case("lyrics") {
            return ElementKind::Lyrics;
        }
        let mut current = name.to_string();
        for _ in 0..16 {
            if let Some(kind) = builtin_kind(&current) {
                return kind;
            }
            match self.base.get(&current) {
                Some(parent) if *parent != current => current = parent.clone(),
                _ => break,
            }
        }
        ElementKind::Action
    }
}

/// One `<para>` as read: attributes, style attributes, runs and other children.
#[derive(Default)]
struct Para {
    attributes: Vec<(String, String)>,
    style: Vec<(String, String)>,
    runs: Vec<Run>,
    revised: Vec<Revised>,
    /// Characters of text read so far.
    chars: u32,
    children: String,
}

/// Reads the `<para>` elements of a `<paragraphs>` or `<titlepage>` section.
fn paras(xml: &str) -> Result<Vec<Para>, CoreError> {
    let mut reader = quick_xml::Reader::from_str(xml);
    reader.config_mut().trim_text(false);
    let mut out: Vec<Para> = Vec::new();
    let mut depth = 0usize;
    let mut current: Option<Para> = None;
    let mut run: Option<(Style, u32)> = None;
    let mut capture: Option<(usize, usize)> = None;
    loop {
        let before = usize::try_from(reader.buffer_position()).unwrap_or(usize::MAX);
        let event = reader.read_event().map_err(xml_error)?;
        let after = usize::try_from(reader.buffer_position()).unwrap_or(usize::MAX);
        if let Some((capture_depth, start)) = capture {
            match event {
                Event::Start(_) => depth += 1,
                Event::End(_) => {
                    if depth == capture_depth {
                        if let Some(para) = current.as_mut() {
                            para.children.push_str(xml.get(start..after).unwrap_or_default());
                        }
                        capture = None;
                    }
                    depth = depth.saturating_sub(1);
                }
                Event::Eof => break,
                _ => {}
            }
            continue;
        }
        match event {
            Event::Start(ref start) | Event::Empty(ref start) => {
                let empty = matches!(event, Event::Empty(_));
                if !empty {
                    depth += 1;
                    if depth > MAX_DEPTH {
                        return Err(CoreError::Xml("elements are nested too deeply".into()));
                    }
                }
                let name = start.local_name().as_ref().to_ascii_lowercase();
                match (name.as_str(), current.is_some()) {
                    ("para", false) => {
                        current = Some(Para { attributes: all_attrs(start), ..Para::default() });
                        if empty && let Some(para) = current.take() {
                            if out.len() >= MAX_ELEMENTS {
                                return Err(CoreError::TooManyElements);
                            }
                            out.push(para);
                        }
                    }
                    ("style", true) => {
                        if let Some(para) = current.as_mut() {
                            para.style = all_attrs(start);
                        }
                    }
                    ("text", true) => {
                        let attributes = all_attrs(start);
                        let style = Style {
                            bold: truthy(get(&attributes, "bold")),
                            italic: truthy(get(&attributes, "italic")),
                            underline: truthy(get(&attributes, "underline")),
                        };
                        let revision = get(&attributes, "revision").and_then(|r| r.trim().parse().ok()).unwrap_or(0);
                        if !empty {
                            run = Some((style, revision));
                        }
                    }
                    (_, true) => {
                        if empty {
                            if let Some(para) = current.as_mut() {
                                para.children.push_str(xml.get(before..after).unwrap_or_default());
                            }
                        } else {
                            capture = Some((depth, before));
                        }
                    }
                    _ => {}
                }
            }
            Event::End(end) => {
                let name = end.local_name().as_ref().to_ascii_lowercase();
                if name == "text" {
                    run = None;
                } else if name == "para"
                    && let Some(para) = current.take()
                {
                    if out.len() >= MAX_ELEMENTS {
                        return Err(CoreError::TooManyElements);
                    }
                    out.push(para);
                }
                depth = depth.saturating_sub(1);
            }
            Event::Text(text) => {
                if let (Some(style), Some(para)) = (run, current.as_mut()) {
                    let content = text.xml_content(XmlVersion::Implicit1_0);
                    push_run(para, &content, style);
                }
            }
            Event::GeneralRef(reference) => {
                if let (Some(style), Some(para)) = (run, current.as_mut()) {
                    let resolved = match reference.resolve_char_ref().map_err(xml_error)? {
                        Some(c) => c.to_string(),
                        None => quick_xml::escape::resolve_predefined_entity(reference.as_ref()).unwrap_or("").to_string(),
                    };
                    push_run(para, &resolved, style);
                }
            }
            Event::CData(data) => {
                if let (Some(style), Some(para)) = (run, current.as_mut()) {
                    push_run(para, data.as_ref(), style);
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    Ok(out)
}

fn push_run(para: &mut Para, text: &str, (style, revision): (Style, u32)) {
    let text = text.replace("\r\n", "\n");
    let len = u32::try_from(text.chars().count()).unwrap_or(u32::MAX);
    let at = para.chars;
    para.chars = at.saturating_add(len);
    mark_revised(&mut para.revised, at, len, revision);
    match para.runs.last_mut() {
        Some(last) if last.style == style => last.text.push_str(&text),
        _ => para.runs.push(Run { text, style, note: false }),
    }
}

/// `<para>` attributes the model holds itself.
fn modelled_para_attribute(name: &str) -> bool {
    matches!(key(name).as_str(), "scenenumber" | "number" | "note" | "synopsis")
}

/// `<style>` attributes the model holds itself.
fn modelled_style_attribute(name: &str) -> bool {
    matches!(key(name).as_str(), "basestylename" | "basestyle" | "dualdialogue" | "pagebreakbefore")
}

/// Parse an Open Screenplay Format document (the XML, not a `.fadein` archive).
pub fn parse(source: &str) -> Result<Script, CoreError> {
    if source.len() > MAX_INPUT_BYTES {
        return Err(CoreError::TooLarge);
    }
    let (root, top) = split(source)?;
    let version = get(&root, "version").unwrap_or("30").to_string();
    let styles = Styles::read(&top);
    let revisions = read_revisions(&top);
    let pages_locked = top
        .iter()
        .filter(|s| s.name.eq_ignore_ascii_case("settings"))
        .any(|s| tags(&s.raw).first().is_some_and(|(_, a)| truthy(get(a, "pages_locked"))));
    let section = |name: &str| top.iter().find(|s| s.name.eq_ignore_ascii_case(name)).map(|s| s.raw.as_str());
    let format = crate::format::from_osf(section("settings"), section("styles"))?;
    let spelling_ignored = section("spelling").map(|raw| crate::fdx::ignored_words(raw, "ignored")).unwrap_or_default();
    let mut document = OsfDocument {
        version,
        sections: Vec::new(),
        archive_entries: Vec::new(),
        revisions: Some(revisions.clone()),
        pages_locked,
        format: Some(format.clone()),
        spelling_ignored: spelling_ignored.clone(),
    };
    // The page the paragraphs read so far end on, while pages are locked.
    let mut page: Option<Option<String>> = pages_locked.then_some(None);
    let mut elements: Vec<Element> = Vec::new();
    let mut title_page = TitlePage::default();
    let mut dual_next = false;

    for section in &top {
        if section.name.eq_ignore_ascii_case("paragraphs") {
            document.sections.push(OsfSection::Paragraphs);
            for para in paras(&section.raw)? {
                read_para(para, &styles, &mut elements, &mut dual_next, &mut page)?;
            }
        } else if section.name.eq_ignore_ascii_case("titlepage") {
            let parsed = read_title_page(&paras(&section.raw)?);
            title_page = parsed.clone();
            document.sections.push(OsfSection::TitlePage { raw: section.raw.clone(), parsed });
        } else {
            document.sections.push(OsfSection::Raw(section.raw.clone()));
        }
    }
    Ok(Script { title_page, elements, revisions, format, osf: Some(Box::new(document)), spelling_ignored, ..Script::default() })
}

fn read_para(
    para: Para,
    styles: &Styles,
    elements: &mut Vec<Element>,
    dual_next: &mut bool,
    page: &mut Option<Option<String>>,
) -> Result<(), CoreError> {
    if elements.len() >= MAX_ELEMENTS {
        return Err(CoreError::TooManyElements);
    }
    let style_name = get(&para.style, "basestylename").or_else(|| get(&para.style, "basestyle")).unwrap_or("Action").to_string();
    let marker = tags(&para.children).into_iter().find(|(tag, _)| tag == MARKER).map(|(_, a)| a);
    let mut kind = match marker.as_ref().and_then(|m| get(m, "kind")) {
        Some("section") => ElementKind::Section,
        Some("synopsis") => ElementKind::Synopsis,
        Some("lyrics") => ElementKind::Lyrics,
        _ => styles.kind(&style_name),
    };
    let centered = get(&para.style, "align").is_some_and(|a| a.eq_ignore_ascii_case("center"));
    if kind == ElementKind::Action && centered && builtin_kind(&style_name).is_some() {
        kind = ElementKind::Centered;
    }
    if truthy(get(&para.style, "pagebreakbefore")) && !elements.is_empty() {
        elements.push(Element::new(ElementKind::PageBreak, ""));
    }
    let mut markup = inline::write(&para.runs);
    let ours: Vec<Vec<(String, String)>> = tags(&para.children).into_iter().filter(|(tag, _)| tag == NOTE_MARKER).map(|(_, a)| a).collect();
    let mut notes = Vec::new();
    if ours.is_empty() {
        // A plain note rides at the end of the text as a Fountain note, with nothing added
        // around it, so writing it back gives the same text.
        if let Some(note) = get(&para.attributes, "note").filter(|n| !n.trim().is_empty()) {
            markup.push_str(&format!("[[{}]]", note.replace("]]", "] ]")));
        }
    } else {
        // Our own notes say everything the `note` attribute holds, and more.
        for attributes in ours.iter().take(MAX_ELEMENTS) {
            let text = get(attributes, "text").unwrap_or_default();
            if truthy(get(attributes, "plain")) {
                markup.push_str(&format!("[[{}]]", text.replace("]]", "] ]")));
            } else {
                notes.push(note_from(attributes));
            }
        }
    }
    let mut element = Element::new(kind, markup);
    element.notes = notes;
    element.revised = para.revised.clone();
    if kind == ElementKind::SceneHeading {
        element.scene_number =
            get(&para.attributes, "scenenumber").or_else(|| get(&para.attributes, "number")).filter(|n| !n.trim().is_empty()).map(str::to_string);
    }
    if kind == ElementKind::Section {
        element.level = marker.as_ref().and_then(|m| get(m, "level")).and_then(|l| l.parse().ok()).unwrap_or(1).clamp(1, 6);
    }
    if kind == ElementKind::Character {
        element.dual = std::mem::take(dual_next);
        if truthy(get(&para.style, "dualdialogue")) {
            *dual_next = true;
        }
    }
    // Children other than our own go back as they came.
    let children = strip_ours(&para.children);
    if kind == ElementKind::SceneHeading
        && let Some(scene) = tags(&para.children).into_iter().find(|(tag, _)| tag == SCENE_MARKER).map(|(_, a)| a)
    {
        let value = |name: &str| get(&scene, name).unwrap_or_default().to_string();
        element.scene = crate::model::SceneMeta {
            title: value("title"),
            summary: value("summary"),
            color: get(&scene, "color").filter(|c| !c.is_empty()).map(str::to_string),
        };
    }
    // Locked pages: a page starts with the paragraph whose page number changes, unless our own
    // page elements say exactly which pages start in it.
    if let Some(current) = page.as_mut() {
        let label = get(&para.attributes, "page_number").map(str::to_string);
        let ours: Vec<PageStart> = tags(&para.children).into_iter().filter(|(tag, _)| tag == PAGE_MARKER).map(|(_, a)| page_from(&a)).collect();
        if ours.len() > MAX_ELEMENTS {
            return Err(CoreError::TooManyElements);
        }
        if label.is_some() && label != *current && ours.is_empty() {
            element.page_starts.push(PageStart { at: 0, label: label.clone().unwrap_or_default(), fdx_attributes: Vec::new() });
        }
        element.page_starts.extend(ours);
        *current = element.page_starts.last().map(|p| p.label.clone()).or(label).or(current.take());
    }
    let attributes: Vec<(String, String)> =
        para.attributes.iter().filter(|(k, _)| !(modelled_para_attribute(k) || (page.is_some() && key(k) == "pagenumber"))).cloned().collect();
    let mut style_attributes: Vec<(String, String)> = para.style.iter().filter(|(k, _)| !modelled_style_attribute(k)).cloned().collect();
    if kind == ElementKind::Centered {
        style_attributes.retain(|(k, _)| key(k) != "align");
    }
    // A `.fdx` paragraph type of its own comes back with the paragraph.
    if let Some(type_name) = marker.as_ref().and_then(|m| get(m, "fdx_type")) {
        element.fdx =
            Some(Box::new(crate::model::FdxParagraph { type_name: type_name.to_string(), kind, attributes: Vec::new(), children: String::new() }));
    }
    let canonical = style_for(kind);
    if style_name != canonical || !attributes.is_empty() || !style_attributes.is_empty() || !children.is_empty() {
        element.osf = Some(Box::new(OsfParagraph { style: style_name, kind, attributes, style_attributes, children }));
    }
    elements.push(element);
    if let Some(synopsis) = get(&para.attributes, "synopsis").filter(|s| !s.trim().is_empty()) {
        elements.push(Element::new(ElementKind::Synopsis, synopsis.to_string()));
    }
    Ok(())
}

/// Children without our own `method_storyteller` elements.
fn strip_ours(children: &str) -> String {
    let mut reader = quick_xml::Reader::from_str(children);
    let mut out = String::new();
    let mut from = 0usize;
    let mut depth = 0usize;
    loop {
        let before = usize::try_from(reader.buffer_position()).unwrap_or(usize::MAX);
        let Ok(event) = reader.read_event() else { return children.to_string() };
        let after = usize::try_from(reader.buffer_position()).unwrap_or(usize::MAX);
        match event {
            Event::Start(_) => depth += 1,
            Event::End(_) => depth = depth.saturating_sub(1),
            Event::Empty(start)
                if depth == 0
                    && matches!(
                        start.local_name().as_ref(),
                        "method_storyteller" | "method_storyteller_note" | "method_storyteller_page" | "method_storyteller_scene"
                    ) =>
            {
                out.push_str(children.get(from..before).unwrap_or_default());
                from = after;
            }
            Event::Eof => break,
            _ => {}
        }
    }
    out.push_str(children.get(from..).unwrap_or_default());
    out
}

fn form_name(form: FdxNoteForm) -> &'static str {
    match form {
        FdxNoteForm::Inline => "inline",
        FdxNoteForm::Ranged => "ranged",
        FdxNoteForm::Unanchored => "unanchored",
    }
}

/// A script note from one of our `method_storyteller_note` elements.
fn note_from(attributes: &[(String, String)]) -> ScriptNote {
    let range = get(attributes, "range").and_then(|r| {
        let (from, to) = r.split_once(',')?;
        Some((from.trim().parse().ok()?, to.trim().parse().ok()?))
    });
    let fdx = get(attributes, "fdx_form").map(|form| {
        let form = match form {
            "ranged" => FdxNoteForm::Ranged,
            "unanchored" => FdxNoteForm::Unanchored,
            _ => FdxNoteForm::Inline,
        };
        let kept =
            attributes.iter().filter_map(|(k, v)| Some((k.strip_prefix("fdx_")?.to_string(), v.clone()))).filter(|(k, _)| k != "form").collect();
        Box::new(FdxNote { form, attributes: kept, raw: String::new(), as_read: None })
    });
    ScriptNote {
        text: get(attributes, "text").unwrap_or_default().to_string(),
        range,
        color: get(attributes, "color").map(str::to_string),
        author: get(attributes, "author").map(str::to_string),
        unanchored: truthy(get(attributes, "unanchored")),
        fdx,
    }
}

/// One of our `method_storyteller_note` elements for a script note.
fn note_element(note: &ScriptNote) -> String {
    let mut out = format!("<{NOTE_MARKER}");
    write_attr("text", &note.text, &mut out);
    if let Some((from, to)) = note.range {
        write_attr("range", &format!("{from},{to}"), &mut out);
    }
    if let Some(color) = &note.color {
        write_attr("color", color, &mut out);
    }
    if let Some(author) = &note.author {
        write_attr("author", author, &mut out);
    }
    if note.unanchored {
        out.push_str(" unanchored=\"1\"");
    }
    // Final Draft data, unless it is only what writing the note to an `.fdx` would make anyway.
    if let Some(fdx) = &note.fdx {
        let bare = ScriptNote { fdx: None, ..note.clone() };
        if fdx.form != default_note_form(note) || fdx.attributes != note_attributes(&bare, fdx.form, None) {
            write_attr("fdx_form", form_name(fdx.form), &mut out);
            for (key, value) in &fdx.attributes {
                write_attr(&format!("fdx_{key}"), value, &mut out);
            }
        }
    }
    out.push_str("/>");
    out
}

/// Title-page fields from `<titlepage>` paragraphs: bookmarked ones by their bookmark, the rest by
/// position (a "by" line after the title is the credit; other lines join the field above).
fn read_title_page(paras: &[Para]) -> TitlePage {
    let mut page = TitlePage::default();
    let mut current: Option<String> = None;
    for para in paras {
        let text = inline::write(&para.runs);
        let trimmed = text.trim();
        let bookmark = get(&para.attributes, "bookmark").map(|b| match b.trim().to_ascii_lowercase().as_str() {
            "title" => "Title",
            "author" | "authors" | "written by" => "Author",
            "draft" | "draft date" => "Draft date",
            "contact" => "Contact",
            "copyright" => "Copyright",
            "credit" => "Credit",
            "source" => "Source",
            _ => "Notes",
        });
        if trimmed.is_empty() {
            current = None;
            continue;
        }
        let lower = trimmed.to_lowercase();
        let key = match bookmark {
            Some(key) => key.to_string(),
            None if lower == "by" || lower.starts_with("written by") || lower.starts_with("screenplay by") || lower.starts_with("story by") => {
                "Credit".to_string()
            }
            None => current.clone().unwrap_or_else(|| if page.get("Title").is_none() { "Title".into() } else { "Notes".into() }),
        };
        match page.fields.last_mut() {
            Some(field) if field.key == key && current.as_deref() == Some(key.as_str()) => {
                field.value.push('\n');
                field.value.push_str(trimmed);
            }
            _ => page.fields.push(TitleField { key: key.clone(), value: trimmed.to_string() }),
        }
        current = Some(key);
    }
    page
}

// ---------------------------------------------------------------------------------------------
// Revisions

/// OSF's revision colours in order (`color_index`), with the page colours we print them in.
/// The names are the format's; the colours are our own choice, as the format has none.
const PALETTE: [(&str, &str); 10] = [
    ("White", "#FFFFFF"),
    ("Blue", "#C6E2FF"),
    ("Pink", "#FFCCE0"),
    ("Yellow", "#FFF7B2"),
    ("Green", "#CCF2CC"),
    ("Goldenrod", "#F2D57E"),
    ("Buff", "#F0DCB4"),
    ("Salmon", "#FFB4A0"),
    ("Cherry", "#E06070"),
    ("Tan", "#D2B48C"),
];

fn rgb(value: &str) -> Option<(i32, i32, i32)> {
    let hex = value.trim().strip_prefix('#').filter(|h| h.len() == 6)?;
    let part = |r: std::ops::Range<usize>| hex.get(r).and_then(|p| i32::from_str_radix(p, 16).ok());
    Some((part(0..2)?, part(2..4)?, part(4..6)?))
}

/// The palette colour a set is written as: one its name says, else the nearest to its page
/// colour, else one by its number.
fn palette_index(set: &RevisionSet) -> usize {
    let words: Vec<String> = set.name.split(|c: char| !c.is_alphabetic()).map(str::to_ascii_lowercase).collect();
    if let Some(index) = PALETTE.iter().position(|(name, _)| words.iter().any(|w| *w == name.to_ascii_lowercase())) {
        return index;
    }
    if let Some((r, g, b)) = set.page_color.as_deref().and_then(rgb) {
        let distance = |(_, hex): &(&str, &str)| rgb(hex).map_or(i32::MAX, |(pr, pg, pb)| (pr - r).pow(2) + (pg - g).pow(2) + (pb - b).pow(2));
        if let Some((index, _)) = PALETTE.iter().enumerate().min_by_key(|(_, entry)| distance(entry)) {
            return index;
        }
    }
    (set.id as usize) % PALETTE.len()
}

/// Our attributes and Final Draft's, as `prefix_key` pairs.
fn prefixed<'a>(prefix: &'a str, attributes: &'a [(String, String)]) -> impl Iterator<Item = (String, String)> + 'a {
    attributes.iter().map(move |(k, v)| (format!("{prefix}{k}"), v.clone()))
}

fn unprefixed(prefix: &str, attributes: &[(String, String)]) -> Vec<(String, String)> {
    attributes.iter().filter_map(|(k, v)| Some((k.strip_prefix(prefix)?.to_string(), v.clone()))).collect()
}

/// Revision sets from `<lists><revision_colors>` and the active one from `settings@revision`.
fn read_revisions(top: &[TopLevel]) -> Revisions {
    let mut revisions = Revisions::default();
    for section in top {
        if section.name.eq_ignore_ascii_case("settings") {
            revisions.active = tags(&section.raw).first().and_then(|(_, a)| get(a, "revision")).and_then(|r| r.trim().parse().ok());
        }
        if !section.name.eq_ignore_ascii_case("lists") {
            continue;
        }
        for (tag, attributes) in tags(&section.raw) {
            match key(&tag).as_str() {
                "revisioncolors" => revisions.fdx_attributes = unprefixed("fdx_", &attributes),
                "revisioncolor" if revisions.sets.len() < MAX_ELEMENTS => {
                    let palette = get(&attributes, "color_name").and_then(|n| PALETTE.iter().find(|(p, _)| p.eq_ignore_ascii_case(n.trim())));
                    revisions.sets.push(RevisionSet {
                        id: get(&attributes, "index").and_then(|i| i.trim().parse().ok()).unwrap_or(0),
                        name: get(&attributes, "name").unwrap_or_default().to_string(),
                        mark: get(&attributes, "mark").unwrap_or_default().to_string(),
                        color: get(&attributes, "method_storyteller_color").map(str::to_string),
                        page_color: match get(&attributes, "method_storyteller_page_color") {
                            Some(page) => Some(page.to_string()).filter(|p| !p.is_empty()),
                            None => palette.map(|(_, hex)| hex.to_string()),
                        },
                        fdx_attributes: unprefixed("fdx_", &attributes),
                    });
                }
                _ => {}
            }
        }
    }
    revisions
}

/// The revision list, spelled as the document spells it (camelCase in the unofficial "2.1").
fn revision_colors(revisions: &Revisions, spelling: Spelling) -> String {
    let camel = spelling == Spelling::Camel;
    let (list, entry, name_key, index_key) = if camel {
        ("revisionColors", "revisionColor", "colorName", "colorIndex")
    } else {
        ("revision_colors", "revision_color", "color_name", "color_index")
    };
    let mut out = format!("<{list}");
    let bare = Revisions { fdx_attributes: Vec::new(), ..revisions.clone() };
    if !revisions.fdx_attributes.is_empty() && revisions.fdx_attributes != revisions_attributes(&bare) {
        for (k, v) in prefixed("fdx_", &revisions.fdx_attributes) {
            write_attr(&k, &v, &mut out);
        }
    }
    if revisions.sets.is_empty() {
        out.push_str("/>");
        return out;
    }
    out.push('>');
    for set in &revisions.sets {
        let index = palette_index(set);
        let (color_name, palette_color) = PALETTE.get(index).copied().unwrap_or(("White", "#FFFFFF"));
        out.push_str(&format!("<{entry}"));
        write_attr("name", &set.name, &mut out);
        write_attr("index", &set.id.to_string(), &mut out);
        write_attr(name_key, color_name, &mut out);
        write_attr(index_key, &index.to_string(), &mut out);
        write_attr("mark", &set.mark, &mut out);
        // No page colour at all is said too, or it would read back as the palette's.
        match set.page_color.as_deref() {
            Some(page) if page.eq_ignore_ascii_case(palette_color) => {}
            page => write_attr("method_storyteller_page_color", page.unwrap_or_default(), &mut out),
        }
        if let Some(color) = &set.color {
            write_attr("method_storyteller_color", color, &mut out);
        }
        let bare = RevisionSet { fdx_attributes: Vec::new(), ..set.clone() };
        if !set.fdx_attributes.is_empty() && set.fdx_attributes != revision_set_attributes(&bare) {
            for (k, v) in prefixed("fdx_", &set.fdx_attributes) {
                write_attr(&k, &v, &mut out);
            }
        }
        out.push_str("/>");
    }
    out.push_str(&format!("</{list}>"));
    out
}

/// A `<lists>` section with its revision colours, in whichever spelling, replaced by `revisions`.
fn with_revision_colors(lists: &str, revisions: &Revisions, spelling: Spelling) -> String {
    let fresh = revision_colors(revisions, spelling);
    let mut reader = quick_xml::Reader::from_str(lists);
    let mut depth = 0usize;
    let mut open: Option<usize> = None;
    let mut root_end: Option<(usize, bool)> = None;
    loop {
        let before = usize::try_from(reader.buffer_position()).unwrap_or(usize::MAX);
        let Ok(event) = reader.read_event() else { return lists.to_string() };
        let after = usize::try_from(reader.buffer_position()).unwrap_or(usize::MAX);
        let is_ours = |name: &str| key(name) == "revisioncolors";
        match event {
            Event::Start(start) => {
                depth += 1;
                if depth == 1 {
                    root_end = Some((after, false));
                } else if depth == 2 && is_ours(start.local_name().as_ref()) {
                    open = Some(before);
                }
            }
            Event::End(_) => {
                if depth == 2
                    && let Some(start) = open.take()
                {
                    return format!("{}{fresh}{}", lists.get(..start).unwrap_or_default(), lists.get(after..).unwrap_or_default());
                }
                depth = depth.saturating_sub(1);
            }
            Event::Empty(start) => {
                if depth == 0 {
                    root_end = Some((after, true));
                } else if depth == 1 && is_ours(start.local_name().as_ref()) {
                    return format!("{}{fresh}{}", lists.get(..before).unwrap_or_default(), lists.get(after..).unwrap_or_default());
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    // No list yet: it goes first in the section.
    match root_end {
        Some((end, false)) => format!("{}{fresh}{}", lists.get(..end).unwrap_or_default(), lists.get(end..).unwrap_or_default()),
        Some((_, true)) => rebuild_start_tag(lists, |_| {}, Some(&fresh)),
        None => lists.to_string(),
    }
}

/// A section with its root start tag rebuilt from its parsed attributes after `edit`, and the
/// rest as it was. `content` opens a self-closing root around it.
fn rebuild_start_tag(raw: &str, edit: impl FnOnce(&mut Vec<(String, String)>), content: Option<&str>) -> String {
    let mut reader = quick_xml::Reader::from_str(raw);
    loop {
        let before = usize::try_from(reader.buffer_position()).unwrap_or(usize::MAX);
        let event = match reader.read_event() {
            Ok(Event::Eof) | Err(_) => return raw.to_string(),
            Ok(event) => event,
        };
        let after = usize::try_from(reader.buffer_position()).unwrap_or(usize::MAX);
        let (start, empty) = match event {
            Event::Start(start) => (start, false),
            Event::Empty(start) => (start, true),
            _ => continue,
        };
        let name = start.name().as_ref().to_string();
        let mut attributes = all_attrs(&start);
        edit(&mut attributes);
        let mut out = format!("{}<{name}", raw.get(..before).unwrap_or_default());
        for (k, v) in &attributes {
            write_attr(k, v, &mut out);
        }
        match (empty, content) {
            (true, Some(content)) => out.push_str(&format!(">{content}</{name}>")),
            (true, None) => out.push_str("/>"),
            (false, _) => out.push('>'),
        }
        out.push_str(raw.get(after..).unwrap_or_default());
        return out;
    }
}

/// A `<settings>` section with `name` set to `value` (or removed for `None`), unless what it
/// holds already `means` the same.
fn with_setting(settings: &str, name: &str, value: Option<String>, means: impl Fn(Option<&str>) -> bool) -> String {
    let current = tags(settings).first().and_then(|(_, a)| get(a, name).map(str::to_string));
    if means(current.as_deref()) {
        return settings.to_string();
    }
    rebuild_start_tag(
        settings,
        |attributes| {
            let wanted = key(name);
            match (attributes.iter().position(|(k, _)| key(k) == wanted), value) {
                (Some(at), Some(value)) => {
                    if let Some((_, slot)) = attributes.get_mut(at) {
                        *slot = value;
                    }
                }
                (Some(at), None) => {
                    attributes.remove(at);
                }
                (None, Some(value)) => attributes.push((name.to_string(), value)),
                (None, None) => {}
            }
        },
        None,
    )
}

/// A `<settings>` section with `revision` set to the active set.
fn with_active_revision(settings: &str, active: Option<u32>) -> String {
    with_setting(settings, "revision", active.map(|a| a.to_string()), |v| v.and_then(|r| r.trim().parse::<u32>().ok()) == active)
}

/// A `<settings>` tag with `pages_locked` saying whether pages are locked.
fn with_pages_locked(settings: &str, locked: bool) -> String {
    with_setting(settings, "pages_locked", Some(if locked { "1" } else { "0" }.to_string()), |v| truthy(v) == locked)
}

/// A locked page from one of our `method_storyteller_page` elements.
fn page_from(attributes: &[(String, String)]) -> PageStart {
    PageStart {
        at: get(attributes, "at").and_then(|a| a.trim().parse().ok()).unwrap_or(0),
        label: get(attributes, "label").unwrap_or_default().to_string(),
        fdx_attributes: unprefixed("fdx_", attributes),
    }
}

/// Our element for a page start that `page_number` alone cannot say: one inside the paragraph,
/// or one with Final Draft data writing `.fdx` would not make anyway.
fn has_fdx_extras(page: &PageStart) -> bool {
    let bare = PageStart { fdx_attributes: Vec::new(), ..page.clone() };
    !page.fdx_attributes.is_empty() && page.fdx_attributes != locked_page_attributes(&bare, None)
}

/// Whether `page_number` alone says which pages start in a paragraph: one page, starting with
/// it, numbered differently from the page before, and nothing more to carry.
fn page_number_says_it(starts: &[PageStart], before: Option<&str>) -> bool {
    match starts {
        [] => true,
        [only] => only.at == 0 && Some(only.label.as_str()) != before && !has_fdx_extras(only),
        _ => false,
    }
}

fn page_element(page: &PageStart) -> String {
    let extras = has_fdx_extras(page);
    let mut out = format!("<{PAGE_MARKER}");
    write_attr("at", &page.at.to_string(), &mut out);
    write_attr("label", &page.label, &mut out);
    if extras {
        for (k, v) in prefixed("fdx_", &page.fdx_attributes) {
            write_attr(&k, &v, &mut out);
        }
    }
    out.push_str("/>");
    out
}

// ---------------------------------------------------------------------------------------------
// Spelling

/// The words spell check ignores, as `<ignored>` entries shaped like the user dictionary's.
fn ignored_xml(words: &[String]) -> String {
    if words.is_empty() {
        return "<ignored/>".to_string();
    }
    let mut out = String::from("<ignored>");
    for word in words {
        out.push_str("<entry");
        write_attr("word", word, &mut out);
        out.push_str("/>");
    }
    out.push_str("</ignored>");
    out
}

// ---------------------------------------------------------------------------------------------
// Styles

/// A `<styles>` section with a style for each of the format's elements: one the section had is
/// replaced where it was, others follow; header and footer styles stay as they were.
fn rebuild_styles(raw: &str, format: &crate::format::Format, spelling: Spelling) -> String {
    let osf_spelling = crate::format::OsfSpelling { base_style: spelling.base_style() };
    let page = format.page.as_ref();
    let mut written = vec![false; format.elements.len()];
    let mut out = String::from("  <styles>\n");
    for child in crate::fdx::child_elements(raw) {
        let (tag, attributes) = tags(child).into_iter().next().unwrap_or_default();
        // A style without a name is not in the format; it stays as it was.
        if tag.eq_ignore_ascii_case("style") && get(&attributes, "name").is_some() {
            let name = get(&attributes, "name").unwrap_or_default();
            if let Some(index) = format.elements.iter().position(|e| e.name == name)
                && let (Some(element), Some(done)) = (format.elements.get(index), written.get_mut(index))
                && !*done
            {
                out.push_str(&format!("    {}\n", crate::format::osf_style(element, page, &osf_spelling)));
                *done = true;
            }
            continue;
        }
        out.push_str(&format!("    {}\n", child.trim()));
    }
    for (element, done) in format.elements.iter().zip(written) {
        if !done {
            out.push_str(&format!("    {}\n", crate::format::osf_style(element, page, &osf_spelling)));
        }
    }
    out.push_str("  </styles>");
    out
}

// ---------------------------------------------------------------------------------------------
// Writing

/// The OSF style a kind is written as when the paragraph brought no style of its own.
fn style_for(kind: ElementKind) -> &'static str {
    match kind {
        ElementKind::SceneHeading => "Scene Heading",
        ElementKind::Character => "Character",
        ElementKind::Parenthetical => "Parenthetical",
        ElementKind::Dialogue => "Dialogue",
        ElementKind::Transition => "Transition",
        ElementKind::Shot => "Shot",
        ElementKind::Lyrics => "Lyrics",
        ElementKind::NewAct => "New Act",
        ElementKind::EndOfAct => "End of Act",
        ElementKind::Action | ElementKind::Centered | ElementKind::PageBreak | ElementKind::Section | ElementKind::Synopsis => "Action",
    }
}

/// Built-in styles for a new document, matching the default layout (docs/spec.md §5). Indents
/// are tenths of a millimetre from the margins (1.5" left, 1.0" right).
const DEFAULT_STYLES: &str = r#"  <styles>
    <style name="Normal Text" builtin="1" builtin_index="0" font="Courier Prime" size="12"/>
    <style name="Scene Heading" builtin="1" builtin_index="1" basestylename="Normal Text" style_enter="Action" style_tab_after="Action" font="Courier Prime" size="12" spacebefore="2.0" keepwithnext="1" allcaps="1"/>
    <style name="Action" builtin="1" builtin_index="2" basestylename="Normal Text" style_tab_before="Scene Heading" style_tab_after="Character" font="Courier Prime" size="12" spacebefore="1.0"/>
    <style name="Character" builtin="1" builtin_index="3" basestylename="Normal Text" style_enter="Dialogue" style_tab_before="Action" style_tab_after="Transition" font="Courier Prime" size="12" spacebefore="1.0" keepwithnext="1" leftindent="508" rightindent="64" allcaps="1"/>
    <style name="Parenthetical" builtin="1" builtin_index="4" basestylename="Normal Text" style_enter="Dialogue" style_tab_before="Dialogue" style_tab_after="Dialogue" font="Courier Prime" size="12" spacebefore="0.0" keepwithnext="1" leftindent="381" firstlineindent="-25" rightindent="508"/>
    <style name="Dialogue" builtin="1" builtin_index="5" basestylename="Normal Text" style_enter="Action" style_tab_before="Parenthetical" style_tab_after="Parenthetical" font="Courier Prime" size="12" spacebefore="0.0" leftindent="254" rightindent="381"/>
    <style name="Transition" builtin="1" builtin_index="6" basestylename="Normal Text" style_enter="Scene Heading" style_tab_after="Action" font="Courier Prime" size="12" spacebefore="1.0" align="right" leftindent="1016" rightindent="102" allcaps="1"/>
    <style name="Shot" builtin="1" builtin_index="7" basestylename="Normal Text" style_enter="Action" style_tab_after="Action" font="Courier Prime" size="12" spacebefore="2.0" keepwithnext="1" allcaps="1"/>
    <style name="Lyrics" basestylename="Dialogue" italic="1"/>
    <style name="New Act" basestylename="Normal Text" style_enter="Scene Heading" style_tab_after="Scene Heading" font="Courier Prime" size="12" spacebefore="1.0" align="center" allcaps="1"/>
    <style name="End of Act" basestylename="Normal Text" style_enter="New Act" style_tab_after="New Act" font="Courier Prime" size="12" spacebefore="2.0" align="center" allcaps="1"/>
  </styles>"#;

const DEFAULT_SETTINGS: &str = r##"  <settings page_width="2159" page_height="2794" margin_top="254" margin_bottom="254" margin_left="381" margin_right="254" normal_linesperinch="6.0" element_spacing="1.00" break_on_sentences="true" dialogue_continues="true" dialogue_pagebreaks="true" cont_text="(CONT'D)" more_text="(MORE)" scenes_continue="false" scene_time_separator=" - " page_header="#." header_alignment="3" header_first_page="false"/>"##;

fn write_attr(name: &str, value: &str, out: &mut String) {
    out.push(' ');
    out.push_str(name);
    out.push_str("=\"");
    // Line breaks and tabs are written as character references: in an attribute, XML turns the
    // literal characters into spaces, so a note would lose its lines.
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

/// Writes text runs, split where `element`'s revised text starts and ends.
fn write_runs(runs: &[Run], element: Option<&Element>, out: &mut String) {
    let printed: Vec<&Run> = runs.iter().filter(|r| !r.note).collect();
    if printed.is_empty() {
        out.push_str("      <text></text>\n");
        return;
    }
    let mut at = 0u32;
    let mut revisions = element.map(Element::revision_cursor);
    for run in printed {
        let chars: Vec<char> = run.text.chars().collect();
        let len = u32::try_from(chars.len()).unwrap_or(u32::MAX);
        let mut from = 0u32;
        loop {
            let here = at.saturating_add(from);
            let to = revisions.as_mut().and_then(|r| r.next_edge(here)).map(|edge| edge.saturating_sub(at)).filter(|end| *end < len).unwrap_or(len);
            out.push_str("      <text");
            if run.style.bold {
                out.push_str(" bold=\"1\"");
            }
            if run.style.italic {
                out.push_str(" italic=\"1\"");
            }
            if run.style.underline {
                out.push_str(" underline=\"1\"");
            }
            if let Some(revision) = revisions.as_mut().and_then(|r| r.set_at(here)) {
                out.push_str(&format!(" revision=\"{revision}\""));
            }
            out.push('>');
            let piece: String = chars.get(from as usize..to as usize).unwrap_or_default().iter().collect();
            escape(&piece, out);
            out.push_str("</text>\n");
            from = to;
            if from >= len {
                break;
            }
        }
        at = at.saturating_add(len);
    }
}

/// What a paragraph is written with besides its element.
#[derive(Default)]
struct ParaExtras {
    page_break: bool,
    dual_first: bool,
    synopsis: Option<String>,
    /// The locked page the paragraph starts on.
    page_number: Option<String>,
    /// Write every page that starts in the paragraph as one of our elements.
    page_elements: bool,
}

fn write_para(element: &Element, extras: &ParaExtras, spelling: Spelling, styles_known: bool, out: &mut String) {
    let runs = inline::parse(&element.text);
    let plain: Vec<&str> = runs.iter().filter(|r| r.note).map(|r| r.text.trim()).filter(|t| !t.is_empty()).collect();
    let mut notes = plain.clone();
    notes.extend(element.notes.iter().filter(|n| !n.unanchored).map(|n| n.text.as_str()));
    let kept = element.osf.as_deref().filter(|o| o.kind == element.kind);
    out.push_str("    <para");
    if let Some(number) = element.scene_number.as_deref().filter(|_| element.kind == ElementKind::SceneHeading) {
        write_attr(spelling.scene_number(), number, out);
    }
    if let Some(page) = &extras.page_number {
        write_attr("page_number", page, out);
    }
    if !notes.is_empty() {
        write_attr("note", &notes.join(" / "), out);
    }
    if let Some(synopsis) = &extras.synopsis {
        write_attr("synopsis", synopsis, out);
    }
    if let Some(osf) = element.osf.as_deref() {
        for (name, value) in &osf.attributes {
            write_attr(name, value, out);
        }
    }
    out.push_str(">\n      <style");
    // Paragraphs OSF has no kind for are written as their nearest style, plus our marker.
    let mut marker = match element.kind {
        ElementKind::Section => Some(format!("<{MARKER} kind=\"section\" level=\"{}\"", element.level.clamp(1, 6))),
        ElementKind::Synopsis => Some(format!("<{MARKER} kind=\"synopsis\"")),
        ElementKind::Lyrics if !styles_known => Some(format!("<{MARKER} kind=\"lyrics\"")),
        _ => None,
    };
    // A `.fdx` paragraph type of its own (`New Act`), so it goes back to `.fdx` by that name.
    let own_type = crate::fdx::fdx_type(element);
    let plain = Element { fdx: None, ..element.clone() };
    if own_type != crate::fdx::fdx_type(&plain) {
        let mut tag = marker.take().unwrap_or_else(|| format!("<{MARKER}"));
        write_attr("fdx_type", &own_type, &mut tag);
        marker = Some(tag);
    }
    let marker = marker.map(|m| format!("{m}/>"));
    let style = match kept {
        Some(osf) => osf.style.clone(),
        None if element.kind == ElementKind::Lyrics && !styles_known => "Dialogue".to_string(),
        None => style_for(element.kind).to_string(),
    };
    write_attr(spelling.base_style(), &style, out);
    if extras.dual_first {
        out.push_str(" dualdialogue=\"1\"");
    }
    if extras.page_break {
        out.push_str(" pagebreakbefore=\"1\"");
    }
    if element.kind == ElementKind::Centered {
        out.push_str(" align=\"center\"");
    }
    // Style overrides belong to the paragraph's old type; a changed type starts clean.
    if let Some(osf) = kept {
        for (name, value) in &osf.style_attributes {
            write_attr(name, value, out);
        }
    }
    out.push_str("/>\n");
    if let Some(marker) = marker {
        out.push_str("      ");
        out.push_str(&marker);
        out.push('\n');
    }
    for page in element.page_starts.iter().filter(|_| extras.page_elements) {
        out.push_str("      ");
        out.push_str(&page_element(page));
        out.push('\n');
    }
    if element.kind == ElementKind::SceneHeading && !element.scene.is_empty() {
        out.push_str(&format!("      <{SCENE_MARKER}"));
        write_attr("title", &element.scene.title, out);
        write_attr("summary", &element.scene.summary, out);
        if let Some(color) = &element.scene.color {
            write_attr("color", color, out);
        }
        out.push_str("/>\n");
    }
    if !element.notes.is_empty() {
        for text in runs.iter().filter(|r| r.note).map(|r| r.text.as_str()) {
            out.push_str(&format!("      <{NOTE_MARKER}"));
            write_attr("text", text, out);
            out.push_str(" plain=\"1\"/>\n");
        }
        for note in &element.notes {
            out.push_str("      ");
            out.push_str(&note_element(note));
            out.push('\n');
        }
    }
    if let Some(osf) = element.osf.as_deref().filter(|o| !o.children.trim().is_empty()) {
        out.push_str("      ");
        out.push_str(osf.children.trim());
        out.push('\n');
    }
    write_runs(&runs, Some(element), out);
    out.push_str("    </para>\n");
}

fn write_paragraphs(script: &Script, spelling: Spelling, styles_known: bool, out: &mut String) {
    out.push_str("  <paragraphs>\n");
    let locked = script.pages_locked();
    let mut page: Option<String> = None;
    let elements = &script.elements;
    let mut page_break = false;
    let mut index = 0;
    while let Some(element) = elements.get(index) {
        match element.kind {
            ElementKind::PageBreak => {
                page_break = true;
                index += 1;
                continue;
            }
            ElementKind::Synopsis => {
                // A synopsis right after a printed paragraph was written on that paragraph.
                let after_printed = index > 0 && elements.get(index - 1).is_some_and(|e| e.kind.is_printed() && e.kind != ElementKind::Synopsis);
                if after_printed {
                    index += 1;
                    continue;
                }
            }
            _ => {}
        }
        let next = elements.get(index + 1);
        let synopsis = next.filter(|n| n.kind == ElementKind::Synopsis && element.kind.is_printed()).map(|n| n.text.clone());
        let dual_first = element.kind == ElementKind::Character && !element.dual && {
            let mut end = index + 1;
            while elements.get(end).is_some_and(|e| e.kind.is_dialogue_part() || e.kind == ElementKind::Lyrics) {
                end += 1;
            }
            elements.get(end).is_some_and(|e| e.kind == ElementKind::Character && e.dual)
        };
        let page_elements = locked && !page_number_says_it(&element.page_starts, page.as_deref());
        if let Some(last) = element.page_starts.iter().rfind(|p| p.at == 0) {
            page = Some(last.label.clone());
        }
        let page_number = if locked { page.clone() } else { None };
        write_para(
            element,
            &ParaExtras { page_break: std::mem::take(&mut page_break), dual_first, synopsis, page_number, page_elements },
            spelling,
            styles_known,
            out,
        );
        if let Some(last) = element.page_starts.last() {
            page = Some(last.label.clone());
        }
        index += 1;
    }
    out.push_str("  </paragraphs>\n");
}

fn write_title_page(title: &TitlePage, spelling: Spelling, out: &mut String) {
    if title.is_empty() {
        return;
    }
    out.push_str("  <titlepage>\n");
    for field in &title.fields {
        // Every field is bookmarked, so it reads back as the same field.
        let (bookmark, align) = match field.key.to_ascii_lowercase().as_str() {
            "title" => (Some("Title"), "center"),
            "author" | "authors" => (Some("Author"), "center"),
            "credit" => (Some("Credit"), "center"),
            "source" => (Some("Source"), "center"),
            "draft date" | "date" => (Some("Draft"), "right"),
            "contact" => (Some("Contact"), "left"),
            "copyright" => (Some("Copyright"), "left"),
            _ => (Some("Notes"), "left"),
        };
        for (n, line) in field.value.split('\n').enumerate() {
            out.push_str("    <para");
            if n == 0
                && let Some(bookmark) = bookmark
            {
                write_attr("bookmark", bookmark, out);
            }
            out.push_str(&format!(">\n      <style {}=\"Normal Text\" align=\"{align}\"/>\n", spelling.base_style()));
            write_runs(&inline::parse(line), None, out);
            out.push_str("    </para>\n");
        }
        out.push_str(&format!("    <para>\n      <style {}=\"Normal Text\"/>\n      <text></text>\n    </para>\n", spelling.base_style()));
    }
    out.push_str("  </titlepage>\n");
}

/// Write a script as an Open Screenplay Format document. A new document is written as version
/// 3.0, the version most other apps read; an imported one keeps its version and spellings.
pub fn write(script: &Script) -> String {
    let mut out = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    match script.osf.as_deref() {
        Some(document) => {
            let spelling = Spelling::of(&document.version);
            let styles_known = document.sections.iter().any(|s| matches!(s, OsfSection::Raw(raw) if raw.contains("name=\"Lyrics\"")));
            out.push_str("<document type=\"Open Screenplay Format document\"");
            write_attr("version", &document.version, &mut out);
            out.push_str(">\n");
            let mut title_written = false;
            let mut paragraphs_written = false;
            let revisions_same = document.revisions.as_ref() == Some(&script.revisions);
            let format_same = document.format.as_ref() == Some(&script.format);
            let (mut settings_written, mut styles_written, mut spelling_written) = (false, false, false);
            let mut lists_written = false;
            for section in &document.sections {
                match section {
                    OsfSection::Paragraphs => {
                        write_paragraphs(script, spelling, styles_known, &mut out);
                        paragraphs_written = true;
                    }
                    OsfSection::TitlePage { raw, parsed } if !title_written => {
                        if *parsed == script.title_page {
                            out.push_str("  ");
                            out.push_str(raw.trim());
                            out.push('\n');
                        } else {
                            write_title_page(&script.title_page, spelling, &mut out);
                        }
                        title_written = true;
                    }
                    OsfSection::TitlePage { .. } => {}
                    OsfSection::Raw(raw) => {
                        let raw = raw.trim();
                        let raw = match top_name(raw).as_str() {
                            "lists" if !revisions_same => {
                                lists_written = true;
                                with_revision_colors(raw, &script.revisions, spelling)
                            }
                            "settings" => {
                                let raw = match script.format.page.as_ref().filter(|_| !format_same) {
                                    Some(page) => crate::format::osf_settings(page, raw),
                                    None => raw.to_string(),
                                };
                                settings_written = true;
                                let raw = if revisions_same { raw } else { with_active_revision(&raw, script.revisions.active) };
                                if script.pages_locked() == document.pages_locked { raw } else { with_pages_locked(&raw, script.pages_locked()) }
                            }
                            "styles" if !format_same => {
                                styles_written = true;
                                rebuild_styles(raw, &script.format, spelling)
                            }
                            "spelling" => {
                                spelling_written = true;
                                if document.spelling_ignored == script.spelling_ignored {
                                    raw.to_string()
                                } else {
                                    crate::fdx::replace_child(raw, "ignored", &ignored_xml(&script.spelling_ignored))
                                }
                            }
                            _ => raw.to_string(),
                        };
                        out.push_str("  ");
                        out.push_str(&raw);
                        out.push('\n');
                    }
                }
            }
            if !paragraphs_written {
                write_paragraphs(script, spelling, styles_known, &mut out);
            }
            if !title_written {
                write_title_page(&script.title_page, spelling, &mut out);
            }
            if !revisions_same && !lists_written && !script.revisions.sets.is_empty() {
                out.push_str(&format!("  <lists>{}</lists>\n", revision_colors(&script.revisions, spelling)));
            }
            if !format_same
                && !settings_written
                && let Some(page) = &script.format.page
            {
                out.push_str(&format!("  {}\n", crate::format::osf_settings(page, "<settings/>")));
            }
            if !format_same && !styles_written && !script.format.elements.is_empty() {
                out.push_str(&rebuild_styles("<styles/>", &script.format, spelling));
                out.push('\n');
            }
            if !spelling_written && !script.spelling_ignored.is_empty() {
                out.push_str(&format!("  <spelling>{}</spelling>\n", ignored_xml(&script.spelling_ignored)));
            }
        }
        None => {
            out.push_str("<document type=\"Open Screenplay Format document\" version=\"30\">\n");
            let settings = match &script.format.page {
                Some(page) => format!("  {}", crate::format::osf_settings(page, DEFAULT_SETTINGS)),
                None => DEFAULT_SETTINGS.to_string(),
            };
            let settings = with_active_revision(&settings, script.revisions.active);
            out.push_str(&if script.pages_locked() { with_pages_locked(&settings, true) } else { settings });
            out.push('\n');
            if script.format.elements.is_empty() {
                out.push_str(DEFAULT_STYLES);
            } else {
                out.push_str(&rebuild_styles("<styles/>", &script.format, Spelling::Snake));
            }
            out.push('\n');
            write_paragraphs(script, Spelling::Snake, true, &mut out);
            write_title_page(&script.title_page, Spelling::Snake, &mut out);
            if !script.revisions.sets.is_empty() || !script.revisions.fdx_attributes.is_empty() {
                out.push_str(&format!("  <lists>{}</lists>\n", revision_colors(&script.revisions, Spelling::Snake)));
            }
            if !script.spelling_ignored.is_empty() {
                out.push_str(&format!("  <spelling>{}</spelling>\n", ignored_xml(&script.spelling_ignored)));
            }
        }
    }
    out.push_str("</document>\n");
    out
}
