//! Edit operations the commands share: text, types, inserts, paste, Enter and Tab.

use super::*;

/// Replace an element's text, applying the kind's capitalisation.
pub fn set_text(session: &mut Session, index: usize, text: &str, typing: bool) -> Result<(), CommandError> {
    let Some(element) = session.script.elements.get(index) else {
        return Err(CommandError::Invalid { name: "index", reason: "no such element".into() });
    };
    let mut text = if element.kind.is_single_line() { text.replace(['\n', '\r'], " ") } else { text.replace('\r', "") };
    // Capitals for the kinds that always take them. A type the script's settings print in
    // capitals (a multi-camera script's action) keeps the writer's casing: the editor and the
    // page show it in capitals.
    if element.kind.is_caps() {
        text = text.to_uppercase();
    }
    if element.text == text {
        return Ok(());
    }
    // Typing "int." or "ext." at the start of action makes it a scene heading, as in Final Draft,
    // however many keys arrive at once.
    let becomes_heading = typing && element.kind == ElementKind::Action && flow::becomes_heading(&element.text, &text);
    session.checkpoint(if typing { EditKind::Typing(index) } else { EditKind::Other });
    let mark = session.marking_set();
    if let Some(element) = session.script.elements.get_mut(index) {
        element.set_text_revised(text, mark);
    }
    if becomes_heading {
        // Its own undo step, so one Undo turns it back into the action just typed.
        session.checkpoint(EditKind::Other);
        if let Some(element) = session.script.elements.get_mut(index) {
            element.kind = ElementKind::SceneHeading;
            element.normalize();
        }
    }
    Ok(())
}

pub(super) fn set_kind(session: &mut Session, index: usize, to: ElementKind) -> Result<(), CommandError> {
    let Some(element) = session.script.elements.get(index) else {
        return Err(CommandError::Invalid { name: "index", reason: "no such element".into() });
    };
    let from = element.kind;
    if from == to {
        return Ok(());
    }
    let (text, shift) = flow::convert_text(&element.text, from, to);
    session.checkpoint(EditKind::Other);
    let mark = session.marking_set();
    if let Some(element) = session.script.elements.get_mut(index) {
        element.kind = to;
        element.set_text_revised(text, mark);
        element.normalize();
    }
    if session.caret.element == index {
        session.caret.offset = session.caret.offset.saturating_add_signed(shift);
    }
    Ok(())
}

pub(super) fn insert(session: &mut Session, at: usize, element: Element) -> Result<(), CommandError> {
    if session.script.elements.len() >= storyteller_core::MAX_ELEMENTS {
        return Err(CommandError::Failed("the script has too many elements".into()));
    }
    let at = at.min(session.script.elements.len());
    session.script.elements.insert(at, element);
    Ok(())
}

/// The elements of the scene `index` is in: from its heading (or the start of the script) up to
/// the next heading.
pub fn scene_range(script: &Script, index: usize) -> std::ops::Range<usize> {
    let elements = &script.elements;
    let index = index.min(elements.len().saturating_sub(1));
    let start = elements.get(..=index).unwrap_or_default().iter().rposition(|e| e.kind == ElementKind::SceneHeading).unwrap_or(0);
    let end = elements
        .get(start + 1..)
        .unwrap_or_default()
        .iter()
        .position(|e| e.kind == ElementKind::SceneHeading)
        .map_or(elements.len(), |n| start + 1 + n);
    start..end
}

