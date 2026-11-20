//! Bookmarks and the stored autocomplete lists.

use super::*;

/// Bookmarks and the stored autocomplete lists.
pub(super) fn list_commands() -> Vec<Command> {
    use ParamType::*;
    vec![
        Command {
            id: "bookmark.add",
            label: "Add Bookmark",
            menu: "",
            shortcut: None,
            params: const { &[INDEX, param("name", String, true, "The bookmark's name")] },
            enabled: always,
            run: |s, p| {
                let index = element_index(s, p)?;
                let name = string(p, "name")?.trim().to_string();
                if name.is_empty() {
                    return Err(CommandError::Invalid { name: "name", reason: "a bookmark needs a name".into() });
                }
                s.checkpoint(EditKind::Other);
                // A name points to one place: used again, it moves here.
                for element in &mut s.script.elements {
                    if element.bookmark.as_deref().is_some_and(|old| old.eq_ignore_ascii_case(&name)) {
                        element.bookmark = None;
                    }
                }
                if let Some(element) = s.script.elements.get_mut(index) {
                    element.bookmark = Some(name);
                }
                Ok(Outcome::done())
            },
        },
        Command {
            id: "bookmark.remove",
            label: "Remove Bookmark",
            menu: "Edit/Remove Bookmark",
            shortcut: None,
            params: const { &[INDEX] },
            enabled: has_bookmark,
            run: |s, p| {
                let index = element_index(s, p)?;
                if s.script.elements.get(index).is_some_and(|e| e.bookmark.is_some()) {
                    s.checkpoint(EditKind::Other);
                    if let Some(element) = s.script.elements.get_mut(index) {
                        element.bookmark = None;
                    }
                }
                Ok(Outcome::done())
            },
        },
        Command {
            id: "bookmark.list",
            label: "List Bookmarks",
            menu: "",
            shortcut: None,
            params: &[],
            enabled: always,
            run: |s, _| {
                let pages = s.paginated().element_pages.clone();
                let rows: Vec<Value> = s
                    .script
                    .elements
                    .iter()
                    .enumerate()
                    .filter_map(|(i, e)| {
                        e.bookmark.as_ref().map(|name| json!({ "name": name, "element": i, "page": pages.get(i).copied().unwrap_or(0) }))
                    })
                    .collect();
                Ok(Outcome::value(json!(rows)))
            },
        },
        Command {
            id: "lists.get",
            label: "Autocomplete Lists",
            menu: "",
            shortcut: None,
            params: &[],
            enabled: always,
            run: |s, _| Ok(Outcome::value(json!(s.script.lists))),
        },
        Command {
            id: "lists.add",
            label: "Add to an Autocomplete List",
            menu: "",
            shortcut: None,
            params: const { &[LIST_NAME, param("entry", String, true, "The entry, stored in capitals")] },
            enabled: always,
            run: |s, p| {
                let name = list_name(s, p)?;
                let entry = string(p, "entry")?;
                let present = s.script.lists.list(&name).is_some_and(|list| list.iter().any(|e| e.eq_ignore_ascii_case(entry.trim())));
                if entry.trim().is_empty() || present {
                    return Ok(Outcome::value(json!({ "added": false })));
                }
                s.checkpoint(EditKind::Other);
                let added = s.script.lists.list_mut(&name).is_some_and(|list| storyteller_core::autocomplete::Lists::add(list, &entry));
                Ok(Outcome::value(json!({ "added": added })))
            },
        },
        Command {
            id: "lists.remove",
            label: "Remove from an Autocomplete List",
            menu: "",
            shortcut: None,
            params: const { &[LIST_NAME, param("entry", String, true, "The entry")] },
            enabled: always,
            run: |s, p| {
                let name = list_name(s, p)?;
                let entry = string(p, "entry")?;
                let present = s.script.lists.list(&name).is_some_and(|list| list.iter().any(|e| e.eq_ignore_ascii_case(entry.trim())));
                if present {
                    s.checkpoint(EditKind::Other);
                    if let Some(list) = s.script.lists.list_mut(&name) {
                        list.retain(|e| !e.eq_ignore_ascii_case(entry.trim()));
                    }
                }
                Ok(Outcome::value(json!({ "removed": present })))
            },
        },
        Command {
            id: "lists.rebuild",
            label: "Rebuild Autocomplete Lists",
            menu: "",
            shortcut: None,
            params: &[],
            enabled: always,
            run: |s, _| {
                s.checkpoint(EditKind::Other);
                let mut lists = std::mem::take(&mut s.script.lists);
                lists.rebuild(&s.script);
                s.script.lists = lists;
                Ok(Outcome::done())
            },
        },
    ]
}
