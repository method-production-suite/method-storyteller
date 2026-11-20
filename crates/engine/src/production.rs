//! Production: scene moves and numbers, revisions, locked pages and omitted scenes.

use super::*;

/// Production: scene moves and numbers, revisions, locked pages and omitted scenes.
pub(super) fn production_commands() -> Vec<Command> {
    use ParamType::*;
    vec![
        Command {
            id: "scene.move",
            label: "Move Scene",
            menu: "",
            shortcut: None,
            params: const {
                &[
                    param("index", Integer, true, "The scene's heading"),
                    param("to", Integer, true, "The element it should start before (one past the last: the end of the script)"),
                ]
            },
            enabled: always,
            run: |s, p| {
                let heading = opt_index(p, "index")?.ok_or(CommandError::Invalid { name: "index", reason: "required".into() })?;
                let to = opt_index(p, "to")?.ok_or(CommandError::Invalid { name: "to", reason: "required".into() })?;
                let moved = move_scene(s, heading, to)?;
                Ok(Outcome::value(json!({ "moved": moved })))
            },
        },
        Command {
            id: "scene.number",
            label: "Number Scenes…",
            menu: "Production/Number Scenes…",
            shortcut: None,
            params: const {
                &[
                    param("keep_existing", Bool, false, "Keep numbers that are already set, and letter new scenes (12A)"),
                    param("start", Integer, false, "The first number (default 1)"),
                    param("from_caret", Bool, false, "Number only from the caret's scene on"),
                    param("skip_io", Bool, false, "Leave out the letters I and O"),
                ]
            },
            enabled: always,
            run: |s, p| {
                let start = opt_index(p, "start")?.map_or(1, |n| u32::try_from(n).unwrap_or(u32::MAX));
                let from = if opt_bool(p, "from_caret")?.unwrap_or(false) {
                    // The caret's scene: its heading, or the first heading after it.
                    s.script.elements.iter().take(s.caret.element + 1).rposition(|e| e.kind == ElementKind::SceneHeading).unwrap_or(s.caret.element)
                } else {
                    0
                };
                let options = storyteller_core::model::Numbering {
                    start,
                    keep_existing: opt_bool(p, "keep_existing")?.unwrap_or(false),
                    from,
                    skip_io: opt_bool(p, "skip_io")?.unwrap_or(false),
                };
                let mut numbered = s.script.clone();
                let result = numbered.number_with(options);
                // Scenes with no number between their neighbours, for the writer to set by hand.
                let unplaced: Vec<std::string::String> = result
                    .unplaced
                    .iter()
                    .map(|u| {
                        let position = s.script.scene_indices().iter().position(|&h| h == u.heading).map_or(0, |p| p + 1);
                        format!(
                            "There's no number between {} and {} for scene {position}; set it by hand (Production > Edit Scene Number…).",
                            u.before, u.after
                        )
                    })
                    .collect();
                if result.changed == 0 {
                    let message = if unplaced.is_empty() { "Every scene already has that number".to_string() } else { unplaced.join(" ") };
                    return Ok(Outcome::message(message));
                }
                s.checkpoint(EditKind::Other);
                s.script = numbered;
                let changed = result.changed;
                let done = if changed == 1 { "Numbered 1 scene.".to_string() } else { format!("Numbered {changed} scenes.") };
                Ok(Outcome::message(if unplaced.is_empty() {
                    done.trim_end_matches('.').to_string()
                } else {
                    format!("{done} {}", unplaced.join(" "))
                }))
            },
        },
        Command {
            id: "revisions.list",
            label: "Revision Sets",
            menu: "",
            shortcut: None,
            params: &[],
            enabled: always,
            run: |s, _| {
                let sets: Vec<Value> = s
                    .script
                    .revisions
                    .sets
                    .iter()
                    .map(|set| {
                        json!({
                            "id": set.id, "name": set.name, "mark": set.mark, "page_color": set.page_color,
                            "active": s.script.revisions.active == Some(set.id), "marks": s.script.revision_marks(set.id),
                        })
                    })
                    .collect();
                Ok(Outcome::value(json!(sets)))
            },
        },
        Command {
            id: "revisions.add",
            label: "Next Revision Set",
            menu: "",
            shortcut: None,
            params: const { &[param("date", String, false, "The date in its name (default today, as 10/21/26)")] },
            enabled: always,
            run: |s, p| {
                let date = opt_string(p, "date")?.unwrap_or_else(today_short);
                s.checkpoint(EditKind::Other);
                let id = s.script.add_revision_set(&date);
                Ok(Outcome::value(json!({ "id": id })))
            },
        },
        Command {
            id: "revisions.update",
            label: "Change Revision Set",
            menu: "",
            shortcut: None,
            params: const {
                &[
                    param("id", Integer, true, "The set"),
                    param("name", String, false, "Its name"),
                    param("mark", String, false, "Its margin mark, one or two characters"),
                    param("page_color", String, false, "Its page colour, #RRGGBB (empty for none)"),
                ]
            },
            enabled: always,
            run: |s, p| {
                let id = set_id(p)?;
                let (name, mark, page) = (opt_string(p, "name")?, opt_string(p, "mark")?, opt_string(p, "page_color")?);
                if let Some(page) = page.as_deref().filter(|c| !c.is_empty()) {
                    let valid = page.len() == 7 && page.starts_with('#') && page.chars().skip(1).all(|c| c.is_ascii_hexdigit());
                    if !valid {
                        return Err(CommandError::Invalid { name: "page_color", reason: "a colour is #RRGGBB, such as #C9DAF8".into() });
                    }
                }
                let mut changed = s.script.clone();
                changed
                    .update_revision_set(id, name.as_deref(), mark.as_deref(), page.as_deref().map(|c| (!c.is_empty()).then_some(c)))
                    .map_err(|reason| CommandError::Invalid { name: "id", reason })?;
                if changed.revisions == s.script.revisions {
                    return Ok(Outcome::done());
                }
                s.checkpoint(EditKind::Other);
                s.script = changed;
                Ok(Outcome::done())
            },
        },
        Command {
            id: "revisions.delete",
            label: "Delete Revision Set",
            menu: "",
            shortcut: None,
            params: const { &[param("id", Integer, true, "The set; its marks move to the set before it")] },
            enabled: always,
            run: |s, p| {
                let id = set_id(p)?;
                let mut changed = s.script.clone();
                let moved = changed.delete_revision_set(id).map_err(|reason| CommandError::Invalid { name: "id", reason })?;
                s.checkpoint(EditKind::Other);
                s.script = changed;
                Ok(Outcome::value(json!({ "moved": moved })))
            },
        },
        Command {
            id: "revisions.set_active",
            label: "Active Revision Set",
            menu: "",
            shortcut: None,
            params: const { &[param("id", Integer, true, "The set changes are marked in")] },
            enabled: always,
            run: |s, p| {
                let id = set_id(p)?;
                if s.script.revisions.set(id).is_none() {
                    return Err(CommandError::Invalid { name: "id", reason: "there is no such revision set".into() });
                }
                if s.script.revisions.active != Some(id) {
                    s.checkpoint(EditKind::Other);
                    s.script.revisions.active = Some(id);
                }
                Ok(Outcome::done())
            },
        },
        Command {
            id: "revisions.mode",
            label: "Revision Mode",
            menu: "Production/Revision Mode",
            shortcut: Some("Cmd+Shift+R"),
            params: const { &[param("on", Bool, false, "On or off (default: the other way)")] },
            enabled: always,
            run: |s, p| {
                let on = opt_bool(p, "on")?.unwrap_or(!s.script.revisions.mode);
                // Kept in the file, so turning it on or off is an edit with its own Undo.
                if on != s.script.revisions.mode {
                    s.checkpoint(EditKind::Other);
                    if on && s.script.revisions.active.is_none() {
                        // The first set, so changes have somewhere to be marked.
                        s.script.add_revision_set(&today_short());
                    }
                    s.script.revisions.mode = on;
                }
                let set = s.script.revisions.active.and_then(|id| s.script.revisions.set(id)).map(|set| set.name.clone()).unwrap_or_default();
                Ok(Outcome::message(if on { format!("Revision mode: changes are marked in {set}") } else { "Revision mode is off".to_string() }))
            },
        },
        Command {
            id: "revisions.mark",
            label: "Mark Revised",
            menu: "",
            shortcut: None,
            params: const {
                &[
                    INDEX,
                    param("start", Integer, false, "Byte offset where the text starts (default: the whole paragraph)"),
                    param("end", Integer, false, "Byte offset where it ends"),
                ]
            },
            enabled: always,
            run: |s, p| mark_range(s, p, true),
        },
        Command {
            id: "revisions.clear",
            label: "Clear Revised",
            menu: "",
            shortcut: None,
            params: const {
                &[
                    INDEX,
                    param("start", Integer, false, "Byte offset where the text starts (default: the whole paragraph)"),
                    param("end", Integer, false, "Byte offset where it ends"),
                ]
            },
            enabled: always,
            run: |s, p| mark_range(s, p, false),
        },
        Command {
            id: "pages.lock",
            label: "Lock Pages",
            menu: "Production/Lock Pages",
            shortcut: None,
            params: const { &[param("revision_mode", Bool, false, "Also turn on revision mode, as a locked script is a production draft")] },
            enabled: |s| !s.script.pages_locked(),
            run: |s, p| {
                if s.script.pages_locked() {
                    return Err(CommandError::Failed("the pages are already locked".into()));
                }
                let paginated = storyteller_core::paginate::paginate(&s.script, &s.layout);
                // One Undo for the locks and the revision mode with them.
                s.checkpoint(EditKind::Other);
                if opt_bool(p, "revision_mode")?.unwrap_or(false) && !s.script.revisions.mode {
                    if s.script.revisions.active.is_none() {
                        s.script.add_revision_set(&today_short());
                    }
                    s.script.revisions.mode = true;
                }
                let count = storyteller_core::paginate::lock_pages(&mut s.script, &paginated);
                Ok(Outcome {
                    message: Some(format!("Locked {count} {}", if count == 1 { "page" } else { "pages" })),
                    value: json!({ "pages": count }),
                })
            },
        },
        Command {
            id: "pages.lock_a_pages",
            label: "Lock A-Pages",
            menu: "Production/Lock A-Pages",
            shortcut: None,
            params: const { &[] },
            enabled: |s| s.script.pages_locked(),
            run: |s, _| {
                let paginated = storyteller_core::paginate::paginate(&s.script, &s.layout);
                if paginated.pages.iter().any(|p| p.locked.is_none()) {
                    return Err(CommandError::Failed(
                        "these locked pages came from another app and are not placed reliably yet; Unlock All Pages, then Lock Pages".into(),
                    ));
                }
                let mut locked = s.script.clone();
                let count = storyteller_core::paginate::lock_a_pages(&mut locked, &paginated);
                if count == 0 {
                    return Err(CommandError::Failed("there are no A-pages to lock".into()));
                }
                s.checkpoint(EditKind::Other);
                s.script = locked;
                Ok(Outcome {
                    message: Some(format!("Locked {count} A-{}", if count == 1 { "page" } else { "pages" })),
                    value: json!({ "pages": count }),
                })
            },
        },
        Command {
            id: "pages.unlock_a_pages",
            label: "Unlock A-Pages",
            menu: "Production/Unlock A-Pages",
            shortcut: None,
            params: const { &[] },
            enabled: |s| s.script.lock_level() > 0,
            run: |s, _| {
                if s.script.lock_level() == 0 {
                    return Err(CommandError::Failed("no A-pages are locked".into()));
                }
                s.checkpoint(EditKind::Other);
                let count = s.script.unlock_a_pages();
                Ok(Outcome {
                    message: Some(format!("Unlocked {count} A-{}", if count == 1 { "page" } else { "pages" })),
                    value: json!({ "pages": count }),
                })
            },
        },
        Command {
            id: "pages.unlock_all",
            label: "Unlock All Pages",
            menu: "Production/Unlock All Pages",
            shortcut: None,
            params: const { &[] },
            enabled: |s| s.script.pages_locked(),
            run: |s, _| {
                if !s.script.pages_locked() {
                    return Err(CommandError::Failed("no pages are locked".into()));
                }
                s.checkpoint(EditKind::Other);
                let count = s.script.unlock_pages();
                Ok(Outcome { message: Some("Unlocked all pages; they number straight through".into()), value: json!({ "pages": count }) })
            },
        },
        Command {
            id: "scene.omit",
            label: "Omit Scene",
            menu: "Production/Omit Scene",
            shortcut: None,
            params: const { &[INDEX] },
            enabled: |s| in_live_scene(s),
            run: |s, p| {
                let heading = heading_of(s, element_index(s, p)?)?;
                let mut omitted = s.script.clone();
                omitted.omit_scene(heading).map_err(|reason| CommandError::Invalid { name: "index", reason })?;
                s.checkpoint(EditKind::Other);
                s.script = omitted;
                s.caret = Caret { element: heading, offset: 0 };
                Ok(Outcome::message("Omitted the scene; Production > Unomit Scene brings it back".to_string()))
            },
        },
        Command {
            id: "scene.unomit",
            label: "Unomit Scene",
            menu: "Production/Unomit Scene",
            shortcut: None,
            params: const { &[INDEX] },
            enabled: |s| s.script.elements.get(s.caret.element).is_some_and(storyteller_core::Element::is_omitted),
            run: |s, p| {
                let heading = heading_of(s, element_index(s, p)?)?;
                let mut back = s.script.clone();
                back.unomit_scene(heading).map_err(|reason| CommandError::Invalid { name: "index", reason })?;
                s.checkpoint(EditKind::Other);
                s.script = back;
                s.caret = Caret { element: heading, offset: 0 };
                Ok(Outcome::done())
            },
        },
        Command {
            id: "scene.number_margins",
            label: "Scene Number Margins",
            menu: "",
            shortcut: None,
            params: const {
                &[param("left", Bool, false, "Print scene numbers in the left margin"), param("right", Bool, false, "Print them in the right margin")]
            },
            enabled: always,
            run: |s, p| {
                let (left, right) = s.script.scene_number_margins();
                let (left, right) = (opt_bool(p, "left")?.unwrap_or(left), opt_bool(p, "right")?.unwrap_or(right));
                let mut changed = s.script.clone();
                if changed.set_scene_number_margins(left, right) {
                    s.checkpoint(EditKind::Other);
                    s.script = changed;
                    s.invalidate_layout();
                }
                Ok(Outcome::value(json!({ "left": left, "right": right })))
            },
        },
        Command {
            id: "scene.set_number",
            label: "Edit Scene Number",
            menu: "",
            shortcut: None,
            params: const { &[INDEX, param("number", String, true, "The number, such as 12 or 12A (empty clears it)")] },
            enabled: always,
            run: |s, p| {
                let index = element_index(s, p)?;
                let heading = s
                    .script
                    .elements
                    .iter()
                    .take(index + 1)
                    .rposition(|e| e.kind == ElementKind::SceneHeading)
                    .ok_or_else(|| CommandError::Invalid { name: "index", reason: "that is not in a scene".into() })?;
                let number = string(p, "number")?;
                let mut renumbered = s.script.clone();
                renumbered.set_scene_number(heading, &number).map_err(|reason| CommandError::Invalid { name: "number", reason })?;
                if renumbered.elements.get(heading) == s.script.elements.get(heading) {
                    return Ok(Outcome::done());
                }
                s.checkpoint(EditKind::Other);
                s.script = renumbered;
                Ok(Outcome::value(json!({ "heading": heading })))
            },
        },
        Command {
            id: "scene.clear_numbers",
            label: "Remove Scene Numbers",
            menu: "Production/Remove Scene Numbers",
            shortcut: None,
            params: &[],
            enabled: always,
            run: |s, _| {
                s.checkpoint(EditKind::Other);
                s.script.clear_scene_numbers();
                Ok(Outcome::done())
            },
        },
    ]
}