/// Mark Revised (`mark`) or Clear Revised: the printed text between two byte offsets of an
/// element's source, or all of it.
pub(super) fn mark_range(s: &mut Session, p: &Params, mark: bool) -> Result<Outcome, CommandError> {
    let index = element_index(s, p)?;
    let set = if mark {
        Some(
            s.script
                .revisions
                .active
                .ok_or_else(|| CommandError::Invalid { name: "index", reason: "add a revision set first (Production > Revisions…)".into() })?,
        )
    } else {
        None
    };
    let text = s.script.elements.get(index).map(|e| e.text.clone()).unwrap_or_default();
    let (start, end) = (opt_index(p, "start")?.unwrap_or(0), opt_index(p, "end")?.unwrap_or(text.len()));
    let printed = |byte: usize| u32::try_from(inline::printed_before(&text, byte.min(text.len()))).unwrap_or(u32::MAX);
    let (from, to) = (printed(start.min(end)), printed(end.max(start)));
    let mut changed = s.script.clone();
    if let Some(element) = changed.elements.get_mut(index) {
        element.mark_range(from, to, set);
    }
    if changed.elements.get(index) == s.script.elements.get(index) {
        return Ok(Outcome::done());
    }
    s.checkpoint(EditKind::Other);
    s.script = changed;
    Ok(Outcome::done())
}

/// A revision set's id from the `id` parameter.
pub(super) fn set_id(params: &Params) -> Result<u32, CommandError> {
    let id = opt_index(params, "id")?.ok_or(CommandError::Missing("id"))?;
    u32::try_from(id).map_err(|_| CommandError::Invalid { name: "id", reason: "there is no such revision set".into() })
}

/// Today's date as a revision set's name carries it: `10/21/26`.
pub(super) fn today_short() -> std::string::String {
    let (year, month, day) = today();
    format!("{month:02}/{day:02}/{:02}", year.rem_euclid(100))
}

/// Today's date as a file carries it: `2026-10-21`.
pub(super) fn today_iso() -> std::string::String {
    let (year, month, day) = today();
    format!("{year:04}-{month:02}-{day:02}")
}

/// Today's date where the writer is, or the UTC date when the system cannot say, so there is
/// always a date.
pub(super) fn today() -> (i64, u32, u32) {
    match storyteller_platform::local_date() {
        Some((year, month, day)) => (i64::from(year), u32::from(month), u32::from(day)),
        None => utc_today(),
    }
}

/// Now, in UTC, as RFC 3339 to the second: `2026-11-05T09:30:00Z`.
pub fn now_rfc3339() -> std::string::String {
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
    let (year, month, day) = civil_from_days(i64::try_from(secs / 86_400).unwrap_or(0));
    let time = secs % 86_400;
    format!("{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z", time / 3600, time % 3600 / 60, time % 60)
}

/// Today's date in UTC.
pub(super) fn utc_today() -> (i64, u32, u32) {
    let days = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs() / 86_400);
    civil_from_days(i64::try_from(days).unwrap_or(0))
}

/// The calendar date `days` after 1970-01-01 (Howard Hinnant's algorithm).
pub(super) fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = u32::try_from(doy - (153 * mp + 2) / 5 + 1).unwrap_or(1);
    let month = u32::try_from(if mp < 10 { mp + 3 } else { mp - 9 }).unwrap_or(1);
    (yoe + era * 400 + i64::from(month <= 2), month, day)
}

#[cfg(test)]
mod date_tests {
    #[test]
    fn civil_dates() {
        assert_eq!(super::civil_from_days(0), (1970, 1, 1));
        assert_eq!(super::civil_from_days(20_747), (2026, 10, 21));
        assert_eq!(super::civil_from_days(11_016), (2000, 2, 29));
    }

    #[test]
    fn the_local_date_is_within_a_day_of_utc() {
        let local = storyteller_platform::local_date().expect("the system gives a local date");
        let local = (i64::from(local.0), u32::from(local.1), u32::from(local.2));
        let days = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs() / 86_400);
        let days = i64::try_from(days).unwrap_or(0);
        let near: Vec<_> = (days - 1..=days + 1).map(super::civil_from_days).collect();
        assert!(near.contains(&local), "{local:?} is not within a day of {near:?}");
        assert!((1..=12).contains(&local.1) && (1..=31).contains(&local.2));
        assert_eq!(super::today(), local);
    }
}

/// The heading of the scene element `index` is in.
pub(super) fn heading_of(session: &Session, index: usize) -> Result<usize, CommandError> {
    session
        .script
        .elements
        .iter()
        .take(index + 1)
        .rposition(|e| e.kind == ElementKind::SceneHeading)
        .ok_or_else(|| CommandError::Invalid { name: "index", reason: "that is not in a scene".into() })
}

