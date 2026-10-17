//! Fountain (<https://fountain.io/syntax>) reader and writer, written from the public spec.
//!
//! The writer emits canonical Fountain and forces an element (`.`, `!`, `@`, `>`) only when its
//! plain form would read back as something else, so `parse(write(script)) == script` for any
//! normalized script whose dialogue blocks follow character cues.
//!
//! Known lossy cases: Fountain has no Shot element (shots are written as forced action), and
//! two consecutive dialogue paragraphs of one speech merge into one with a line break.

use crate::model::{Element, ElementKind, Script, TitleField, TitlePage};
use crate::{CoreError, MAX_ELEMENTS, MAX_INPUT_BYTES};

/// Title-page keys that identify a title page.
const KNOWN_TITLE_KEYS: [&str; 13] =
    ["title", "credit", "author", "authors", "source", "draft date", "date", "contact", "copyright", "notes", "revision", "format", "font"];

const SCENE_PREFIXES: [&str; 6] = ["INT./EXT", "INT/EXT", "I/E", "INT", "EXT", "EST"];

/// A line holding exactly two spaces is an intentional empty line inside an element.
fn is_blank(line: &str) -> bool {
    line.trim().is_empty() && line != "  "
}

/// `INT.`, `EXT `, `INT./EXT.`, `I/E` … followed by `.` or a space.
pub fn is_scene_heading(text: &str) -> bool {
    let upper = text.trim_start().to_uppercase();
    SCENE_PREFIXES.iter().any(|prefix| upper.strip_prefix(prefix).is_some_and(|rest| rest.starts_with(['.', ' '])))
}

/// An upper-case line ending in `TO:`.
pub fn is_transition(text: &str) -> bool {
    let text = text.trim();
    text.ends_with("TO:") && !text.chars().any(char::is_lowercase)
}

/// An upper-case name (extensions in brackets may be any case) with at least one letter.
fn is_character_line(text: &str) -> bool {
    let name = crate::model::character_name(text);
    !name.is_empty()
        && name.chars().any(char::is_alphabetic)
        && !name.chars().any(char::is_lowercase)
        && !name.starts_with(['!', '.', '@', '#', '=', '~', '>', '[', '('])
}

fn is_page_break(text: &str) -> bool {
    text.len() >= 3 && text.chars().all(|c| c == '=')
}

fn title_key(line: &str) -> Option<(&str, &str)> {
    if line.starts_with([' ', '\t']) {
        return None;
    }
    let (key, value) = line.split_once(':')?;
    let valid = !key.is_empty()
        && key.len() <= 40
        && key.chars().next().is_some_and(|c| c.is_ascii_alphabetic())
        && key.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, ' ' | '_' | '-'));
    valid.then_some((key.trim(), value.trim()))
}

fn split_scene_number(text: &str) -> (String, Option<String>) {
    let trimmed = text.trim_end();
    if let Some(body) = trimmed.strip_suffix('#')
        && let Some(start) = body.rfind('#')
    {
        let number = body.get(start + 1..).unwrap_or("");
        let valid = !number.is_empty() && number.chars().all(|c| c.is_alphanumeric() || matches!(c, '.' | '-'));
        if valid {
            let heading = body.get(..start).unwrap_or("").trim_end();
            return (heading.to_string(), Some(number.to_string()));
        }
    }
    (trimmed.to_string(), None)
}

fn remove_boneyard(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find("/*") {
        let Some(after) = rest.get(start + 2..) else { break };
        let Some(end) = after.find("*/") else { break };
        out.push_str(rest.get(..start).unwrap_or(""));
        rest = after.get(end + 2..).unwrap_or("");
    }
    out.push_str(rest);
    out
}

struct Builder {
    elements: Vec<Element>,
}

impl Builder {
    fn push(&mut self, element: Element) -> Result<(), CoreError> {
        if self.elements.len() >= MAX_ELEMENTS {
            return Err(CoreError::TooManyElements);
        }
        self.elements.push(element);
        Ok(())
    }
}

