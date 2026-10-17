//! Final Draft XML (`.fdx`) import and export.
//!
//! Clean-room: built from the publicly known structure of the format (`FinalDraft` →
//! `Content` → `Paragraph[@Type]` → `Text[@Style]`, `DualDialogue`, `TitlePage`), from public
//! sample files and from files this module writes (see docs/research/v1-notes/fdx-format.md).
//!
//! Everything we do not model is kept and written back: other top-level sections (element
//! settings, page layout, revisions, tags …) verbatim in [`FdxDocument`], and each paragraph's
//! own type name, extra attributes and non-text children (scene properties, notes, omitted
//! scenes) in [`FdxParagraph`]. Per-run attributes such as revision ids are not kept yet.

use quick_xml::XmlVersion;
use quick_xml::events::{BytesStart, Event};

use crate::inline::{self, Run, Style};
use crate::layout::{Align, ElementStyle, Layout, chars_per_line};
use crate::model::{Element, ElementKind, FdxDocument, FdxParagraph, FdxSection, Script, TitleField, TitlePage};
use crate::paginate;
use crate::{CoreError, MAX_ELEMENTS, MAX_INPUT_BYTES};

/// Deepest element nesting accepted.
const MAX_DEPTH: usize = 128;
/// Our own marker for which title-page field a paragraph holds (ignored by other apps).
const FIELD_ATTR: &str = "StorytellerField";

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

fn write_attr(key: &str, value: &str, out: &mut String) {
    out.push(' ');
    out.push_str(key);
    out.push_str("=\"");
    escape(value, out);
    out.push('"');
}

/// `tag` (a start tag such as `<SceneProperties Length="1" Page="3"/>`) with `name` set to `value`.
fn set_tag_attr(tag: &str, name: &str, value: &str) -> String {
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
    write_text_runs(&element.text, &format!("{indent}  "), out);
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
    let mut title_written = false;
    for section in sections {
        match section {
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
    has_text: bool,
    /// The paragraph wraps a dual-dialogue block; it is not an element itself.
    wraps_dual: bool,
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
}

/// The text style of the `Text` run being read: our style plus Final Draft's `AllCaps`.
#[derive(Clone, Copy)]
struct RunStyle {
    style: Style,
    all_caps: bool,
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
    let mut run = RunStyle { style: Style::default(), all_caps: false };
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
                has_text: false,
                wraps_dual: false,
            });
            return None;
        }
        let parent = self.parent_paragraph().map(|p| p.region);
        match (name, parent) {
            ("Text", Some(_)) => {
                self.run = Some(parse_style(attr(start, "Style")));
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
            (_, Some(Region::Script)) => Some(CaptureInto::Paragraph),
            _ => None,
        }
    }

    fn text(&mut self, text: &str) {
        let Some(run) = self.run else { return };
        let Some(paragraph) = self.paragraphs.last_mut() else { return };
        let text = if run.all_caps { text.to_uppercase() } else { text.to_string() };
        match paragraph.runs.last_mut() {
            Some(last) if last.style == run.style => last.text.push_str(&text),
            _ => paragraph.runs.push(Run { text, style: run.style, note: false }),
        }
    }

    fn captured(&mut self, into: CaptureInto, xml: &str) {
        match into {
            CaptureInto::Section => self.document.sections.push(FdxSection::Raw(xml.to_string())),
            CaptureInto::Paragraph => {
                if let Some(paragraph) = self.parent_paragraph() {
                    paragraph.children.push_str(xml);
                }
            }
        }
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
        }
        let type_name = paragraph.kind.unwrap_or_else(|| "Action".into());
        let (mut kind, level) = kind_from_fdx(&type_name);
        let centered = paragraph.alignment.as_deref().is_some_and(|a| a.eq_ignore_ascii_case("center"));
        if kind == ElementKind::Action && centered && type_name.trim() == "Action" {
            kind = ElementKind::Centered;
        }
        let mut element = Element::new(kind, markup);
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
        Ok(())
    }

    fn into_script(mut self) -> Script {
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
        Script { title_page, elements: self.elements, fdx: Some(Box::new(self.document)), osf: None }
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
                        importer.captured(into, xml);
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
                let skip = name == "Paragraph" && paragraph_region(&importer.stack).is_some();
                if !skip {
                    match importer.open(&name, &start, before) {
                        Some(into) => importer.captured(into, source.get(before..after).unwrap_or_default()),
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
            Event::GeneralRef(reference) => {
                let resolved = match reference.resolve_char_ref().map_err(xml_error)? {
                    Some(c) => c.to_string(),
                    None => {
                        let name = reference.decode().map_err(xml_error)?;
                        quick_xml::escape::resolve_predefined_entity(&name).unwrap_or("").to_string()
                    }
                };
                importer.text(&resolved);
            }
            Event::Eof => break,
            _ => {}
        }
    }
    if !saw_root {
        return Err(CoreError::NotFdx("no <FinalDraft> root element".into()));
    }
    Ok(importer.into_script())
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
                }
            }
            _ => {}
        }
    }
}