/// Whether the caret is in a scene that is not omitted.
pub(super) fn in_live_scene(session: &Session) -> bool {
    heading_of(session, session.caret.element).ok().and_then(|h| session.script.elements.get(h)).is_some_and(|e| !e.is_omitted())
}

/// Whether pasted text is screenplay text in several paragraphs (a blank line between two), so
/// that pasting it makes elements rather than one long paragraph.
pub fn is_screenplay_paste(text: &str) -> bool {
    text.replace("\r\n", "\n").trim().split("\n\n").filter(|part| !part.trim().is_empty()).nth(1).is_some()
}

/// The elements that move with the scene headed at `heading`: up to the next heading, or to the
/// act lines that close its act, which stay where they are.
pub fn scene_body(script: &Script, heading: usize) -> std::ops::Range<usize> {
    let rest = script.elements.iter().enumerate().skip(heading + 1);
    let end = rest
        .filter(|(_, e)| matches!(e.kind, ElementKind::SceneHeading | ElementKind::NewAct | ElementKind::EndOfAct))
        .map(|(i, _)| i)
        .next()
        .unwrap_or(script.elements.len());
    heading..end
}

/// Moves the scene headed at `heading` so it starts where element `to` is now, as one Undo step.
/// Returns whether anything moved. In revision mode the moved paragraphs are marked whole. A
/// locked page that started in them stays behind, on the paragraph after, so the pages around
/// them keep their numbers.
pub(super) fn move_scene(s: &mut Session, heading: usize, to: usize) -> Result<bool, CommandError> {
    if s.script.elements.get(heading).is_none_or(|e| e.kind != ElementKind::SceneHeading) {
        return Err(CommandError::Invalid { name: "index", reason: "not a scene heading".into() });
    }
    if to > s.script.elements.len() {
        return Err(CommandError::Invalid { name: "to", reason: format!("there are {} elements", s.script.elements.len()) });
    }
    let body = scene_body(&s.script, heading);
    if body.contains(&to) || to == body.end {
        return Ok(false);
    }
    s.checkpoint(EditKind::Other);
    let mark = s.marking_set();
    let at = if to > body.end { to - body.len() } else { to };
    let mut moved: Vec<Element> = s.script.elements.drain(body.clone()).collect();
    let mut starts = Vec::new();
    for element in &mut moved {
        for mut start in std::mem::take(&mut element.page_starts) {
            start.at = 0;
            starts.push(start);
        }
        element.mark_whole(mark);
    }
    if let Some(next) = s.script.elements.get_mut(body.start) {
        starts.append(&mut next.page_starts);
        next.page_starts = starts;
    } else if let Some(last) = s.script.elements.last_mut() {
        last.page_starts.append(&mut starts);
    }
    for (n, element) in moved.into_iter().enumerate() {
        s.script.elements.insert(at + n, element);
    }
    s.caret = Caret { element: at, offset: 0 };
    Ok(true)
}

/// Pastes screenplay text at `start..end` of element `index`, read as Fountain, as one Undo
/// step: the text before the paste stays in the element, the pasted elements follow, and the
/// text after the paste becomes an element of its own. Returns how many elements were pasted.
pub(super) fn paste_screenplay(s: &mut Session, index: usize, start: usize, end: usize, text: &str) -> Result<usize, CommandError> {
    if index >= s.script.elements.len() {
        return Err(CommandError::Invalid { name: "index", reason: "no such element".into() });
    }
    let pasted = pasted_elements(s, text)?;
    if pasted.is_empty() {
        return Ok(0);
    }
    s.checkpoint(EditKind::Other);
    Ok(place_pasted(s, index, start, end, pasted))
}

