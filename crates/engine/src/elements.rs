//! Elements: their text, scene properties, styles, types, and Enter and Tab.

use super::*;

/// Elements: their text, scene properties, styles, types, and Enter and Tab.
pub(super) fn element_commands() -> Vec<Command> {
    use ParamType::*;
    vec![
        Command {
            id: "element.set_text",
            label: "Set Text",
            menu: "",
            shortcut: None,
            params: const {
                &[
                    INDEX,
                    param("text", String, true, "New text, with Fountain inline markup"),
                    param("typing", Bool, false, "Merge with the previous typing step"),
                ]
            },
            enabled: always,
            run: |s, p| {
                let index = element_index(s, p)?;
                let text = string(p, "text")?;
                set_text(s, index, &text, opt_bool(p, "typing")?.unwrap_or(false))?;
                if s.caret.element == index {
                    s.caret.offset = s.caret.offset.min(s.script.elements.get(index).map_or(0, |e| e.text.len()));
                }
                Ok(Outcome::done())
            },
        },
        Command {
            id: "scene.set_properties",
            label: "Scene Properties",
            menu: "",
            shortcut: None,
            params: const {
                &[
                    INDEX,
                    param("title", String, false, "The scene's short title (empty for none)"),
                    param("summary", String, false, "What happens in it (empty for none)"),
                    param("color", String, false, "#RRGGBB (empty for none)"),
                ]
            },
            enabled: always,
            run: |s, p| {
                // The scene the element is in: its heading.
                let index = element_index(s, p)?;
                let heading = s
                    .script
                    .elements
                    .iter()
                    .take(index + 1)
                    .rposition(|e| e.kind == ElementKind::SceneHeading)
                    .ok_or_else(|| CommandError::Invalid { name: "index", reason: "that is not in a scene".into() })?;
                let mut meta = s.script.elements.get(heading).map(|e| e.scene.clone()).unwrap_or_default();
                if let Some(title) = opt_string(p, "title")? {
                    meta.title = title.replace(['\n', '\r'], " ").trim().to_string();
                }
                if let Some(summary) = opt_string(p, "summary")? {
                    meta.summary = summary.replace('\r', "").trim().to_string();
                }
                if let Some(color) = opt_string(p, "color")? {
                    let color = color.trim();
                    let valid = color.len() == 7 && color.starts_with('#') && color.chars().skip(1).all(|c| c.is_ascii_hexdigit());
                    if !color.is_empty() && !valid {
                        return Err(CommandError::Invalid { name: "color", reason: "a colour is #RRGGBB, such as #3366CC".into() });
                    }
                    let new = storyteller_core::model::SceneMeta { color: (!color.is_empty()).then(|| color.to_uppercase()), ..Default::default() };
                    // The same colour as before keeps the way the file wrote it.
                    if new.rgb() != meta.rgb() {
                        meta.color = new.color;
                    }
                }
                if s.script.elements.get(heading).is_some_and(|e| e.scene == meta) {
                    return Ok(Outcome::done());
                }
                s.checkpoint(EditKind::Other);
                if let Some(element) = s.script.elements.get_mut(heading) {
                    element.scene = meta;
                }
                Ok(Outcome::value(json!({ "heading": heading })))
            },
        },
        Command {
            id: "format.style",
            label: "Bold, Italic or Underline",
            menu: "",
            shortcut: None,
            params: const {
                &[
                    INDEX,
                    param("start", Integer, true, "Byte offset where the text to style starts"),
                    param("end", Integer, true, "Byte offset where it ends"),
                    param("style", String, true, "bold, italic or underline"),
                ]
            },
            enabled: always,
            run: |s, p| {
                let index = element_index(s, p)?;
                let which = storyteller_core::inline::StyleKind::from_id(&string(p, "style")?)
                    .ok_or_else(|| CommandError::Invalid { name: "style", reason: "bold, italic or underline".into() })?;
                let (start, end) = (opt_index(p, "start")?.unwrap_or(0), opt_index(p, "end")?.unwrap_or(0));
                let text = s
                    .script
                    .elements
                    .get(index)
                    .map(|e| e.text.clone())
                    .ok_or(CommandError::Invalid { name: "index", reason: "no such element".into() })?;
                let (styled, from, to) =
                    storyteller_core::inline::toggle_style(&text, start.min(end), end.max(start), which).ok_or_else(|| CommandError::Invalid {
                        name: "start",
                        reason: "that text can't take the style here: select words with a space or punctuation either side".into(),
                    })?;
                s.checkpoint(EditKind::Other);
                if let Some(element) = s.script.elements.get_mut(index) {
                    element.text = styled;
                }
                s.caret = Caret { element: index, offset: to };
                Ok(Outcome::value(json!({ "start": from, "end": to })))
            },
        },
        Command {
            id: "element.insert",
            label: "Insert Element",
            menu: "",
            shortcut: None,
            params: const {
                &[
                    param("index", Integer, false, "Insert after this element (default: the caret's)"),
                    param("kind", Kind, true, "Element type id"),
                    param("text", String, false, "Text"),
                ]
            },
            enabled: always,
            run: |s, p| {
                let after = element_index(s, p)?;
                let kind = kind(p, "kind")?;
                let mut element = Element::new(kind, opt_string(p, "text")?.unwrap_or_default());
                element.normalize();
                element.mark_whole(s.marking_set());
                s.checkpoint(EditKind::Other);
                let len = element.text.len();
                insert(s, after + 1, element)?;
                s.caret = Caret { element: after + 1, offset: len };
                Ok(Outcome::value(json!({ "index": after + 1 })))
            },
        },
        Command {
            id: "edit.paste_screenplay",
            label: "Paste Screenplay Text",
            menu: "",
            shortcut: None,
            params: const {
                &[
                    INDEX,
                    param("start", Integer, false, "Byte offset where the paste begins (default: the caret)"),
                    param("end", Integer, false, "Byte offset where the replaced selection ends (default: start)"),
                    param("text", String, true, "Screenplay text, read as Fountain"),
                ]
            },
            enabled: always,
            run: |s, p| {
                let index = element_index(s, p)?;
                let start = text_offset(s, p, index).and_then(|caret| Ok(opt_index(p, "start")?.unwrap_or(caret)))?;
                let end = opt_index(p, "end")?.unwrap_or(start);
                let pasted = paste_screenplay(s, index, start, end, &string(p, "text")?)?;
                Ok(Outcome::value(json!({ "elements": pasted })))
            },
        },
        Command {
            id: "edit.copy_range",
            label: "Copy Range",
            menu: "",
            shortcut: None,
            params: const { &RANGE },
            enabled: always,
            run: |s, p| {
                let (from, to) = text_range(s, p)?;
                let elements = range_elements(s, from, to);
                let text = storyteller_core::fountain::write(&Script { elements: elements.clone(), ..Script::default() });
                s.remember_copied(text.clone(), elements);
                Ok(Outcome::value(json!({ "text": text })))
            },
        },
        Command {
            id: "edit.replace_range",
            label: "Replace Range",
            menu: "",
            shortcut: None,
            params: const {
                &[
                    RANGE[0],
                    RANGE[1],
                    RANGE[2],
                    RANGE[3],
                    param("text", String, false, "What takes its place: plain text, or screenplay text that becomes elements (default: nothing)"),
                ]
            },
            enabled: always,
            run: |s, p| {
                let (from, to) = text_range(s, p)?;
                let text = if p.contains_key("text") { string(p, "text")? } else { std::string::String::new() };
                replace_range(s, from, to, &text)?;
                Ok(Outcome::done())
            },
        },
        Command {
            id: "edit.copy_scene",
            label: "Copy Scene",
            menu: "Edit/Copy Scene",
            shortcut: None,
            params: const { &[INDEX] },
            enabled: always,
            run: |s, p| {
                let index = element_index(s, p)?;
                let range = scene_range(&s.script, index);
                let count = range.len();
                let elements = s.script.elements.get(range).unwrap_or_default().to_vec();
                let text = storyteller_core::fountain::write(&Script { elements, ..Script::default() });
                Ok(Outcome { message: Some(format!("Copied the scene ({count} elements)")), value: json!({ "text": text }) })
            },
        },
        Command {
            id: "element.delete",
            label: "Delete Element",
            menu: "",
            shortcut: None,
            params: const { &[INDEX] },
            enabled: always,
            run: |s, p| {
                let index = element_index(s, p)?;
                s.checkpoint(EditKind::Other);
                s.script.elements.remove(index);
                if s.script.elements.is_empty() {
                    s.script.elements.push(Element::new(ElementKind::SceneHeading, ""));
                }
                s.caret = Caret { element: index.saturating_sub(usize::from(index >= s.script.elements.len())), offset: 0 };
                Ok(Outcome::done())
            },
        },
        Command {
            id: "element.set_kind",
            label: "Set Element Type",
            menu: "",
            shortcut: None,
            params: const { &[INDEX, param("kind", Kind, true, "Element type id")] },
            enabled: always,
            run: |s, p| {
                let index = element_index(s, p)?;
                set_kind(s, index, kind(p, "kind")?)?;
                Ok(Outcome::done())
            },
        },
        kind_command!("format.scene_heading", "Scene Heading", "Format/Scene Heading", "Cmd+1", ElementKind::SceneHeading),
        kind_command!("format.action", "Action", "Format/Action", "Cmd+2", ElementKind::Action),
        kind_command!("format.character", "Character", "Format/Character", "Cmd+3", ElementKind::Character),
        kind_command!("format.parenthetical", "Parenthetical", "Format/Parenthetical", "Cmd+4", ElementKind::Parenthetical),
        kind_command!("format.dialogue", "Dialogue", "Format/Dialogue", "Cmd+5", ElementKind::Dialogue),
        kind_command!("format.transition", "Transition", "Format/Transition", "Cmd+6", ElementKind::Transition),
        kind_command!("format.shot", "Shot", "Format/Shot", "Cmd+7", ElementKind::Shot),
        convert_command!(
            "convert.scene_heading",
            "Change to Scene Heading",
            "Format/Change to Scene Heading",
            CONVERT_KEYS[0],
            ElementKind::SceneHeading
        ),
        convert_command!("convert.action", "Change to Action", "Format/Change to Action", CONVERT_KEYS[1], ElementKind::Action),
        convert_command!("convert.character", "Change to Character", "Format/Change to Character", CONVERT_KEYS[2], ElementKind::Character),
        convert_command!(
            "convert.parenthetical",
            "Change to Parenthetical",
            "Format/Change to Parenthetical",
            CONVERT_KEYS[3],
            ElementKind::Parenthetical
        ),
        convert_command!("convert.dialogue", "Change to Dialogue", "Format/Change to Dialogue", CONVERT_KEYS[4], ElementKind::Dialogue),
        convert_command!("convert.transition", "Change to Transition", "Format/Change to Transition", CONVERT_KEYS[5], ElementKind::Transition),
        convert_command!("convert.shot", "Change to Shot", "Format/Change to Shot", CONVERT_KEYS[6], ElementKind::Shot),
        Command {
            id: "convert.new_act",
            label: "Change to New Act",
            menu: "Format/Change to New Act",
            shortcut: None,
            params: const { &[INDEX] },
            enabled: always,
            run: |s, p| {
                let index = element_index(s, p)?;
                set_kind(s, index, ElementKind::NewAct)?;
                Ok(Outcome::done())
            },
        },
        Command {
            id: "convert.end_of_act",
            label: "Change to End of Act",
            menu: "Format/Change to End of Act",
            shortcut: None,
            params: const { &[INDEX] },
            enabled: always,
            run: |s, p| {
                let index = element_index(s, p)?;
                set_kind(s, index, ElementKind::EndOfAct)?;
                Ok(Outcome::done())
            },
        },
        Command {
            id: "flow.enter",
            label: "New Element (Enter)",
            menu: "",
            shortcut: Some("Enter"),
            params: const { &[INDEX, OFFSET] },
            enabled: always,
            run: |s, p| {
                let index = element_index(s, p)?;
                let offset = text_offset(s, p, index)?;
                Ok(match enter(s, index, offset)? {
                    Entered::Next => Outcome::done(),
                    Entered::Picker => Outcome { message: Some("Choose an element".into()), value: serde_json::json!({ "picker": true }) },
                })
            },
        },
        Command {
            id: "flow.tab",
            label: "Next Element Type (Tab)",
            menu: "",
            shortcut: Some("Tab"),
            params: const { &[INDEX] },
            enabled: always,
            run: |s, p| {
                let index = element_index(s, p)?;
                tab(s, index)?;
                Ok(Outcome::done())
            },
        },
        Command {
            id: "flow.shift_tab",
            label: "Previous Element Type (Shift-Tab)",
            menu: "",
            shortcut: Some("Shift+Tab"),
            params: const { &[INDEX] },
            enabled: always,
            run: |s, p| {
                let index = element_index(s, p)?;
                let to = s.element(index).map(|e| flow::rule_for(e.kind).shift_tab).unwrap_or(ElementKind::Action);
                set_kind(s, index, to)?;
                Ok(Outcome::done())
            },
        },
        Command {
            id: "element.merge_previous",
            label: "Join With Previous (Backspace at Start)",
            menu: "",
            shortcut: None,
            params: const { &[INDEX] },
            enabled: always,
            run: |s, p| {
                let index = element_index(s, p)?;
                let merged = merge_previous(s, index)?;
                Ok(Outcome::value(json!({ "merged": merged })))
            },
        },
    ]
}
