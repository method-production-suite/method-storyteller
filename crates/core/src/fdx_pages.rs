//! Deletion marks and locked pages, placed by position in the script.

use super::*;

/// Every page start with its `Position` in the script, in order.
/// Every deletion mark with its place in the whole script, in order.
pub(super) fn deletions(elements: &[Element]) -> Vec<(u32, Deletion)> {
    let mut out: Vec<(u32, Deletion)> = elements
        .iter()
        .zip(text_starts(elements))
        .filter_map(|(element, start)| start.map(|start| element.deletions.iter().map(move |d| (start.saturating_add(d.at), *d))))
        .flatten()
        .collect();
    out.sort_by_key(|(offset, d)| (*offset, d.set));
    out
}

pub(super) fn write_deletions(root: &[(String, String)], marks: &[(u32, Deletion)], out: &mut String) {
    out.push_str("  <DeletedText");
    for (key, value) in root {
        write_attr(key, value, out);
    }
    if marks.is_empty() {
        out.push_str("/>\n");
        return;
    }
    out.push_str(">\n");
    for (offset, deletion) in marks {
        out.push_str(&format!("    <DeletedTextLocation Offset=\"{offset}\" RevisionID=\"{}\"/>\n", deletion.set));
    }
    out.push_str("  </DeletedText>\n");
}

pub(super) fn locked_pages(elements: &[Element]) -> Vec<(u32, &PageStart)> {
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
        // A page locked here says its lock level when it has one.
        if page.level > 0 {
            attributes.push(("LockLevel".into(), page.level.to_string()));
        }
        attributes.sort();
    }
    attributes
}

/// The `LockedPages` attribute that says this app placed every page (see [`crate::paginate::paginate`]).
pub(super) const LOCKS_PLACED_HERE: &str = "StorytellerPositions";

pub(super) fn write_locked_pages(root: &[(String, String)], pages: &[(u32, &PageStart)], out: &mut String) {
    out.push_str("  <LockedPages");
    let mut root = root.to_vec();
    let placed_here = !pages.is_empty() && pages.iter().all(|(_, page)| page.fdx_attributes.is_empty());
    set_or_remove(&mut root, LOCKS_PLACED_HERE, placed_here.then(|| "Yes".to_string()));
    for (key, value) in &root {
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