/// The elements a paste of `text` makes: the very elements of the last range copied here when it
/// is that text, so their types survive even where Fountain cannot say them (dialogue with no
/// cue); otherwise `text` read as Fountain.
pub(super) fn pasted_elements(s: &Session, text: &str) -> Result<Vec<Element>, CommandError> {
    let pasted = match s.copied_elements(text) {
        Some(elements) => elements.to_vec(),
        // The text alone, never a title page: a pasted `Title: …` line is action.
        None => storyteller_core::fountain::parse(&format!("\n{}", text.replace("\r\n", "\n").trim()))
            .map_err(|e| CommandError::Failed(format!("the pasted text could not be read: {e}")))?
            .elements
            .into_iter()
            .filter(|e| e.kind != ElementKind::PageBreak || !e.text.is_empty())
            .collect(),
    };
    if s.script.elements.len().saturating_add(pasted.len()) > storyteller_core::MAX_ELEMENTS {
        return Err(CommandError::Failed("the script has too many elements".into()));
    }
    Ok(pasted)
}

/// Puts `pasted` (not empty) in at `start..end` of element `index`, inside the caller's Undo step.
pub(super) fn place_pasted(s: &mut Session, index: usize, start: usize, end: usize, mut pasted: Vec<Element>) -> usize {
    let Some(element) = s.script.elements.get(index) else { return 0 };
    let start = floor_char_boundary(&element.text, start);
    let end = floor_char_boundary(&element.text, end.max(start));
    let kind = element.kind;
    let before = element.text.get(..start).unwrap_or_default().to_string();
    let after = element.text.get(end..).unwrap_or_default().to_string();
    let count = pasted.len();
    let mut at = index;
    if before.trim().is_empty() {
        // An empty line (or a paste at its start) takes the first pasted element in its place.
        s.script.elements.remove(index);
    } else {
        if let Some(element) = s.script.elements.get_mut(index) {
            element.set_text(before.trim_end().to_string());
        }
        at = index + 1;
    }
    if !after.trim().is_empty() {
        let mut rest = Element::new(kind, after.trim_start());
        rest.normalize();
        pasted.push(rest);
    }
    // In revision mode every paragraph the paste makes is new, so each is revised whole.
    let mark = s.marking_set();
    for element in &mut pasted {
        element.mark_whole(mark);
    }
    let last = at + count - 1;
    let caret = pasted.get(count - 1).map_or(0, |e| e.text.len());
    for (n, element) in pasted.into_iter().enumerate() {
        s.script.elements.insert(at + n, element);
    }
    s.caret = Caret { element: last, offset: caret };
    count
}

/// The range a command names: `from_index`/`from_offset` to `to_index`/`to_offset`, in script
/// order whichever way it was selected, and on character boundaries.
pub(super) fn text_range(s: &Session, p: &Params) -> Result<(Caret, Caret), CommandError> {
    let place = |index: &'static str, offset: &'static str| -> Result<Caret, CommandError> {
        let element = opt_index(p, index)?.ok_or_else(|| CommandError::Invalid { name: index, reason: "required".into() })?;
        let Some(found) = s.script.elements.get(element) else {
            return Err(CommandError::Invalid { name: index, reason: format!("there are {} elements", s.script.elements.len()) });
        };
        let at = opt_index(p, offset)?.ok_or_else(|| CommandError::Invalid { name: offset, reason: "required".into() })?;
        Ok(Caret { element, offset: floor_char_boundary(&found.text, at) })
    };
    let (a, b) = (place("from_index", "from_offset")?, place("to_index", "to_offset")?);
    Ok(if (a.element, a.offset) <= (b.element, b.offset) { (a, b) } else { (b, a) })
}

/// The elements of a range, the first and last cut to the part inside it. A first or last one
/// with nothing inside the range is left out.
pub(super) fn range_elements(s: &Session, from: Caret, to: Caret) -> Vec<Element> {
    let mut out = Vec::new();
    for index in from.element..=to.element {
        let Some(element) = s.script.elements.get(index) else { continue };
        let start = if index == from.element { from.offset } else { 0 };
        let end = if index == to.element { to.offset } else { element.text.len() };
        // A page break has no text: it is in the range when the range starts on it.
        let nothing_inside =
            if index == from.element { start >= end && element.kind != ElementKind::PageBreak } else { index == to.element && start >= end };
        if nothing_inside {
            continue;
        }
        let mut part = element.clone();
        part.set_text(element.text.get(start..end).unwrap_or_default().to_string());
        // Where the page starts and what was revised belong to this draft, not to the copy.
        part.page_starts.clear();
        part.bookmark = None;
        part.revised.clear();
        part.deletions.clear();
        out.push(part);
    }
    out
}

