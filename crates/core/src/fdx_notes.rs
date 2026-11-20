//! Script notes: in a paragraph, in the ranged `ScriptNotes` list, and unanchored.

use super::*;

/// A top-level section's raw XML is the element `name`.
pub(super) fn section_is(xml: &str, name: &str) -> bool {
    xml.strip_prefix('<').and_then(|rest| rest.strip_prefix(name)).is_some_and(|rest| rest.starts_with([' ', '>', '/', '\n', '\t', '\r']))
}

/// The raw XML of each direct child element of a fragment's root.
pub(crate) fn child_elements(xml: &str) -> Vec<&str> {
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
pub(super) struct ReadNote {
    pub(super) note: ScriptNote,
    /// It was written for a plain `[[note]]`.
    pub(super) plain: bool,
    /// Its `Range`, for a listed note.
    pub(super) range: Option<(u32, u32)>,
}

pub(super) fn read_note(xml: &str, form: FdxNoteForm) -> Option<ReadNote> {
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
                    (2, "Paragraph") => paragraphs.push(String::new()),
                    (3, "Text") => in_text = true,
                    _ => {}
                }
                continue;
            }
            Event::Empty(start) => {
                match (depth, start.local_name().as_ref()) {
                    (0, _) => attributes = Some(all_attrs(&start)),
                    (1, "Paragraph") => paragraphs.push(String::new()),
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
            Event::Text(text) if in_text => text.xml_content(XmlVersion::Implicit1_0).into_owned(),
            Event::CData(data) if in_text => data.as_ref().to_string(),
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

pub(super) fn set_or_remove(attributes: &mut Vec<(String, String)>, key: &str, value: Option<String>) {
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
pub(super) fn note_unchanged(note: &ScriptNote) -> Option<&str> {
    let fdx = note.fdx.as_deref()?;
    let read = fdx.as_read.as_deref()?;
    (!fdx.raw.is_empty() && read.text == note.text && read.color == note.color && read.author == note.author).then_some(fdx.raw.as_str())
}

pub(super) fn note_xml(note: &ScriptNote, attributes: &[(String, String)]) -> String {
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
pub(super) fn inline_note_xml(note: &ScriptNote) -> String {
    match note_unchanged(note) {
        Some(raw) => raw.to_string(),
        None => note_xml(note, &note_attributes(note, note_form(note), None)),
    }
}

/// A note of the `ScriptNotes` list, covering `from..to` of the whole script.
pub(super) fn ranged_note_xml(note: &ScriptNote, from: u32, to: u32) -> String {
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
pub(super) fn plain_note_xml(text: &str) -> String {
    note_xml(&ScriptNote { text: text.to_string(), ..ScriptNote::default() }, &[(INLINE_NOTE_ATTR.to_string(), "Yes".to_string())])
}

pub(super) fn write_note_list<I: IntoIterator<Item = String>>(name: &str, notes: I, out: &mut String) {
    out.push_str(&format!("  <{name}>\n"));
    for note in notes {
        out.push_str("    ");
        out.push_str(&note);
        out.push('\n');
    }
    out.push_str(&format!("  </{name}>\n"));
}

/// Printed characters of an element, as Final Draft counts them for `ScriptNotes@Range`.
pub(super) fn printed_len(element: &Element) -> u32 {
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
pub(super) fn ranged_notes(elements: &[Element]) -> Vec<(u32, u32, &ScriptNote)> {
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
