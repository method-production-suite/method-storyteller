//! Reading `.fdx`: the importer that walks the XML into a script.

use super::*;

/// Where an open script or title-page paragraph sits.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Region {
    Script,
    Title,
}

pub(super) struct ParagraphState {
    pub(super) region: Region,
    /// Stack depth of the `Paragraph` element itself.
    pub(super) depth: usize,
    pub(super) kind: Option<String>,
    pub(super) alignment: Option<String>,
    pub(super) number: Option<String>,
    pub(super) level: u8,
    pub(super) new_page: bool,
    pub(super) field: Option<String>,
    pub(super) attributes: Vec<(String, String)>,
    pub(super) children: String,
    pub(super) runs: Vec<Run>,
    pub(super) notes: Vec<ScriptNote>,
    pub(super) revised: Vec<Revised>,
    /// Characters of printed text read so far.
    pub(super) chars: u32,
    pub(super) has_text: bool,
    /// The paragraph wraps a dual-dialogue block; it is not an element itself.
    pub(super) wraps_dual: bool,
}

impl ParagraphState {
    /// Characters of printed text read so far.
    fn printed_chars(&self) -> u32 {
        self.chars
    }
}

pub(super) struct TitleParagraph {
    pub(super) field: Option<String>,
    pub(super) alignment: String,
    pub(super) text: String,
}

/// A child element being copied verbatim, from its start tag to its end tag.
pub(super) struct Capture {
    pub(super) depth: usize,
    pub(super) start: usize,
    pub(super) into: CaptureInto,
}

#[derive(Clone, Copy)]
pub(super) enum CaptureInto {
    Section,
    Paragraph,
    /// A `ScriptNote` inside a script paragraph.
    Note,
}

/// The text style of the `Text` run being read: our style plus Final Draft's `AllCaps`.
#[derive(Clone, Copy)]
pub(super) struct RunStyle {
    pub(super) style: Style,
    pub(super) all_caps: bool,
    pub(super) revision: u32,
}

/// Attributes the model holds itself, so they are not kept in [`FdxParagraph::attributes`].
pub(super) const MODELLED_ATTRIBUTES: [&str; 5] = ["Type", "Number", "StartsNewPage", "Level", FIELD_ATTR];

pub(super) fn attr(element: &BytesStart<'_>, name: &str) -> Option<String> {
    element
        .attributes()
        .flatten()
        .find(|a| a.key.as_ref() == name)
        .and_then(|a| a.normalized_value(XmlVersion::Implicit1_0).ok())
        .map(|v| v.into_owned())
}

pub(crate) fn all_attrs(element: &BytesStart<'_>) -> Vec<(String, String)> {
    element
        .attributes()
        .flatten()
        .filter_map(|a| {
            let value = a.normalized_value(XmlVersion::Implicit1_0).ok()?;
            Some((a.key.as_ref().to_string(), value.into_owned()))
        })
        .collect()
}