/// Replaces a range, across paragraphs or within one, with `text`, as one Undo step. The first
/// paragraph keeps its type and takes what follows the range in the last one; pasted screenplay
/// text becomes elements there. In revision mode the join leaves a deletion mark, as Backspace
/// at a paragraph's start does.
pub(super) fn replace_range(s: &mut Session, from: Caret, to: Caret, text: &str) -> Result<(), CommandError> {
    let as_elements = !text.is_empty() && s.pastes_as_elements(text);
    let pasted = if as_elements { pasted_elements(s, text)? } else { Vec::new() };
    let Some(first) = s.script.elements.get(from.element) else {
        return Err(CommandError::Invalid { name: "from_index", reason: "no such element".into() });
    };
    let Some(last) = s.script.elements.get(to.element) else {
        return Err(CommandError::Invalid { name: "to_index", reason: "no such element".into() });
    };
    let kind = match (first.kind, last.kind) {
        (ElementKind::PageBreak, ElementKind::PageBreak) => ElementKind::Action,
        (ElementKind::PageBreak, other) | (other, _) => other,
    };
    let before = first.text.get(..from.offset).unwrap_or_default().to_string();
    let after = last.text.get(to.offset..).unwrap_or_default().to_string();
    // What comes up from the last paragraph takes the first one's capitals.
    let caps = kind.is_caps();
    let after = if caps { after.to_uppercase() } else { after };
    s.checkpoint(EditKind::Other);
    let mark = s.marking_set().filter(|set| *set != 0);
    let mut tail = (from.element != to.element).then(|| s.script.elements.get(to.element).cloned()).flatten();
    if let Some(tail) = tail.as_mut() {
        tail.set_text(after.clone());
    }
    // Within one paragraph, what is anchored after the cut is lifted out the same way, so it
    // comes back after the join rather than being cut off with the text. A note that runs across
    // the cut stays, and how far it ran past the cut is kept for after the join.
    let mut runs_past: Vec<(usize, u32)> = Vec::new();
    // Text put in at a caret takes the revision mark of the character after it, as typing does.
    let mut carried = None;
    if from.element == to.element
        && let Some(element) = s.script.elements.get_mut(from.element)
    {
        let cut = u32::try_from(inline::plain(element.text.get(..to.offset).unwrap_or_default()).chars().count()).unwrap_or(u32::MAX);
        carried = (from == to).then(|| element.revision_at(cut)).flatten();
        let mut moved = Element::new(kind, "");
        element.split_anchors(cut, &mut moved);
        runs_past = element.notes.iter().enumerate().filter_map(|(n, note)| note.range.filter(|r| r.1 > cut).map(|r| (n, r.1 - cut))).collect();
        tail = Some(moved);
    }
    if from.element < to.element {
        s.script.elements.drain(from.element + 1..=to.element);
    }
    // Plain text goes in before the join, so what was anchored after the range comes after it.
    let text = if as_elements || text.is_empty() {
        String::new()
    } else {
        let text = if caps { text.to_uppercase() } else { text.replace('\r', "") };
        if kind.is_single_line() { text.replace('\n', " ") } else { text }
    };
    let marking = s.marking_set();
    let Some(element) = s.script.elements.get_mut(from.element) else { return Ok(()) };
    element.kind = kind;
    // Set in steps, so the first paragraph's anchors stop at the cut and the last one's come
    // along after it (and after any new text).
    element.set_text(before.clone());
    let cut_at = u32::try_from(inline::plain(&before).chars().count()).unwrap_or(u32::MAX);
    let inserted = format!("{before}{text}");
    if !text.is_empty() {
        element.set_text_revised(inserted.clone(), marking);
    }
    let joined_at = u32::try_from(inline::plain(&inserted).chars().count()).unwrap_or(u32::MAX);
    element.set_text(format!("{inserted}{after}"));
    for (note, past) in runs_past {
        if let Some(range) = element.notes.get_mut(note).and_then(|note| note.range.as_mut()) {
            range.1 = joined_at.saturating_add(past);
        }
    }
    if let Some(tail) = tail.as_mut() {
        element.absorb_anchors(tail, joined_at);
    }
    // In revision mode the new text is already marked in the active set.
    if let Some(set) = carried.filter(|_| !text.is_empty() && marking.is_none_or(|m| m == 0)) {
        element.mark_range(cut_at, joined_at, Some(set));
    }
    if let Some(set) = mark.filter(|_| from != to) {
        element.mark_deletion(cut_at, set);
    }
    element.normalize();
    s.caret = Caret { element: from.element, offset: inserted.len() };
    if as_elements && !pasted.is_empty() {
        place_pasted(s, from.element, before.len(), before.len(), pasted);
    }
    Ok(())
}

