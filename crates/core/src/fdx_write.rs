//! Writing `.fdx`: paragraphs, text runs, scene properties and the document around them.

use super::*;

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

pub(super) fn style_attr(style: Style) -> String {
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

pub(super) fn write_text_runs(markup: &str, indent: &str, out: &mut String) {
    let runs = inline::parse(markup);
    if runs.is_empty() {
        out.push_str(indent);
        out.push_str("<Text></Text>\n");
        return;
    }
    for run in runs {
        out.push_str(indent);
        // A plain `[[note]]` is a script note, as in the script, so it doesn't print.
        if run.note {
            out.push_str(&plain_note_xml(&run.text));
            out.push('\n');
            continue;
        }
        let style = style_attr(run.style);
        if style.is_empty() {
            out.push_str("<Text>");
        } else {
            out.push_str(&format!("<Text Style=\"{style}\">"));
        }
        escape(&run.text, out);
        out.push_str("</Text>\n");
    }
}

pub(super) fn write_run(text: &str, style: Style, revision: Option<u32>, indent: &str, out: &mut String) {
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
pub(super) fn write_body(element: &Element, indent: &str, out: &mut String) {
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

pub(super) fn write_attr(key: &str, value: &str, out: &mut String) {
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

/// Our child of an omitted scene's heading: the scene it replaced, as Fountain. Final Draft
/// keeps an omitted scene's content in a way no saved file has shown yet; this keeps ours.
pub(super) const OMITTED_CHILD: &str = "StorytellerOmitted";
/// The `.fdx` name of each autocomplete list, and of an entry in it.
pub(super) const SMART_TYPE: [(&str, &str, &str); 6] = [
    ("characters", "Characters", "Character"),
    ("extensions", "Extensions", "Extension"),
    ("intros", "SceneIntros", "SceneIntro"),
    ("locations", "Locations", "Location"),
    ("times", "TimesOfDay", "TimeOfDay"),
    ("transitions", "Transitions", "Transition"),
];

/// The autocomplete lists a `SmartType` section holds.
pub(super) fn read_lists(xml: &str) -> crate::autocomplete::Lists {
    let mut lists = crate::autocomplete::Lists::default();
    for (name, section, _) in SMART_TYPE {
        if let Some(list) = lists.list_mut(name) {
            *list = ignored_words(xml, &loose(section), crate::autocomplete::MAX_LIST);
        }
    }
    let separator = |section: &str| -> Option<String> {
        let mut reader = quick_xml::Reader::from_str(xml);
        while let Ok(event) = reader.read_event() {
            match event {
                Event::Start(start) | Event::Empty(start) if loose(start.local_name().as_ref()) == loose(section) => {
                    return attr(&start, "Separator");
                }
                Event::Eof => return None,
                _ => {}
            }
        }
        None
    };
    lists.intro_separator = separator("SceneIntros");
    lists.time_separator = separator("TimesOfDay");
    lists
}

/// A `SmartType` section for `lists`.
pub(super) fn lists_xml(lists: &crate::autocomplete::Lists) -> String {
    let mut out = String::from("<SmartType>\n");
    for (name, section, entry) in SMART_TYPE {
        out.push_str(&format!("    <{section}"));
        let separator = match section {
            "SceneIntros" => lists.intro_separator.as_deref().or(Some(". ")),
            "TimesOfDay" => lists.time_separator.as_deref().or(Some(" - ")),
            _ => None,
        };
        if let Some(separator) = separator {
            out.push_str(" Separator=\"");
            escape(separator, &mut out);
            out.push('"');
        }
        out.push_str(">\n");
        for text in lists.list(name).into_iter().flatten() {
            out.push_str(&format!("      <{entry}>"));
            escape(text, &mut out);
            out.push_str(&format!("</{entry}>\n"));
        }
        out.push_str(&format!("    </{section}>\n"));
    }
    out.push_str("  </SmartType>");
    out
}

/// Our child of a paragraph with a bookmark: its name.
pub(super) const BOOKMARK_CHILD: &str = "StorytellerBookmark";

/// The text of our child `tag`, and the children without it.
pub(super) fn take_child(children: &str, tag: &str) -> Option<(String, String)> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let start = children.find(&open)?;
    let end = start + children.get(start..)?.find(&close)? + close.len();
    let raw = children.get(start + open.len()..end - close.len())?;
    let text = quick_xml::escape::unescape(raw).map(|t| t.into_owned()).unwrap_or_else(|_| raw.to_string());
    let rest = format!("{}{}", children.get(..start).unwrap_or_default(), children.get(end..).unwrap_or_default());
    Some((text, rest.trim().to_string()))
}

/// The paragraph's own `SceneProperties`, which Final Draft writes first among its children:
/// (where its start tag begins, where it ends, where the whole element ends). An omitted scene
/// keeps its original heading, with properties of its own, nested further in.
pub(super) fn scene_properties(children: &str) -> Option<(usize, usize, usize)> {
    let start = children.len() - children.trim_start().len();
    if !children.get(start..)?.starts_with("<SceneProperties") {
        return None;
    }
    let tag_end = start + children.get(start..)?.find('>')? + 1;
    if children.get(..tag_end)?.ends_with("/>") {
        return Some((start, tag_end, tag_end));
    }
    let close = "</SceneProperties>";
    let end = tag_end + children.get(tag_end..)?.find(close)? + close.len();
    Some((start, tag_end, end))
}

/// The scene title, summary and colour a heading's `SceneProperties` holds.
pub(super) fn scene_meta(children: &str) -> SceneMeta {
    let Some((start, tag_end, end)) = scene_properties(children) else { return SceneMeta::default() };
    let tag = children.get(start..tag_end).unwrap_or_default();
    let mut reader = quick_xml::Reader::from_str(tag);
    let attributes = match reader.read_event() {
        Ok(Event::Start(e) | Event::Empty(e)) => all_attrs(&e),
        _ => Vec::new(),
    };
    let attr = |name: &str| attributes.iter().find(|(k, _)| k == name).map(|(_, v)| v.clone());
    let inner = children.get(tag_end..end).unwrap_or_default();
    let summary = inner
        .find("<Summary>")
        .and_then(|at| {
            let from = at + "<Summary>".len();
            inner.get(from..from + inner.get(from..)?.find("</Summary>")?)
        })
        .map(|raw| {
            // Plain text, as every summary in the corpus is; paragraphs inside, if any, one a
            // line, each paragraph's styled runs joined as they read.
            let runs = |paragraph: &str| -> String {
                paragraph
                    .split("<Text")
                    .skip(1)
                    .filter_map(|t| t.split_once('>').and_then(|(_, rest)| rest.split_once("</Text>")).map(|(text, _)| text))
                    .collect()
            };
            let text = if raw.contains("<Text") {
                if raw.contains("<Paragraph") { raw.split("<Paragraph").skip(1).map(runs).collect::<Vec<_>>().join("\n") } else { runs(raw) }
            } else {
                raw.to_string()
            };
            quick_xml::escape::unescape(text.trim()).map(|t| t.into_owned()).unwrap_or_else(|_| text.trim().to_string())
        })
        .unwrap_or_default();
    SceneMeta { title: attr("Title").unwrap_or_default(), summary, color: attr("Color").filter(|c| !c.is_empty()) }
}

/// A colour as Final Draft writes it, `#RRRRGGGGBBBB`; one already in that form, as it is.
pub(super) fn fdx_color(color: &str) -> String {
    let hex = color.trim().trim_start_matches('#');
    if hex.len() == 6 && hex.chars().all(|c| c.is_ascii_hexdigit()) {
        let up = hex.to_uppercase();
        let (r, g, b) = (&up[0..2], &up[2..4], &up[4..6]);
        return format!("#{r}{r}{g}{g}{b}{b}");
    }
    color.to_string()
}

/// `text` escaped for XML content.
pub(super) fn escaped(text: &str) -> String {
    let mut out = String::new();
    escape(text, &mut out);
    out
}

/// A scene heading's children with its `SceneProperties` page and length set from our own
/// pagination, and its title, summary and colour from the scene's, when they changed: updated
/// in place when the file had one, otherwise added first.
pub(super) fn scene_children(children: &str, page: &str, eighths: u32, meta: &SceneMeta) -> String {
    let length = paginate::format_eighths(eighths);
    let Some((start, tag_end, end)) = scene_properties(children) else {
        let mut tag = format!("<SceneProperties Length=\"{length}\" Page=\"{page}\" Title=\"{}\"", escaped(&meta.title));
        if let Some(color) = &meta.color {
            tag.push_str(&format!(" Color=\"{}\"", escaped(&fdx_color(color))));
        }
        let properties = if meta.summary.is_empty() {
            format!("{tag}/>")
        } else {
            format!("{tag}><Summary>{}</Summary></SceneProperties>", escaped(&meta.summary))
        };
        return format!("{properties}{children}");
    };
    let before = children.get(..start).unwrap_or_default();
    let after = children.get(end..).unwrap_or_default();
    let mut tag = set_tag_attr(&set_tag_attr(children.get(start..tag_end).unwrap_or_default(), "Length", &length), "Page", page);
    let mut inner = children.get(tag_end..end).unwrap_or_default().to_string();
    let read = scene_meta(children);
    if read == *meta {
        return format!("{before}{tag}{inner}{after}");
    }
    // Something changed: title and colour in the tag, the summary in its own element.
    tag = set_tag_attr(&tag, "Title", &escaped(&meta.title));
    tag = match &meta.color {
        Some(color) => set_tag_attr(&tag, "Color", &escaped(&fdx_color(color))),
        None => remove_tag_attr(&tag, "Color"),
    };
    if read.summary != meta.summary {
        let summary = if meta.summary.is_empty() { String::new() } else { format!("<Summary>{}</Summary>", escaped(&meta.summary)) };
        if let Some(at) = inner.find("<Summary>")
            && let Some(len) = inner.get(at..).and_then(|rest| rest.find("</Summary>"))
        {
            inner.replace_range(at..at + len + "</Summary>".len(), &summary);
        } else if !summary.is_empty() {
            if tag.ends_with("/>") {
                tag = format!("{}>", tag.trim_end_matches("/>").trim_end());
                inner = format!("{summary}</SceneProperties>");
            } else if let Some(close) = inner.rfind("</SceneProperties>") {
                inner.insert_str(close, &summary);
            }
        }
    }
    format!("{before}{tag}{inner}{after}")
}

/// `tag` without the attribute `name`.
pub(super) fn remove_tag_attr(tag: &str, name: &str) -> String {
    let key = format!(" {name}=\"");
    let Some(at) = tag.find(&key) else { return tag.to_string() };
    let start = at + key.len();
    match tag.get(start..).and_then(|rest| rest.find('"')) {
        Some(len) => format!("{}{}", tag.get(..at).unwrap_or_default(), tag.get(start + len + 1..).unwrap_or_default()),
        None => tag.to_string(),
    }
}

pub(super) fn write_paragraph(element: &Element, scene: Option<(&str, u32)>, new_page: bool, indent: &str, out: &mut String) {
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
        Some((page, eighths)) => scene_children(kept, page, eighths, &element.scene),
        None => kept.to_string(),
    };
    if !children.is_empty() {
        out.push_str(indent);
        out.push_str("  ");
        out.push_str(&children);
        out.push('\n');
    }
    if let Some(omitted) = element.omitted.as_deref().filter(|o| !o.trim().is_empty()) {
        out.push_str(indent);
        out.push_str(&format!("  <{OMITTED_CHILD}>"));
        escape(omitted, out);
        out.push_str(&format!("</{OMITTED_CHILD}>\n"));
    }
    if let Some(name) = element.bookmark.as_deref() {
        out.push_str(indent);
        out.push_str(&format!("  <{BOOKMARK_CHILD}>"));
        escape(name, out);
        out.push_str(&format!("</{BOOKMARK_CHILD}>\n"));
    }
    write_body(element, &format!("{indent}  "), out);
    out.push_str(indent);
    out.push_str("</Paragraph>\n");
}

pub(super) fn speech_end(elements: &[Element], cue: usize) -> usize {
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
    // The page as printed: `12A` in a script whose pages are locked.
    let label = |page: u32| {
        usize::try_from(page)
            .ok()
            .and_then(|p| p.checked_sub(1))
            .and_then(|p| paginated.pages.get(p))
            .map_or_else(|| page.to_string(), paginate::Page::label)
    };
    let scenes: std::collections::HashMap<usize, (String, u32)> =
        paginate::scene_lengths(script, &paginated, layout).into_iter().map(|s| (s.element, (label(s.page), s.eighths))).collect();
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
        write_paragraph(element, scenes.get(&index).map(|(page, eighths)| (page.as_str(), *eighths)), new_page, "    ", &mut out);
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
    let marks = deletions(elements);
    let marks_same = script.fdx.as_deref().map(|d| d.deletions.as_slice()).unwrap_or_default() == marks.as_slice();
    let mut marks_written = false;
    let mut title_written = false;
    let format_same = script.fdx.as_ref().and_then(|d| d.format.as_ref()) == Some(&script.format);
    let mut ignored_written = false;
    let mut lists_written = false;
    let (mut elements_written, mut layout_written, mut mores_written, mut numbers_written) = (false, false, false, false);
    for section in sections {
        match section {
            // A changed format is written from the model, all element settings where the first was.
            FdxSection::Raw(xml) if !format_same && section_is(xml, "ElementSettings") => {
                if !elements_written {
                    write_element_settings(&script.format, &mut out);
                    elements_written = true;
                }
            }
            FdxSection::Raw(xml) if !format_same && section_is(xml, "PageLayout") => {
                if let Some(page) = script.format.page.as_ref().filter(|_| !layout_written) {
                    out.push_str(&format!("  {}\n", crate::format::fdx_page_layout(page)));
                }
                layout_written = true;
            }
            FdxSection::Raw(xml) if !format_same && section_is(xml, "MoresAndContinueds") => {
                if let Some(page) = script.format.page.as_ref().filter(|_| !mores_written) {
                    out.push_str(&format!("  {}\n", crate::format::fdx_mores(page)));
                }
                mores_written = true;
            }
            FdxSection::Raw(xml) if !format_same && section_is(xml, "SceneNumberOptions") => {
                if let Some(page) = script.format.page.as_ref().filter(|_| !numbers_written) {
                    out.push_str(&format!("  {}\n", crate::format::fdx_scene_numbers(page)));
                }
                numbers_written = true;
            }
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
            FdxSection::Raw(xml) if section_is(xml, "SmartType") => {
                out.push_str("  ");
                if script.fdx.as_ref().is_some_and(|d| d.lists == script.lists) {
                    out.push_str(xml);
                } else {
                    out.push_str(&lists_xml(&script.lists));
                }
                out.push('\n');
                lists_written = true;
            }
            FdxSection::Raw(xml) if section_is(xml, "SpellCheckIgnoreLists") && !ignored_written => {
                if script.fdx.as_ref().is_some_and(|d| d.spelling_ignored == script.spelling_ignored) {
                    out.push_str("  ");
                    out.push_str(xml);
                } else {
                    out.push_str("  ");
                    out.push_str(&replace_child(xml, "ignoredwords", &ignored_words_xml(&script.spelling_ignored)));
                }
                out.push('\n');
                ignored_written = true;
            }
            FdxSection::Raw(xml) if section_is(xml, "DeletedText") && !marks_written => {
                if marks_same {
                    out.push_str("  ");
                    out.push_str(xml);
                    out.push('\n');
                } else {
                    let root = tags(xml).into_iter().next().map(|(_, a)| a).unwrap_or_default();
                    write_deletions(&root, &marks, &mut out);
                }
                marks_written = true;
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
    if !format_same {
        if !elements_written {
            write_element_settings(&script.format, &mut out);
        }
        if let Some(page) = &script.format.page {
            if !layout_written {
                out.push_str(&format!("  {}\n", crate::format::fdx_page_layout(page)));
            }
            if !mores_written {
                out.push_str(&format!("  {}\n", crate::format::fdx_mores(page)));
            }
            // Written only when a margin is said, so a file that never had the section stays without it.
            if !numbers_written && (page.numbers_left.is_some() || page.numbers_right.is_some() || !page.fdx_scene_numbers.is_empty()) {
                out.push_str(&format!("  {}\n", crate::format::fdx_scene_numbers(page)));
            }
        }
    }
    if !lists_written && !script.lists.is_empty() {
        out.push_str(&format!("  {}\n", lists_xml(&script.lists)));
    }
    if !ignored_written && !script.spelling_ignored.is_empty() {
        out.push_str(&format!("  <SpellCheckIgnoreLists><IgnoredRanges/>{}</SpellCheckIgnoreLists>\n", ignored_words_xml(&script.spelling_ignored)));
    }
    if !locks_written && !locks.is_empty() {
        write_locked_pages(&[], &locks, &mut out);
    }
    if !marks_written && !marks.is_empty() {
        write_deletions(&[], &marks, &mut out);
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

pub(super) fn write_element_settings(format: &crate::format::Format, out: &mut String) {
    for element in &format.elements {
        out.push_str(&format!("  {}\n", crate::format::fdx_element(element, format.page.as_ref())));
    }
}

pub(super) fn write_title_page(title: &TitlePage, out: &mut String) {
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
