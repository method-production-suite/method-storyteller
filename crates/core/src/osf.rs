//! Open Screenplay Format (`.osf`, and the `document.xml` inside a `.fadein`) import and export.
//!
//! Clean-room: OSF has no written specification, so this follows the format's published example
//! documents (versions 1.2, 2.0, 3.0 and 4.0) and permissively licensed sample files. Names are
//! matched loosely, because versions spell them differently (`basestylename`, `basestyle`,
//! `baseStyleName`; `scene_number`, `number`, `sceneNumber`).
//!
//! Everything the model does not use is kept and written back: other top-level sections
//! (settings, styles, lists …) verbatim in [`OsfDocument`], and each paragraph's style name,
//! other attributes, style overrides and other children in [`OsfParagraph`]. Text-run attributes
//! other than bold, italic and underline (strikethrough, colour, revision) are not kept yet.
//!
//! Sections and synopses have no OSF equivalent of their own. A synopsis rides on the
//! paragraph before it (`synopsis`); otherwise both are written as paragraphs carrying a
//! `<method_storyteller kind="…"/>` child, which other apps ignore.

use std::collections::HashMap;

use quick_xml::XmlVersion;
use quick_xml::events::Event;

use crate::fdx::{all_attrs, escape};
use crate::inline::{self, Run, Style};
use crate::layout::{Align, ElementStyle, Layout, chars_per_line};
use crate::model::{Element, ElementKind, OsfDocument, OsfParagraph, OsfSection, Script, TitleField, TitlePage};
use crate::{CoreError, MAX_ELEMENTS, MAX_INPUT_BYTES};

/// Deepest element nesting accepted.
const MAX_DEPTH: usize = 128;
/// Our marker for paragraphs OSF has no kind for.
const MARKER: &str = "method_storyteller";

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
                let name = String::from_utf8_lossy(start.local_name().as_ref()).into_owned();
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
                let name = String::from_utf8_lossy(start.local_name().as_ref()).into_owned();
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
            Event::Start(start) | Event::Empty(start) => {
                out.push((String::from_utf8_lossy(start.local_name().as_ref()).into_owned(), all_attrs(&start)))
            }
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
    children: String,
}