pub(super) fn new_element(kind: ElementKind) -> (Element, usize) {
    if kind == ElementKind::Parenthetical { (Element::new(kind, "()"), 1) } else { (Element::new(kind, ""), 0) }
}

/// What Enter did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Entered {
    /// Ended the element and started the next one.
    Next,
    /// The element is empty: the app offers the element picker (Final Draft's behaviour).
    Picker,
}

/// Enter at the caret: Final Draft element flow.
pub fn enter(session: &mut Session, index: usize, offset: usize) -> Result<Entered, CommandError> {
    let Some(element) = session.script.elements.get(index).cloned() else {
        return Err(CommandError::Invalid { name: "index", reason: "no such element".into() });
    };
    let rule = flow::rule_for(element.kind);
    if flow::is_empty(element.kind, &element.text) {
        return Ok(Entered::Picker);
    }
    session.checkpoint(EditKind::Other);
    let offset = floor_char_boundary(&element.text, offset);
    if offset == 0 {
        // At the start of a paragraph: open an empty one of the same type above it.
        insert(session, index, Element::new(element.kind, ""))?;
        session.caret = Caret { element: index + 1, offset: 0 };
        return Ok(Entered::Next);
    }
    let mut head = element.text.get(..offset).unwrap_or("").to_string();
    let mut tail = element.text.get(offset..).unwrap_or("").trim_start().to_string();
    // A caret just inside a parenthetical's closing bracket ends the parenthetical.
    if element.kind == ElementKind::Parenthetical && tail.trim() == ")" {
        head.push(')');
        tail.clear();
    }
    let next_kind = rule.enter;
    let (mut fresh, mut caret) = new_element(next_kind);
    if let Some(current) = session.script.elements.get_mut(index) {
        // Revised text and notes after the caret go with the text to the new paragraph.
        // The new paragraph starts after the spaces Enter drops.
        let dropped = element.text.get(offset..).unwrap_or("").chars().take_while(|c| c.is_whitespace()).count();
        let at = u32::try_from(inline::plain(element.text.get(..offset).unwrap_or("")).chars().count() + dropped).unwrap_or(u32::MAX);
        current.split_anchors(at, &mut fresh);
        current.set_text(head.trim_end().to_string());
    }
    if !tail.is_empty() {
        let (text, _) = flow::convert_text(&tail, element.kind, next_kind);
        fresh.text = text;
        caret = usize::from(next_kind == ElementKind::Parenthetical);
        // In revision mode the paragraph Enter makes is new, so it is revised whole.
        fresh.mark_whole(session.marking_set());
    }
    insert(session, index + 1, fresh)?;
    session.caret = Caret { element: index + 1, offset: caret };
    Ok(Entered::Next)
}

/// Start an element of `kind` right after `index`, with the caret in it.
pub(super) fn start_after(session: &mut Session, index: usize, kind: ElementKind) -> Result<(), CommandError> {
    session.checkpoint(EditKind::Other);
    let (fresh, caret) = new_element(kind);
    insert(session, index + 1, fresh)?;
    session.caret = Caret { element: index + 1, offset: caret };
    Ok(())
}