/// Parse a Fountain document.
pub fn parse(source: &str) -> Result<Script, CoreError> {
    if source.len() > MAX_INPUT_BYTES {
        return Err(CoreError::TooLarge);
    }
    let source = source.strip_prefix('\u{feff}').unwrap_or(source).replace("\r\n", "\n").replace('\r', "\n");
    let source = remove_boneyard(&source);
    let all_lines: Vec<&str> = source.split('\n').collect();

    let (title_page, body_start) = parse_title_page(&all_lines);
    let lines = all_lines.get(body_start..).unwrap_or(&[]);
    let mut builder = Builder { elements: Vec::new() };
    let count = lines.len();
    let line_at = |index: usize| lines.get(index).copied().unwrap_or("");
    let blank_at = |index: usize| index >= count || is_blank(line_at(index));

    let mut i = 0;
    while i < count {
        let line = line_at(i);
        if is_blank(line) {
            i += 1;
            continue;
        }
        let trimmed = line.trim();
        let prev_blank = i == 0 || blank_at(i - 1);
        let next_blank = blank_at(i + 1);

        if is_page_break(trimmed) {
            builder.push(Element::new(ElementKind::PageBreak, ""))?;
            i += 1;
        } else if trimmed.starts_with('#') {
            let level = trimmed.chars().take_while(|c| *c == '#').count().clamp(1, 6) as u8;
            let mut element = Element::new(ElementKind::Section, trimmed.trim_start_matches('#').trim());
            element.level = level;
            builder.push(element)?;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix('=') {
            builder.push(Element::new(ElementKind::Synopsis, rest.trim()))?;
            i += 1;
        } else if trimmed.starts_with('~') {
            let mut text = Vec::new();
            while i < count && line_at(i).trim().starts_with('~') {
                text.push(line_at(i).trim().strip_prefix('~').unwrap_or("").to_string());
                i += 1;
            }
            builder.push(Element::new(ElementKind::Lyrics, text.join("\n")))?;
        } else if trimmed.starts_with('!') {
            let first = line.trim_start().strip_prefix('!').unwrap_or("");
            i = push_action(&mut builder, lines, i, first)?;
        } else if prev_blank && trimmed.starts_with('.') && !trimmed.starts_with("..") {
            let (heading, number) = split_scene_number(trimmed.get(1..).unwrap_or("").trim_start());
            let mut element = Element::new(ElementKind::SceneHeading, heading);
            element.scene_number = number;
            builder.push(element)?;
            i += 1;
        } else if prev_blank && next_blank && is_scene_heading(trimmed) {
            let (heading, number) = split_scene_number(trimmed);
            let mut element = Element::new(ElementKind::SceneHeading, heading);
            element.scene_number = number;
            builder.push(element)?;
            i += 1;
        } else if trimmed.len() >= 2 && trimmed.starts_with('>') && trimmed.ends_with('<') {
            let mut text = Vec::new();
            while i < count {
                let current = line_at(i).trim();
                if current.len() < 2 || !current.starts_with('>') || !current.ends_with('<') {
                    break;
                }
                text.push(current.get(1..current.len() - 1).unwrap_or("").trim().to_string());
                i += 1;
            }
            builder.push(Element::new(ElementKind::Centered, text.join("\n")))?;
        } else if let Some(rest) = trimmed.strip_prefix('>') {
            builder.push(Element::new(ElementKind::Transition, rest.trim()))?;
            i += 1;
        } else if prev_blank && next_blank && is_transition(trimmed) {
            builder.push(Element::new(ElementKind::Transition, trimmed))?;
            i += 1;
        } else if prev_blank && (trimmed.starts_with('@') || (!next_blank && is_character_line(trimmed))) {
            i = push_dialogue_block(&mut builder, lines, i)?;
        } else {
            i = push_action(&mut builder, lines, i, line)?;
        }
    }
    Ok(Script { title_page, elements: builder.elements, fdx: None, osf: None })
}

fn parse_title_page(lines: &[&str]) -> (TitlePage, usize) {
    let mut page = TitlePage::default();
    let Some((first_key, _)) = lines.first().and_then(|line| title_key(line)) else {
        return (page, 0);
    };
    let block_end = lines.iter().position(|line| line.trim().is_empty()).unwrap_or(lines.len());
    let block = lines.get(..block_end).unwrap_or(&[]);
    let has_known_key = std::iter::once(first_key)
        .chain(block.iter().filter_map(|line| title_key(line).map(|(key, _)| key)))
        .any(|key| KNOWN_TITLE_KEYS.contains(&key.to_ascii_lowercase().as_str()));
    if !has_known_key {
        return (page, 0);
    }
    for line in block {
        match title_key(line) {
            Some((key, value)) => page.fields.push(TitleField { key: key.to_string(), value: value.to_string() }),
            None => {
                if let Some(field) = page.fields.last_mut() {
                    if !field.value.is_empty() {
                        field.value.push('\n');
                    }
                    field.value.push_str(line.trim());
                }
            }
        }
    }
    (page, block_end)
}

fn push_action(builder: &mut Builder, lines: &[&str], start: usize, first: &str) -> Result<usize, CoreError> {
    let mut text = vec![if first == "  " { "" } else { first }];
    let mut i = start + 1;
    while let Some(line) = lines.get(i) {
        if is_blank(line) {
            break;
        }
        text.push(if *line == "  " { "" } else { line });
        i += 1;
    }
    builder.push(Element::new(ElementKind::Action, text.join("\n")))?;
    Ok(i)
}

fn push_dialogue_block(builder: &mut Builder, lines: &[&str], start: usize) -> Result<usize, CoreError> {
    let cue_line = lines.get(start).copied().unwrap_or("").trim();
    let mut cue = cue_line.strip_prefix('@').unwrap_or(cue_line).trim().to_string();
    let mut dual = false;
    if let Some(stripped) = cue.strip_suffix('^') {
        cue = stripped.trim_end().to_string();
        dual = true;
    }
    builder.push(Element::new(ElementKind::Character, cue).with_dual(dual))?;
    let block_start = builder.elements.len();
    let mut i = start + 1;
    while let Some(line) = lines.get(i) {
        if is_blank(line) {
            break;
        }
        let trimmed = line.trim();
        let in_block = builder.elements.len() > block_start;
        let last_kind = builder.elements.last().map(|element| element.kind);
        if trimmed.starts_with('(') {
            builder.push(Element::new(ElementKind::Parenthetical, trimmed))?;
        } else if let Some(lyric) = trimmed.strip_prefix('~') {
            match builder.elements.last_mut() {
                Some(last) if in_block && last_kind == Some(ElementKind::Lyrics) => {
                    last.text.push('\n');
                    last.text.push_str(lyric);
                }
                _ => builder.push(Element::new(ElementKind::Lyrics, lyric))?,
            }
        } else {
            match builder.elements.last_mut() {
                Some(last) if in_block && last_kind == Some(ElementKind::Dialogue) => {
                    last.text.push('\n');
                    last.text.push_str(trimmed);
                }
                _ => builder.push(Element::new(ElementKind::Dialogue, trimmed))?,
            }
        }
        i += 1;
    }
    Ok(i)
}

fn action_needs_force(text: &str, first_in_document: bool) -> bool {
    let mut lines = text.split('\n');
    let first = lines.next().unwrap_or("");
    let multi_line = lines.next().is_some();
    let trimmed = first.trim();
    trimmed.is_empty()
        || trimmed.starts_with(['!', '.', '@', '#', '=', '~', '>'])
        || is_page_break(trimmed)
        || is_scene_heading(trimmed)
        || is_transition(trimmed)
        || (multi_line && is_character_line(trimmed))
        || (first_in_document && title_key(first).is_some())
}

fn push_lines(out: &mut String, text: &str, prefix: &str, suffix: &str) {
    for (index, line) in text.split('\n').enumerate() {
        if index > 0 {
            out.push('\n');
        }
        out.push_str(prefix);
        out.push_str(line);
        out.push_str(suffix);
    }
}

fn push_body_lines(out: &mut String, text: &str) {
    for (index, line) in text.split('\n').enumerate() {
        if index > 0 {
            out.push('\n');
        }
        // An empty line inside an element is written as two spaces.
        out.push_str(if line.trim().is_empty() { "  " } else { line });
    }
}

/// Write a script as Fountain.
pub fn write(script: &Script) -> String {
    let mut out = String::new();
    for field in &script.title_page.fields {
        out.push_str(&field.key);
        out.push(':');
        if field.value.contains('\n') {
            for line in field.value.split('\n') {
                out.push_str("\n    ");
                out.push_str(line.trim());
            }
        } else if !field.value.is_empty() {
            out.push(' ');
            out.push_str(&field.value);
        }
        out.push('\n');
    }
    if !script.title_page.fields.is_empty() {
        out.push('\n');
    }

    let elements = &script.elements;
    let mut in_block = false;
    for (index, element) in elements.iter().enumerate() {
        let previous = index.checked_sub(1).and_then(|at| elements.get(at));
        let next = elements.get(index + 1);
        let continues_block = in_block
            && (element.kind.is_dialogue_part() || (element.kind == ElementKind::Lyrics && previous.map(|p| p.kind) != Some(ElementKind::Lyrics)));
        if index > 0 {
            out.push_str(if continues_block { "\n" } else { "\n\n" });
        }
        in_block = continues_block || element.kind == ElementKind::Character;
        let text = element.text.as_str();
        match element.kind {
            ElementKind::SceneHeading => {
                if !is_scene_heading(text) || text.trim_start().starts_with('.') {
                    out.push('.');
                }
                out.push_str(text);
                if let Some(number) = &element.scene_number {
                    out.push_str(" #");
                    out.push_str(number);
                    out.push('#');
                }
            }
            ElementKind::Action => {
                if action_needs_force(text, index == 0 && script.title_page.fields.is_empty()) {
                    out.push('!');
                }
                push_body_lines(&mut out, text);
            }
            ElementKind::Shot => {
                out.push('!');
                out.push_str(text);
            }
            ElementKind::Character => {
                let followed_by_speech = next.is_some_and(|n| n.kind.is_dialogue_part() || n.kind == ElementKind::Lyrics);
                if !followed_by_speech || !is_character_line(text) || text.trim_start().starts_with('@') {
                    out.push('@');
                }
                out.push_str(text);
                if element.dual {
                    out.push_str(" ^");
                }
            }
            ElementKind::Parenthetical => out.push_str(text),
            ElementKind::Dialogue => {
                let lines: Vec<&str> = text.split('\n').map(str::trim).collect();
                push_body_lines(&mut out, &lines.join("\n"));
            }
            ElementKind::Transition => {
                if !is_transition(text) {
                    out.push('>');
                }
                out.push_str(text);
            }
            ElementKind::Centered => push_lines(&mut out, text, ">", "<"),
            ElementKind::Lyrics => push_lines(&mut out, text, "~", ""),
            ElementKind::Section => {
                out.push_str(&"#".repeat(usize::from(element.level.clamp(1, 6))));
                out.push(' ');
                out.push_str(text);
            }
            ElementKind::Synopsis => {
                out.push_str("= ");
                out.push_str(text);
            }
            ElementKind::PageBreak => out.push_str("==="),
        }
    }
    out.push('\n');
    out
}