pub(super) fn parse_style(value: Option<String>) -> RunStyle {
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

pub(super) fn xml_error(error: impl std::fmt::Display) -> CoreError {
    CoreError::Xml(error.to_string().chars().take(200).collect())
}

/// The text an entity or character reference stands for.
pub(super) fn reference_text(reference: &BytesRef<'_>) -> Result<String, CoreError> {
    Ok(match reference.resolve_char_ref().map_err(xml_error)? {
        Some(c) => c.to_string(),
        None => quick_xml::escape::resolve_predefined_entity(reference.as_ref()).unwrap_or("").to_string(),
    })
}

pub(super) fn names(stack: &[String]) -> Vec<&str> {
    stack.iter().map(String::as_str).collect()
}

/// The region of a `Paragraph` at the top of `stack`, if it is a script or title-page paragraph.
/// Only direct children of `Content` (or of a dual-dialogue block) count: paragraphs nested in
/// scene properties, notes or omitted scenes belong to their parent and are kept verbatim.
pub(super) fn paragraph_region(stack: &[String]) -> Option<Region> {
    match names(stack).as_slice() {
        ["FinalDraft", "Content", "Paragraph"] | ["FinalDraft", "Content", "Paragraph", "DualDialogue", "Paragraph"] => Some(Region::Script),
        ["FinalDraft", "TitlePage", "Content", "Paragraph"] => Some(Region::Title),
        _ => None,
    }
}

/// Everything collected while reading, finished into a [`Script`] at the end.
#[derive(Default)]
pub(super) struct Importer {
    pub(super) stack: Vec<String>,
    pub(super) paragraphs: Vec<ParagraphState>,
    pub(super) run: Option<RunStyle>,
    pub(super) in_dual: bool,
    pub(super) dual_cues: usize,
    pub(super) elements: Vec<Element>,
    pub(super) title: Vec<TitleParagraph>,
    pub(super) pending_break: bool,
    pub(super) document: FdxDocument,
    pub(super) title_start: Option<usize>,
    /// Where the next script paragraph starts in the script's text, as `ScriptNotes@Range` counts.
    pub(super) offset: u32,
    /// Where each element starts in the script's text (`None` for page breaks).
    pub(super) starts: Vec<Option<u32>>,
    pub(super) notes: usize,
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
            // A title page keeps the plain `[[notes]]` this app writes there; other notes on it
            // stay in the title page's own copy, which is written back while it is unchanged.
            ("ScriptNote", Some(Region::Script | Region::Title)) => Some(CaptureInto::Note),
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
            _ => paragraph.runs.push(Run { text, style: run.style, note: false, revised: None }),
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
                    paragraph.runs.push(Run { text: read.note.text, style: Style::default(), note: true, revised: None });
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
            element.scene = scene_meta(&paragraph.children);
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
        // The scene an omitted heading keeps, in our own child: modelled, so not kept raw.
        let mut children = paragraph.children;
        if kind == ElementKind::SceneHeading
            && let Some((omitted, rest)) = take_child(&children, OMITTED_CHILD)
        {
            element.omitted = Some(omitted);
            children = rest;
        }
        if let Some((name, rest)) = take_child(&children, BOOKMARK_CHILD) {
            element.bookmark = Some(name).filter(|name| !name.trim().is_empty());
            children = rest;
        }
        if type_name != fdx_type(&element) || !attributes.is_empty() || !children.is_empty() {
            element.fdx = Some(Box::new(FdxParagraph { type_name, kind, attributes, children }));
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
        // Pages this app locked and wrote: their positions are its own, so they are trusted.
        let ours = tags(&xml).first().is_some_and(|(_, root)| get(root, LOCKS_PLACED_HERE).is_some_and(|v| v == "Yes"));
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
        let mut pages: Vec<(u32, usize, PageStart)> = Vec::new();
        for (position, attributes) in placed {
            let mut kept = attributes.clone();
            for (key, value) in &mut kept {
                if key == "Position" {
                    value.clear();
                }
            }
            let after = starts.partition_point(|(_, s)| *s <= position);
            let Some(&(index, start)) = starts.get(after.saturating_sub(1)) else { continue };
            let label = get(&attributes, "PageNumber").unwrap_or_default().to_string();
            pages.push((position, index, PageStart { at: position.saturating_sub(start), label, fdx_attributes: kept, level: 0 }));
        }
        // The marker is trusted only while the text agrees: every page starts a paragraph or a
        // word. Another app that kept the marker but moved the pages leaves some inside a word.
        let starts_a_word = |index: usize, at: u32| {
            let Some(element) = self.elements.get(index) else { return false };
            let plain: Vec<char> = inline::plain(&element.text).chars().collect();
            let at = usize::try_from(at).unwrap_or(usize::MAX);
            at == 0 || at >= plain.len() || plain.get(at - 1).is_some_and(|c| c.is_whitespace())
        };
        if ours && pages.iter().all(|(_, index, page)| starts_a_word(*index, page.at)) {
            for (_, _, page) in &mut pages {
                page.level = number(get(&page.fdx_attributes, "LockLevel")).unwrap_or(0);
                page.fdx_attributes.clear();
            }
        }
        for (position, index, page) in pages {
            self.document.locked_pages.push((position, page.clone()));
            if let Some(element) = self.elements.get_mut(index) {
                element.page_starts.push(page);
            }
        }
        Ok(())
    }

    /// Reads the `DeletedText` list onto the paragraphs its marks fall in.
    fn place_deletions(&mut self) -> Result<(), CoreError> {
        let Some(xml) = self.document.sections.iter().find_map(|s| match s {
            FdxSection::Raw(xml) if section_is(xml, "DeletedText") => Some(xml.clone()),
            _ => None,
        }) else {
            return Ok(());
        };
        let starts: Vec<(usize, u32)> = self.starts.iter().enumerate().filter_map(|(i, s)| s.map(|s| (i, s))).collect();
        let entries: Vec<Vec<(String, String)>> =
            tags(&xml).into_iter().skip(1).filter(|(name, _)| name == "DeletedTextLocation").map(|(_, a)| a).collect();
        if entries.len() > MAX_ELEMENTS {
            return Err(CoreError::TooManyElements);
        }
        for attributes in entries {
            let (Some(offset), Some(set)) = (number(get(&attributes, "Offset")), number(get(&attributes, "RevisionID"))) else { continue };
            let after = starts.partition_point(|(_, s)| *s <= offset);
            let Some(&(index, start)) = starts.get(after.saturating_sub(1)) else { continue };
            let deletion = Deletion { at: offset.saturating_sub(start), set };
            self.document.deletions.push((offset, deletion));
            if let Some(element) = self.elements.get_mut(index) {
                element.deletions.push(deletion);
            }
        }
        self.document.deletions.sort_by_key(|(offset, d)| (*offset, d.set));
        // In one order everywhere, whatever order the file listed two sets at one place in.
        for element in &mut self.elements {
            element.deletions.sort_by_key(|d| (d.at, d.set));
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
        self.place_deletions()?;
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
        let spelling_ignored = self
            .document
            .sections
            .iter()
            .find_map(|s| match s {
                FdxSection::Raw(xml) if section_is(xml, "SpellCheckIgnoreLists") => Some(ignored_words(xml, "ignoredwords", MAX_ELEMENTS)),
                _ => None,
            })
            .unwrap_or_default();
        self.document.spelling_ignored = spelling_ignored.clone();
        let lists = self
            .document
            .sections
            .iter()
            .find_map(|s| match s {
                FdxSection::Raw(xml) if section_is(xml, "SmartType") => Some(read_lists(xml)),
                _ => None,
            })
            .unwrap_or_default();
        self.document.lists = lists.clone();
        let format = crate::format::from_fdx(self.document.sections.iter().filter_map(|s| match s {
            FdxSection::Raw(xml) => Some(xml.as_str()),
            FdxSection::TitlePage { .. } => None,
        }));
        self.document.format = Some(format.clone());
        Ok(Script {
            lists,
            title_page,
            elements: self.elements,
            revisions: revisions.unwrap_or_default(),
            format,
            fdx: Some(Box::new(self.document)),
            spelling_ignored,
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
                Event::Start(start) => importer.push(start.local_name().as_ref())?,
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
                let name = start.local_name().as_ref().to_string();
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
                let name = start.local_name().as_ref().to_string();
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
                let content = text.xml_content(XmlVersion::Implicit1_0);
                importer.text(&content);
            }
            Event::CData(data) => {
                importer.text(data.as_ref());
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
pub(super) fn hidden_types(sections: &[FdxSection]) -> Option<Vec<(String, bool)>> {
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
                    "ElementSettings" => current = Some((attr(&start, "Type").unwrap_or_default(), false)),
                    "FontSpec" => {
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
pub(super) fn apply_hidden_types(elements: &mut [Element], sections: &[FdxSection]) {
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

pub(super) fn title_page_from(paragraphs: Vec<TitleParagraph>) -> TitlePage {
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
