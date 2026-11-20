//! View settings, the title page and autocomplete suggestions.

use super::*;

/// View settings, the title page and autocomplete suggestions.
pub(super) fn view_commands() -> Vec<Command> {
    use ParamType::*;
    vec![
        Command {
            id: "view.scene_numbers",
            label: "Show Scene Numbers",
            menu: "View/Show Scene Numbers",
            shortcut: None,
            params: const { &[param("show", Bool, false, "Show (true) or hide (false); toggles when omitted")] },
            enabled: always,
            run: |s, p| {
                let show = opt_bool(p, "show")?.unwrap_or(!s.layout.show_scene_numbers);
                s.layout.show_scene_numbers = show;
                s.invalidate_layout();
                Ok(Outcome::value(json!({ "show": show })))
            },
        },
        Command {
            id: "view.show_revisions",
            label: "Show Revisions",
            menu: "",
            shortcut: None,
            params: const { &[param("show", String, true, "active (the active set's), all, or none")] },
            enabled: always,
            run: |s, p| {
                s.show_revisions = show_revisions("show", &string(p, "show")?)?;
                Ok(Outcome::value(json!({ "show": s.show_revisions })))
            },
        },
        Command {
            id: "view.outline",
            label: "Show Outline",
            menu: "View/Show Outline",
            shortcut: None,
            params: const { &[param("show", Bool, false, "Show (true) or hide (false); toggles when omitted")] },
            enabled: always,
            run: |s, p| {
                s.show_outline = opt_bool(p, "show")?.unwrap_or(!s.show_outline);
                // A caret in a paragraph going out of sight moves to the nearest one still there.
                let shown = s.nearest_shown(s.caret.element);
                if shown != s.caret.element {
                    s.caret = Caret { element: shown, offset: 0 };
                }
                Ok(Outcome::value(json!({ "show": s.show_outline })))
            },
        },
        Command {
            id: "view.page_colours",
            label: "Revised Page Colours",
            menu: "View/Revised Page Colours",
            shortcut: None,
            params: const { &[param("show", Bool, false, "Show (true) or hide (false); toggles when omitted")] },
            enabled: always,
            run: |s, p| {
                s.page_colours = opt_bool(p, "show")?.unwrap_or(!s.page_colours);
                Ok(Outcome::value(json!({ "show": s.page_colours })))
            },
        },
        Command {
            id: "title_page.set",
            label: "Set Title Page Field",
            menu: "",
            shortcut: None,
            params: const {
                &[
                    param("key", String, true, "Title, Credit, Author, Source, Draft date, Contact, Copyright or Notes"),
                    param("value", String, true, "Value (empty removes it)"),
                ]
            },
            enabled: always,
            run: |s, p| {
                let key = string(p, "key")?;
                let value = string(p, "value")?;
                if key.trim().is_empty() || key.contains([':', '\n']) {
                    return Err(CommandError::Invalid { name: "key", reason: "keys are one line without colons".into() });
                }
                s.checkpoint(EditKind::Other);
                s.script.title_page.set(key.trim(), &value);
                Ok(Outcome::done())
            },
        },
        Command {
            id: "autocomplete.suggest",
            label: "Autocomplete Suggestions",
            menu: "",
            shortcut: None,
            params: const { &[INDEX] },
            enabled: always,
            run: |s, p| {
                let index = element_index(s, p)?;
                let suggestions = s.element(index).map(|e| autocomplete::suggest(&s.script, e.kind, &e.text)).unwrap_or_default();
                Ok(Outcome::value(json!(suggestions)))
            },
        },
    ]
}
