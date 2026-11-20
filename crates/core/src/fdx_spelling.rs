//! The words spell check ignores, kept in the document.

use super::*;

/// An element name compared loosely: lower case, without underscores.
pub(super) fn loose(name: &str) -> String {
    name.chars().filter(|c| *c != '_').flat_map(char::to_lowercase).collect()
}

/// The words in the child list `list` (compared loosely) of a section: each entry's text, or its
/// `word` attribute. As written, each once whatever its case, in order, and at most `max`.
pub(crate) fn ignored_words(xml: &str, list: &str, max: usize) -> Vec<String> {
    let mut reader = quick_xml::Reader::from_str(xml);
    let mut depth = 0usize;
    let mut in_list = false;
    let mut entry: Option<String> = None;
    let mut words: Vec<String> = Vec::new();
    // What is in `words` already, lowercased, so a long list is read in one pass.
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut push = |word: &str, words: &mut Vec<String>| {
        let word = word.trim();
        if !word.is_empty() && words.len() < max && seen.insert(word.to_lowercase()) {
            words.push(word.to_string());
        }
    };
    while let Ok(event) = reader.read_event() {
        match event {
            Event::Start(start) => {
                depth += 1;
                if depth == 2 && loose(start.local_name().as_ref()) == list {
                    in_list = true;
                } else if depth == 3 && in_list {
                    entry = Some(attr(&start, "word").unwrap_or_default());
                }
            }
            Event::Empty(start) => {
                if depth == 2 && in_list {
                    push(&attr(&start, "word").unwrap_or_default(), &mut words);
                }
            }
            Event::Text(text) if entry.is_some() => {
                if let Some(word) = entry.as_mut() {
                    word.push_str(&text.xml_content(XmlVersion::Implicit1_0));
                }
            }
            Event::GeneralRef(reference) if entry.is_some() => {
                if let (Some(word), Ok(content)) = (entry.as_mut(), reference_text(&reference)) {
                    word.push_str(&content);
                }
            }
            Event::End(_) => {
                if depth == 3
                    && let Some(word) = entry.take()
                {
                    push(&word, &mut words);
                }
                if depth == 2 {
                    in_list = false;
                }
                depth = depth.saturating_sub(1);
            }
            Event::Eof => break,
            _ => {}
        }
    }
    words
}

pub(super) fn ignored_words_xml(words: &[String]) -> String {
    if words.is_empty() {
        return "<IgnoredWords/>".to_string();
    }
    let mut out = String::from("<IgnoredWords>");
    for word in words {
        out.push_str("<Word>");
        escape(word, &mut out);
        out.push_str("</Word>");
    }
    out.push_str("</IgnoredWords>");
    out
}

/// A section with its direct child `child` (compared loosely) replaced by `fresh`, or `fresh`
/// added first when it has none.
pub(crate) fn replace_child(xml: &str, child: &str, fresh: &str) -> String {
    let mut reader = quick_xml::Reader::from_str(xml);
    let mut depth = 0usize;
    let mut open: Option<usize> = None;
    let mut root: Option<(usize, usize, bool)> = None;
    loop {
        let before = usize::try_from(reader.buffer_position()).unwrap_or(usize::MAX);
        let Ok(event) = reader.read_event() else { return xml.to_string() };
        let after = usize::try_from(reader.buffer_position()).unwrap_or(usize::MAX);
        match event {
            Event::Start(start) => {
                depth += 1;
                if depth == 1 {
                    root = Some((before, after, false));
                } else if depth == 2 && loose(start.local_name().as_ref()) == child {
                    open = Some(before);
                }
            }
            Event::End(_) => {
                if depth == 2
                    && let Some(start) = open.take()
                {
                    return format!("{}{fresh}{}", xml.get(..start).unwrap_or_default(), xml.get(after..).unwrap_or_default());
                }
                depth = depth.saturating_sub(1);
            }
            Event::Empty(start) => {
                if depth == 0 {
                    root = Some((before, after, true));
                } else if depth == 1 && loose(start.local_name().as_ref()) == child {
                    return format!("{}{fresh}{}", xml.get(..before).unwrap_or_default(), xml.get(after..).unwrap_or_default());
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    match root {
        Some((_, end, false)) => format!("{}{fresh}{}", xml.get(..end).unwrap_or_default(), xml.get(end..).unwrap_or_default()),
        // A self-closing section opens around the new child.
        Some((start, end, true)) => {
            let tag = xml.get(start..end).unwrap_or_default();
            let name: String = tag.trim_start_matches('<').chars().take_while(|c| !c.is_whitespace() && *c != '/' && *c != '>').collect();
            let open_tag = format!("{}>", tag.trim_end_matches('>').trim_end_matches('/').trim_end());
            format!("{}{open_tag}{fresh}</{name}>{}", xml.get(..start).unwrap_or_default(), xml.get(end..).unwrap_or_default())
        }
        None => xml.to_string(),
    }
}