/// Tab: an empty element changes type; after a non-empty one the next element starts
/// (a parenthetical after a cue or a speech, a cue after action).
pub fn tab(session: &mut Session, index: usize) -> Result<(), CommandError> {
    let Some(element) = session.script.elements.get(index) else {
        return Err(CommandError::Invalid { name: "index", reason: "no such element".into() });
    };
    let rule = flow::rule_for(element.kind);
    if flow::is_empty(element.kind, &element.text) {
        set_kind(session, index, rule.tab_empty)?;
        let (fresh, caret) = new_element(rule.tab_empty);
        if let Some(element) = session.script.elements.get_mut(index) {
            element.set_text(fresh.text);
        }
        session.caret = Caret { element: index, offset: caret };
        return Ok(());
    }
    // Tab after a name can open the extension list instead (decision 3).
    if session.tab_opens_extensions
        && element.kind == ElementKind::Character
        && !element.text.contains('(')
        && let Some(text) = session.script.elements.get(index).map(|e| format!("{} (", e.text.trim_end()))
    {
        set_text(session, index, &text, false)?;
        session.caret = Caret { element: index, offset: text.len() };
        return Ok(());
    }
    start_after(session, index, rule.tab_next)
}

/// ⌘1–⌘7: an empty element becomes that type; otherwise an element of that type starts after it.
pub fn element_shortcut(session: &mut Session, index: usize, kind: ElementKind) -> Result<(), CommandError> {
    let Some(element) = session.script.elements.get(index) else {
        return Err(CommandError::Invalid { name: "index", reason: "no such element".into() });
    };
    if flow::is_empty(element.kind, &element.text) {
        set_kind(session, index, kind)?;
        if kind == ElementKind::Parenthetical && session.element(index).is_some_and(|e| e.text.trim().is_empty()) {
            if let Some(element) = session.script.elements.get_mut(index) {
                element.set_text("()");
            }
            session.caret = Caret { element: index, offset: 1 };
        }
        return Ok(());
    }
    start_after(session, index, kind)
}

/// Backspace at the start of an element: merge it into the previous one.
pub fn merge_previous(session: &mut Session, index: usize) -> Result<bool, CommandError> {
    if index == 0 || index >= session.script.elements.len() {
        return Ok(false);
    }
    session.checkpoint(EditKind::Other);
    // In revision mode the join is marked: a deletion mark where the paragraph break was.
    let mark = session.marking_set().filter(|s| *s != 0);
    let mut current = session.script.elements.remove(index);
    let previous_index = index - 1;
    let Some(previous) = session.script.elements.get_mut(previous_index) else { return Ok(false) };
    if previous.kind == ElementKind::PageBreak {
        session.script.elements.remove(previous_index);
        if let Some(set) = mark {
            current.mark_deletion(0, set);
        }
        session.script.elements.insert(previous_index, current);
        session.caret = Caret { element: previous_index, offset: 0 };
        return Ok(true);
    }
    let mut extra = std::mem::take(&mut current.text);
    if previous.kind == ElementKind::Parenthetical || current.kind == ElementKind::Parenthetical {
        extra = extra.trim_matches(['(', ')']).to_string();
    }
    let join_at = previous.text.trim_end_matches(')').len().min(previous.text.len());
    let at = if previous.kind == ElementKind::Parenthetical { join_at } else { previous.text.len() };
    let mut text = previous.text.clone();
    if previous.kind.is_caps() {
        extra = extra.to_uppercase();
    }
    let at = floor_char_boundary(&text, at);
    text.insert_str(at, &extra);
    previous.set_text(if previous.kind.is_single_line() { text.replace('\n', " ") } else { text });
    // The joined paragraph's revised text and notes come along, where its text now sits.
    let joined_at = u32::try_from(inline::plain(previous.text.get(..at).unwrap_or_default()).chars().count()).unwrap_or(u32::MAX);
    previous.absorb_anchors(&mut current, joined_at);
    if let Some(set) = mark {
        previous.mark_deletion(joined_at, set);
    }
    session.caret = Caret { element: previous_index, offset: at };
    Ok(true)
}
