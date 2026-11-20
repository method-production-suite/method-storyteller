//! Undo, the caret, Go To, find and replace, characters, reports and spelling.

use super::*;

/// Undo, the caret, Go To, find and replace, characters, reports and spelling.
pub(super) fn editing_commands() -> Vec<Command> {
    use ParamType::*;
    vec![
        Command {
            id: "edit.undo",
            label: "Undo",
            menu: "Edit/Undo",
            shortcut: Some("Cmd+Z"),
            params: &[],
            enabled: Session::can_undo,
            run: |s, _| {
                s.undo();
                Ok(Outcome::done())
            },
        },
        Command {
            id: "edit.redo",
            label: "Redo",
            menu: "Edit/Redo",
            shortcut: Some("Cmd+Shift+Z"),
            params: &[],
            enabled: Session::can_redo,
            run: |s, _| {
                s.redo();
                Ok(Outcome::done())
            },
        },
        Command {
            id: "caret.set",
            label: "Move Caret",
            menu: "",
            shortcut: None,
            params: const { &[INDEX, OFFSET] },
            enabled: always,
            run: |s, p| {
                let index = element_index(s, p)?;
                let offset = text_offset(s, p, index)?;
                s.caret = Caret { element: index, offset };
                s.break_typing();
                Ok(Outcome::done())
            },
        },
        Command {
            id: "nav.go_to",
            label: "Go To",
            menu: "",
            shortcut: None,
            params: const {
                &[
                    param("scene", String, false, "Scene number as printed (12, 12A), or its position when scenes are unnumbered"),
                    param("page", Integer, false, "Page number"),
                    param("act", String, false, "Act: its position counted from 1, or its name (TAG, ACT TWO, two)"),
                    param("bookmark", String, false, "A bookmark's name, or enough of its start to tell it from the others"),
                ]
            },
            enabled: always,
            run: |s, p| {
                let index = if let Some(name) = opt_string(p, "bookmark")? {
                    find_bookmark(&s.script, &name)?
                } else if let Some(act) = opt_string(p, "act")? {
                    s.script.find_act(&act).ok_or_else(|| CommandError::Invalid { name: "act", reason: format!("there is no act {}", act.trim()) })?
                } else {
                    go_to(s, opt_string(p, "scene")?, opt_index(p, "page")?)?
                };
                s.caret = Caret { element: index, offset: 0 };
                s.break_typing();
                Ok(Outcome::value(json!({ "element": index })))
            },
        },
        Command {
            id: "find.next",
            label: "Find Next",
            menu: "",
            shortcut: None,
            params: const {
                &[FIND_TEXT, FIND_CASE, FIND_WORD, FIND_KIND, param("backwards", Bool, false, "Find the previous match instead"), INDEX, OFFSET]
            },
            enabled: always,
            run: |s, p| {
                let query = find_query(p)?;
                let hits = find::find_all(&s.script, &query);
                let element = opt_index(p, "index")?.unwrap_or(s.caret.element);
                let offset = opt_index(p, "offset")?.unwrap_or(s.caret.offset);
                let backwards = opt_bool(p, "backwards")?.unwrap_or(false);
                let Some((number, hit)) = find::find_next(&hits, element, offset, backwards) else {
                    return Ok(Outcome { message: Some(format!("“{}” is not in the script", query.text)), value: json!({ "total": 0 }) });
                };
                // Forward, the caret goes after the match; backwards, before it, so repeating moves on.
                s.caret = Caret { element: hit.element, offset: if backwards { hit.start } else { hit.end } };
                s.break_typing();
                Ok(Outcome::value(json!({ "element": hit.element, "start": hit.start, "end": hit.end, "number": number + 1, "total": hits.len() })))
            },
        },
        Command {
            id: "find.replace",
            label: "Replace",
            menu: "",
            shortcut: None,
            params: const {
                &[
                    FIND_TEXT,
                    FIND_CASE,
                    FIND_WORD,
                    FIND_KIND,
                    FIND_REPLACEMENT,
                    INDEX,
                    param("start", Integer, true, "Byte offset where the match to replace starts"),
                ]
            },
            enabled: always,
            run: |s, p| {
                let query = find_query(p)?;
                let replacement = string(p, "replacement")?;
                let index = element_index(s, p)?;
                let start = opt_index(p, "start")?.ok_or(CommandError::Missing("start"))?;
                let hits = find::find_all(&s.script, &query);
                let Some(hit) = hits.iter().find(|h| h.element == index && h.start == start) else {
                    return Ok(Outcome { message: Some("Nothing to replace there".into()), value: json!({ "replaced": false }) });
                };
                let text = s.script.elements.get(index).map(|e| e.text.clone()).unwrap_or_default();
                let new_text = format!("{}{replacement}{}", text.get(..hit.start).unwrap_or(""), text.get(hit.end..).unwrap_or(""));
                set_text(s, index, &new_text, false)?;
                s.caret =
                    Caret { element: index, offset: (hit.start + replacement.len()).min(s.script.elements.get(index).map_or(0, |e| e.text.len())) };
                Ok(Outcome::value(json!({ "replaced": true })))
            },
        },
        Command {
            id: "find.replace_all",
            label: "Replace All",
            menu: "",
            shortcut: None,
            params: const { &[FIND_TEXT, FIND_CASE, FIND_WORD, FIND_KIND, FIND_REPLACEMENT] },
            enabled: always,
            run: |s, p| {
                let query = find_query(p)?;
                let replacement = string(p, "replacement")?;
                let hits = find::find_all(&s.script, &query);
                if hits.is_empty() {
                    return Ok(Outcome { message: Some(format!("“{}” is not in the script", query.text)), value: json!({ "count": 0 }) });
                }
                // Work out every new text first: a replacement that changes nothing is no edit.
                let mut changed: Vec<(usize, std::string::String)> = Vec::new();
                for (index, element) in s.script.elements.iter().enumerate() {
                    let ranges: Vec<&find::Hit> = hits.iter().filter(|h| h.element == index).collect();
                    if ranges.is_empty() {
                        continue;
                    }
                    let mut text = element.text.clone();
                    for hit in ranges.iter().rev() {
                        text.replace_range(hit.start..hit.end, &replacement);
                    }
                    if element.kind.is_single_line() {
                        text = text.replace(['\n', '\r'], " ");
                    }
                    if element.kind.is_caps() {
                        text = text.to_uppercase();
                    }
                    if text != element.text {
                        changed.push((index, text));
                    }
                }
                if changed.is_empty() {
                    return Ok(Outcome { message: Some("Nothing to change".into()), value: json!({ "count": 0 }) });
                }
                // One undo step for the lot.
                s.checkpoint(EditKind::Other);
                let mark = s.marking_set();
                for (index, text) in changed {
                    if let Some(element) = s.script.elements.get_mut(index) {
                        element.set_text_revised(text, mark);
                    }
                }
                s.clamp_caret();
                let count = hits.len();
                Ok(Outcome {
                    message: Some(format!("Replaced {count} {}", if count == 1 { "match" } else { "matches" })),
                    value: json!({ "count": count }),
                })
            },
        },
        Command {
            id: "character.rename",
            label: "Rename Character",
            menu: "",
            shortcut: None,
            params: const {
                &[
                    param("from", String, true, "The character's current name"),
                    param("to", String, true, "The new name"),
                    param("everywhere", Bool, false, "Also rename capitalised whole-word mentions in action, dialogue and other text"),
                    param("merge", Bool, false, "Allow renaming onto a name another character already has, merging the two"),
                ]
            },
            enabled: always,
            run: |s, p| {
                let from = string(p, "from")?.trim().to_uppercase();
                let to = string(p, "to")?.trim().to_string();
                if to.is_empty() {
                    return Err(CommandError::Invalid { name: "to", reason: "the new name is empty".into() });
                }
                if to.contains(['(', ')', '^']) {
                    return Err(CommandError::Invalid {
                        name: "to",
                        reason: "a name cannot contain ( ) or ^; extensions such as (V.O.) are kept as they are".into(),
                    });
                }
                let new_name = to.to_uppercase();
                let merge = opt_bool(p, "merge")?.unwrap_or(false);
                if new_name != from && !merge && character_names(&s.script).iter().any(|(n, _)| *n == new_name) {
                    return Err(CommandError::Invalid {
                        name: "to",
                        reason: format!("{new_name} is already a character; renaming would merge the two"),
                    });
                }
                let everywhere = opt_bool(p, "everywhere")?.unwrap_or(false);
                let is_cue = |e: &Element| e.kind == ElementKind::Character && e.character_name().to_uppercase() == from;
                if !s.script.elements.iter().any(is_cue) {
                    return Err(CommandError::Invalid { name: "from", reason: format!("no character is called {from}") });
                }
                let mention = find::Query { text: from.clone(), case: false, whole_word: true, kind: None };
                // One undo step for every cue and mention.
                s.checkpoint(EditKind::Other);
                let mark = s.marking_set();
                let (mut cues, mut mentions) = (0, 0);
                // The name in the lists goes with it, so the old one is not offered again.
                let old_name = from.to_uppercase();
                if s.script.lists.characters.iter().any(|name| name.to_uppercase() == old_name) {
                    s.script.lists.characters.retain(|name| name.to_uppercase() != old_name);
                    storyteller_core::autocomplete::Lists::add(&mut s.script.lists.characters, &new_name);
                }
                for element in &mut s.script.elements {
                    if is_cue(element) {
                        // Keep the extensions: `NORA (V.O.)` becomes `EMMA (V.O.)`.
                        let name_len = element.character_name().len();
                        let start = element.text.len() - element.text.trim_start().len();
                        let rest = element.text.get(start + name_len..).unwrap_or("").to_string();
                        element.set_text_revised(format!("{new_name}{rest}"), mark);
                        cues += 1;
                    } else if everywhere && element.kind != ElementKind::Character {
                        // Names in prose are capitalised, so `will` is not WILL and `may` is not MAY.
                        let hits: Vec<(usize, usize)> = find::find_in(&element.text, &mention)
                            .into_iter()
                            .filter(|&(start, _)| element.text.get(start..).and_then(|t| t.chars().next()).is_some_and(char::is_uppercase))
                            .collect();
                        let mut text = element.text.clone();
                        for &(start, end) in hits.iter().rev() {
                            let found = text.get(start..end).unwrap_or("");
                            let name = name_like(found, &to);
                            text.replace_range(start..end, &name);
                        }
                        if element.kind.is_caps() {
                            text = text.to_uppercase();
                        }
                        element.set_text_revised(text, mark);
                        mentions += hits.len();
                    }
                }
                s.clamp_caret();
                let message = if everywhere {
                    format!("Renamed {from} in {cues} {} and {mentions} {}", plural(cues, "cue", "cues"), plural(mentions, "mention", "mentions"))
                } else {
                    format!("Renamed {from} in {cues} {}", plural(cues, "cue", "cues"))
                };
                Ok(Outcome { message: Some(message), value: json!({ "cues": cues, "mentions": mentions }) })
            },
        },
        Command {
            id: "character.list",
            label: "List Characters",
            menu: "",
            shortcut: None,
            params: &[],
            enabled: always,
            run: |s, _| Ok(Outcome::value(json!(character_names(&s.script)))),
        },
        Command {
            id: "character.table",
            label: "Character Table",
            menu: "",
            shortcut: None,
            params: const { &[REPORT_PATH] },
            enabled: always,
            run: |s, p| {
                let rows = character_table(&s.script);
                report(s, p, json!(rows), reports::characters_table(&rows))
            },
        },
        Command {
            id: "report.scenes",
            label: "Scene Report",
            menu: "",
            shortcut: None,
            params: const { &[REPORT_PATH] },
            enabled: always,
            run: |s, p| {
                let rows = reports::session_scenes(s);
                report(s, p, json!(rows), reports::scenes_table(&rows))
            },
        },
        Command {
            id: "report.locations",
            label: "Location Report",
            menu: "",
            shortcut: None,
            params: const { &[REPORT_PATH] },
            enabled: always,
            run: |s, p| {
                let rows = reports::locations(&reports::session_scenes(s));
                report(s, p, json!(rows), reports::locations_table(&rows))
            },
        },
        Command {
            id: "report.cast",
            label: "Cast Report",
            menu: "",
            shortcut: None,
            params: const { &[REPORT_PATH] },
            enabled: always,
            run: |s, p| {
                let scenes = reports::session_scenes(s);
                let rows = reports::cast(&scenes);
                let by_scene: Vec<Value> = scenes.iter().map(|r| json!({ "scene": r.label(), "characters": r.characters })).collect();
                report(s, p, json!({ "characters": rows, "scenes": by_scene }), reports::cast_table(&rows))
            },
        },
        Command {
            id: "spell.next",
            label: "Next Misspelling",
            menu: "",
            shortcut: None,
            params: const { &[param("backwards", Bool, false, "Find the previous one instead"), INDEX, OFFSET] },
            enabled: always,
            run: |s, p| {
                let misses = crate::spell::misspelled(&s.script, &s.spelling);
                let element = opt_index(p, "index")?.unwrap_or(s.caret.element);
                let offset = opt_index(p, "offset")?.unwrap_or(s.caret.offset);
                let backwards = opt_bool(p, "backwards")?.unwrap_or(false);
                let hits: Vec<find::Hit> = misses.iter().map(|m| find::Hit { element: m.element, start: m.start, end: m.end }).collect();
                let Some((number, hit)) = find::find_next(&hits, element, offset, backwards) else {
                    return Ok(Outcome { message: Some("No misspellings".into()), value: json!({ "total": 0 }) });
                };
                let word = misses.get(number).map(|m| m.word.clone()).unwrap_or_default();
                s.caret = Caret { element: hit.element, offset: if backwards { hit.start } else { hit.end } };
                s.break_typing();
                Ok(Outcome::value(json!({
                    "element": hit.element, "start": hit.start, "end": hit.end, "word": word,
                    "suggestions": crate::spell::suggestions(&word, s.spelling.language, 6), "number": number + 1, "total": hits.len(),
                })))
            },
        },
        Command {
            id: "spell.ignore",
            label: "Ignore Spelling",
            menu: "",
            shortcut: None,
            params: const { &[param("word", String, true, "The word to accept in this script (kept with the script)")] },
            enabled: always,
            run: |s, p| {
                let word = string(p, "word")?;
                let word = word.trim();
                let lower = word.to_lowercase();
                // Kept with the script as written, so it stays ignored when the file is saved and
                // reopened; a word already there in any case is not added again.
                if !s.script.spelling_ignored.iter().any(|w| w.to_lowercase() == lower) {
                    s.script.spelling_ignored.push(word.to_string());
                    s.dirty = true;
                }
                Ok(Outcome::message(format!("Ignoring “{word}” in this script")))
            },
        },
        Command {
            id: "spell.learn",
            label: "Learn Spelling",
            menu: "",
            shortcut: None,
            params: const { &[param("word", String, true, "The word to add to the writer's dictionary")] },
            enabled: always,
            run: |s, p| {
                let word = string(p, "word")?;
                s.spelling.learned.insert(word.to_lowercase());
                Ok(Outcome { message: Some(format!("Added “{word}” to your dictionary")), value: json!({ "learned": word.to_lowercase() }) })
            },
        },
        Command {
            id: "spell.change",
            label: "Change Spelling",
            menu: "",
            shortcut: None,
            params: const {
                &[
                    INDEX,
                    param("start", Integer, true, "Byte offset where the word starts"),
                    param("word", String, true, "The word as it is now (nothing changes if the text there differs)"),
                    param("replacement", String, true, "The new spelling"),
                ]
            },
            enabled: always,
            run: |s, p| {
                let index = element_index(s, p)?;
                let start = opt_index(p, "start")?.ok_or(CommandError::Missing("start"))?;
                let word = string(p, "word")?;
                let replacement = string(p, "replacement")?;
                let text = s.script.elements.get(index).map(|e| e.text.clone()).unwrap_or_default();
                let end = start.checked_add(word.len()).ok_or(CommandError::Invalid { name: "start", reason: "out of range".into() })?;
                if text.get(start..end) != Some(word.as_str()) {
                    return Ok(Outcome { message: Some("The word has changed since".into()), value: json!({ "changed": false }) });
                }
                let new_text = format!("{}{replacement}{}", text.get(..start).unwrap_or(""), text.get(end..).unwrap_or(""));
                set_text(s, index, &new_text, false)?;
                let caret = start.saturating_add(replacement.len());
                s.caret = Caret { element: index, offset: caret.min(s.script.elements.get(index).map_or(0, |e| e.text.len())) };
                Ok(Outcome::value(json!({ "changed": true })))
            },
        },
    ]
}