/// Reads the `<para>` elements of a `<paragraphs>` or `<titlepage>` section.
fn paras(xml: &str) -> Result<Vec<Para>, CoreError> {
    let mut reader = quick_xml::Reader::from_str(xml);
    reader.config_mut().trim_text(false);
    let mut out: Vec<Para> = Vec::new();
    let mut depth = 0usize;
    let mut current: Option<Para> = None;
    let mut run: Option<Style> = None;
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
                let name = String::from_utf8_lossy(start.local_name().as_ref()).to_ascii_lowercase();
                match (name.as_str(), current.is_some()) {
                    ("para", false) => {
                        current = Some(Para { attributes: all_attrs(start), ..Para::default() });
                        if empty && let Some(para) = current.take() {
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
                        if !empty {
                            run = Some(style);
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
                let name = String::from_utf8_lossy(end.local_name().as_ref()).to_ascii_lowercase();
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
                    let content = text.xml_content(XmlVersion::Implicit1_0).map_err(xml_error)?;
                    push_run(para, &content, style);
                }
            }
            Event::GeneralRef(reference) => {
                if let (Some(style), Some(para)) = (run, current.as_mut()) {
                    let resolved = match reference.resolve_char_ref().map_err(xml_error)? {
                        Some(c) => c.to_string(),
                        None => {
                            let name = reference.decode().map_err(xml_error)?;
                            quick_xml::escape::resolve_predefined_entity(&name).unwrap_or("").to_string()
                        }
                    };
                    push_run(para, &resolved, style);
                }
            }
            Event::CData(data) => {
                if let (Some(style), Some(para)) = (run, current.as_mut()) {
                    let content = data.decode().map_err(xml_error)?;
                    push_run(para, &content, style);
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    Ok(out)
}

fn push_run(para: &mut Para, text: &str, style: Style) {
    let text = text.replace("\r\n", "\n");
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
    let mut document = OsfDocument { version, sections: Vec::new() };
    let mut elements: Vec<Element> = Vec::new();
    let mut title_page = TitlePage::default();
    let mut dual_next = false;

    for section in &top {
        if section.name.eq_ignore_ascii_case("paragraphs") {
            document.sections.push(OsfSection::Paragraphs);
            for para in paras(&section.raw)? {
                read_para(para, &styles, &mut elements, &mut dual_next)?;
            }
        } else if section.name.eq_ignore_ascii_case("titlepage") {
            let parsed = read_title_page(&paras(&section.raw)?);
            title_page = parsed.clone();
            document.sections.push(OsfSection::TitlePage { raw: section.raw.clone(), parsed });
        } else {
            document.sections.push(OsfSection::Raw(section.raw.clone()));
        }
    }
    Ok(Script { title_page, elements, fdx: None, osf: Some(Box::new(document)) })
}

fn read_para(para: Para, styles: &Styles, elements: &mut Vec<Element>, dual_next: &mut bool) -> Result<(), CoreError> {
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
    // The paragraph's note rides at the end of its text as a Fountain note, with nothing added
    // around it, so writing it back gives the same text.
    if let Some(note) = get(&para.attributes, "note").filter(|n| !n.trim().is_empty()) {
        markup.push_str(&format!("[[{}]]", note.replace("]]", "] ]")));
    }
    let mut element = Element::new(kind, markup);
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
    // Children other than our own marker go back as they came.
    let children = match marker {
        Some(_) => remove_marker(&para.children),
        None => para.children.clone(),
    };
    let attributes: Vec<(String, String)> = para.attributes.iter().filter(|(k, _)| !modelled_para_attribute(k)).cloned().collect();
    let mut style_attributes: Vec<(String, String)> = para.style.iter().filter(|(k, _)| !modelled_style_attribute(k)).cloned().collect();
    if kind == ElementKind::Centered {
        style_attributes.retain(|(k, _)| key(k) != "align");
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

fn remove_marker(children: &str) -> String {
    let Some(start) = children.find(&format!("<{MARKER}")) else { return children.to_string() };
    let end = children.get(start..).and_then(|rest| rest.find("/>").map(|at| start + at + 2)).unwrap_or(children.len());
    format!("{}{}", children.get(..start).unwrap_or_default(), children.get(end..).unwrap_or_default())
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
// Layout

/// Tenths of a millimetre to inches.
fn inches(tenths_mm: f32) -> f32 {
    tenths_mm / 254.0
}

fn number(attributes: &[(String, String)], name: &str) -> Option<f32> {
    get(attributes, name).and_then(|v| v.trim().parse::<f32>().ok()).filter(|v| v.is_finite() && v.abs() < 100_000.0)
}

/// Applies an imported document's page size, margins and built-in element styles to `layout`.
pub fn apply_layout(document: &OsfDocument, layout: &mut Layout) {
    let raw: Vec<&str> = document.sections.iter().filter_map(|s| if let OsfSection::Raw(raw) = s { Some(raw.as_str()) } else { None }).collect();
    let settings = raw.iter().flat_map(|r| tags(r)).find(|(tag, _)| tag.eq_ignore_ascii_case("settings")).map(|(_, a)| a).unwrap_or_default();
    let width = number(&settings, "page_width").unwrap_or(2159.0);
    let height = number(&settings, "page_height").unwrap_or(2794.0);
    let (top, bottom) = (number(&settings, "margin_top").unwrap_or(254.0), number(&settings, "margin_bottom").unwrap_or(254.0));
    let (left_margin, right_margin) = (number(&settings, "margin_left").unwrap_or(381.0), number(&settings, "margin_right").unwrap_or(254.0));
    if (1000.0..=5000.0).contains(&width) && (1000.0..=5000.0).contains(&height) {
        layout.page_columns = (inches(width) * 10.0).round() as u16;
        layout.page_rows = (inches(height) * 6.0).round() as u16;
        let top_rows = (inches(top) * 6.0).round() as u16;
        let bottom_rows = (inches(bottom) * 6.0).round() as u16;
        layout.body_top = top_rows;
        layout.lines_per_page = layout.page_rows.saturating_sub(top_rows).saturating_sub(bottom_rows).max(10);
    }
    if let Some(more) = get(&settings, "more_text").filter(|t| !t.trim().is_empty()) {
        layout.more = more.to_string();
    }
    if let Some(contd) = get(&settings, "cont_text").filter(|t| !t.trim().is_empty()) {
        layout.contd = format!("({})", contd.trim().trim_start_matches('(').trim_end_matches(')'));
    }
    for (tag, attributes) in raw.iter().flat_map(|r| tags(r)) {
        if !tag.eq_ignore_ascii_case("style") {
            continue;
        }
        let Some(name) = get(&attributes, "name") else { continue };
        let target = match name.to_ascii_lowercase().as_str() {
            "scene heading" => &mut layout.scene_heading,
            "action" => &mut layout.action,
            "character" => &mut layout.character,
            "parenthetical" => &mut layout.parenthetical,
            "dialogue" => &mut layout.dialogue,
            "transition" => &mut layout.transition,
            "shot" => &mut layout.shot,
            _ => continue,
        };
        apply_style(&attributes, target, width, left_margin, right_margin);
    }
}

fn apply_style(attributes: &[(String, String)], style: &mut ElementStyle, width: f32, left_margin: f32, right_margin: f32) {
    let left = left_margin + number(attributes, "leftindent").unwrap_or(0.0);
    let right = width - right_margin - number(attributes, "rightindent").unwrap_or(0.0);
    let first = number(attributes, "firstlineindent").unwrap_or(0.0).min(0.0);
    if right > left {
        style.left = (inches(left + first) * 10.0).round().max(0.0) as u16;
        style.width = chars_per_line(inches(right - left));
    }
    if let Some(space) = number(attributes, "spacebefore") {
        style.space_before = space.round().clamp(0.0, 10.0) as u16;
    }
    match get(attributes, "align").map(str::to_ascii_lowercase).as_deref() {
        Some("left") => style.align = Align::Left,
        Some("right") => style.align = Align::Right,
        Some("center") => style.align = Align::Center,
        _ => {}
    }
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
  </styles>"#;

const DEFAULT_SETTINGS: &str = r##"  <settings page_width="2159" page_height="2794" margin_top="254" margin_bottom="254" margin_left="381" margin_right="254" normal_linesperinch="6.0" element_spacing="1.00" break_on_sentences="true" dialogue_continues="true" dialogue_pagebreaks="true" cont_text="(CONT'D)" more_text="(MORE)" scenes_continue="false" scene_time_separator=" - " page_header="#." header_alignment="3" header_first_page="false"/>"##;

fn write_attr(name: &str, value: &str, out: &mut String) {
    out.push(' ');
    out.push_str(name);
    out.push_str("=\"");
    escape(value, out);
    out.push('"');
}

fn write_runs(runs: &[Run], out: &mut String) {
    let printed: Vec<&Run> = runs.iter().filter(|r| !r.note).collect();
    if printed.is_empty() {
        out.push_str("      <text></text>\n");
        return;
    }
    for run in printed {
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
        out.push('>');
        escape(&run.text, out);
        out.push_str("</text>\n");
    }
}

/// What a paragraph is written with besides its element.
#[derive(Default)]
struct ParaExtras {
    page_break: bool,
    dual_first: bool,
    synopsis: Option<String>,
}

fn write_para(element: &Element, extras: &ParaExtras, spelling: Spelling, styles_known: bool, out: &mut String) {
    let runs = inline::parse(&element.text);
    let notes: Vec<&str> = runs.iter().filter(|r| r.note).map(|r| r.text.trim()).filter(|t| !t.is_empty()).collect();
    let kept = element.osf.as_deref().filter(|o| o.kind == element.kind);
    out.push_str("    <para");
    if let Some(number) = element.scene_number.as_deref().filter(|_| element.kind == ElementKind::SceneHeading) {
        write_attr(spelling.scene_number(), number, out);
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
    let marker = match element.kind {
        ElementKind::Section => Some(format!("<{MARKER} kind=\"section\" level=\"{}\"/>", element.level.clamp(1, 6))),
        ElementKind::Synopsis => Some(format!("<{MARKER} kind=\"synopsis\"/>")),
        ElementKind::Lyrics if !styles_known => Some(format!("<{MARKER} kind=\"lyrics\"/>")),
        _ => None,
    };
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
    if let Some(osf) = element.osf.as_deref() {
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
    if let Some(osf) = element.osf.as_deref().filter(|o| !o.children.trim().is_empty()) {
        out.push_str("      ");
        out.push_str(osf.children.trim());
        out.push('\n');
    }
    write_runs(&runs, out);
    out.push_str("    </para>\n");
}

fn write_paragraphs(script: &Script, spelling: Spelling, styles_known: bool, out: &mut String) {
    out.push_str("  <paragraphs>\n");
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
        write_para(element, &ParaExtras { page_break: std::mem::take(&mut page_break), dual_first, synopsis }, spelling, styles_known, out);
        index += 1;
    }
    out.push_str("  </paragraphs>\n");
}

fn write_title_page(title: &TitlePage, out: &mut String) {
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
            out.push_str(&format!(">\n      <style basestylename=\"Normal Text\" align=\"{align}\"/>\n"));
            write_runs(&inline::parse(line), out);
            out.push_str("    </para>\n");
        }
        out.push_str("    <para>\n      <style basestylename=\"Normal Text\"/>\n      <text></text>\n    </para>\n");
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
                            write_title_page(&script.title_page, &mut out);
                        }
                        title_written = true;
                    }
                    OsfSection::TitlePage { .. } => {}
                    OsfSection::Raw(raw) => {
                        out.push_str("  ");
                        out.push_str(raw.trim());
                        out.push('\n');
                    }
                }
            }
            if !paragraphs_written {
                write_paragraphs(script, spelling, styles_known, &mut out);
            }
            if !title_written {
                write_title_page(&script.title_page, &mut out);
            }
        }
        None => {
            out.push_str("<document type=\"Open Screenplay Format document\" version=\"30\">\n");
            out.push_str(DEFAULT_SETTINGS);
            out.push('\n');
            out.push_str(DEFAULT_STYLES);
            out.push('\n');
            write_paragraphs(script, Spelling::Snake, true, &mut out);
            write_title_page(&script.title_page, &mut out);
        }
    }
    out.push_str("</document>\n");
    out
}
