//! Revision sets and revised text.

use super::*;

pub(super) fn number(value: Option<&str>) -> Option<u32> {
    value.and_then(|v| v.trim().parse().ok())
}

/// The revision sets of a `Revisions` section.
pub(super) fn read_revisions(xml: &str) -> Revisions {
    let tags = tags(xml);
    let mut revisions = Revisions::default();
    if let Some((_, root)) = tags.first() {
        revisions.active = number(get(root, "ActiveSet"));
        revisions.mode = get(root, "RevisionMode").is_some_and(|m| m.trim().eq_ignore_ascii_case("Yes"));
        revisions.fdx_attributes = root.clone();
    }
    for (_, attributes) in tags.iter().skip(1).filter(|(name, _)| name == "Revision").take(MAX_ELEMENTS) {
        revisions.sets.push(RevisionSet {
            id: number(get(attributes, "ID")).unwrap_or(0),
            name: get(attributes, "Name").unwrap_or_default().to_string(),
            mark: get(attributes, "Mark").unwrap_or_default().to_string(),
            color: get(attributes, "Color").and_then(rgb_from_fdx),
            page_color: get(attributes, "PageColor").and_then(rgb_from_fdx),
            fdx_attributes: attributes.clone(),
        });
    }
    revisions
}

/// `attributes` with `key` set to `value`, keeping a value already there that means the same.
pub(super) fn keep_or_set(attributes: &mut Vec<(String, String)>, key: &str, value: Option<String>, same: impl Fn(&str) -> bool) {
    if get(attributes, key).is_some_and(&same) {
        return;
    }
    set_or_remove(attributes, key, value);
}

/// The `Revision` attributes for a set: the ones it came with, brought up to date.
pub(crate) fn revision_set_attributes(set: &RevisionSet) -> Vec<(String, String)> {
    let mut attributes = set.fdx_attributes.clone();
    keep_or_set(&mut attributes, "Color", set.color.as_deref().and_then(rgb_to_fdx), |c| rgb_from_fdx(c) == set.color);
    keep_or_set(&mut attributes, "ID", Some(set.id.to_string()), |v| number(Some(v)) == Some(set.id));
    keep_or_set(&mut attributes, "Mark", Some(set.mark.clone()), |v| v == set.mark);
    keep_or_set(&mut attributes, "Name", Some(set.name.clone()), |v| v == set.name);
    keep_or_set(&mut attributes, "PageColor", set.page_color.as_deref().and_then(rgb_to_fdx), |c| rgb_from_fdx(c) == set.page_color);
    if set.fdx_attributes.is_empty() {
        attributes.sort();
    }
    attributes
}

/// The `Revisions` element's own attributes, brought up to date.
pub(crate) fn revisions_attributes(revisions: &Revisions) -> Vec<(String, String)> {
    let mut attributes = revisions.fdx_attributes.clone();
    keep_or_set(&mut attributes, "ActiveSet", revisions.active.map(|a| a.to_string()), |v| number(Some(v)) == revisions.active);
    // Written when on, or when the file said it either way.
    let mode = (revisions.mode || get(&attributes, "RevisionMode").is_some()).then(|| (if revisions.mode { "Yes" } else { "No" }).to_string());
    keep_or_set(&mut attributes, "RevisionMode", mode, |v| v.trim().eq_ignore_ascii_case("Yes") == revisions.mode);
    attributes
}

pub(super) fn write_revisions(revisions: &Revisions, out: &mut String) {
    out.push_str("  <Revisions");
    for (key, value) in revisions_attributes(revisions) {
        write_attr(&key, &value, out);
    }
    if revisions.sets.is_empty() {
        out.push_str("/>\n");
        return;
    }
    out.push_str(">\n");
    for set in &revisions.sets {
        out.push_str("    <Revision");
        for (key, value) in revision_set_attributes(set) {
            write_attr(&key, &value, out);
        }
        out.push_str("/>\n");
    }
    out.push_str("  </Revisions>\n");
}
