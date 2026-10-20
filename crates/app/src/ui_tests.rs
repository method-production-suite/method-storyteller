//! The app driven offscreen by egui_kittest: no window, settings in memory only.

use std::io::Write;

use craft_settings::Location;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;

use super::StorytellerApp;

fn harness(script: &str) -> (Harness<'static, StorytellerApp>, tempfile::NamedTempFile) {
    let mut file = tempfile::Builder::new().suffix(".fountain").tempfile().expect("temp file");
    file.write_all(script.as_bytes()).expect("write script");
    let path = file.path().to_path_buf();
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1280.0, 800.0))
        .with_max_steps(64)
        .build_eframe(move |cc| StorytellerApp::without_menu(cc, Some(path), None, Location::Memory, Default::default()));
    harness.run_steps(3);
    (harness, file)
}

fn open_go_to(harness: &mut Harness<'static, StorytellerApp>) {
    harness.state_mut().queued.push("app.go_to".into());
    harness.run_steps(3);
    assert!(harness.state().go_to.is_some(), "the Go To dialog opens");
}

const SCRIPT: &str = "INT. A - DAY #4#\n\nOne.\n\nEXT. B - NIGHT #4A#\n\nTwo.\n\nINT. C - DAY #5#\n\nThree.\n";

#[test]
fn go_to_a_scene_by_number() {
    let (mut harness, _file) = harness(SCRIPT);
    open_go_to(&mut harness);
    if let Some(draft) = harness.state_mut().go_to.as_mut() {
        draft.text = "4a".into();
    }
    harness.get_by_label("Go").click();
    harness.run_steps(3);
    assert!(harness.state().go_to.is_none(), "the dialog closes on success");
    assert_eq!(harness.state().session.caret.element, 2);
}

#[test]
fn go_to_explains_a_missing_page_and_closes_on_escape() {
    let (mut harness, _file) = harness(SCRIPT);
    open_go_to(&mut harness);
    if let Some(draft) = harness.state_mut().go_to.as_mut() {
        draft.mode = super::GoToMode::Page;
        draft.text = "99".into();
    }
    harness.get_by_label("Go").click();
    harness.run_steps(3);
    let error = harness.state().go_to.as_ref().and_then(|d| d.error.clone()).unwrap_or_default();
    assert_eq!(error, "There is no page 99 (the script has 1).");
    harness.key_press(egui::Key::Escape);
    harness.run_steps(3);
    assert!(harness.state().go_to.is_none(), "Escape closes the dialog");
}

const FIND_SCRIPT: &str = "INT. HUT - DAY\n\nNora taps the glass. Nora waits.\n\nNORA\nThe glass is still.\n";

#[test]
fn find_steps_through_matches_and_highlights_them() {
    let (mut harness, _file) = harness(FIND_SCRIPT);
    harness.state_mut().queued.push("app.find".into());
    harness.run_steps(3);
    if let Some(bar) = harness.state_mut().find.as_mut() {
        bar.text = "glass".into();
    }
    harness.run_steps(3);
    let state = harness.state();
    assert_eq!(state.editor.highlight, Some((1, 14, 19)), "typing finds the first match");
    assert_eq!(state.find.as_ref().map(|b| b.status.as_str()), Some("1 of 2"));
    harness.get_by_label("›").click();
    harness.run_steps(3);
    assert_eq!(harness.state().editor.highlight, Some((3, 4, 9)));
    harness.get_by_label("›").click();
    harness.run_steps(3);
    assert_eq!(harness.state().editor.highlight, Some((1, 14, 19)), "Next wraps to the top");
    harness.get_by_label("‹").click();
    harness.run_steps(3);
    assert_eq!(harness.state().editor.highlight, Some((3, 4, 9)), "Previous wraps to the end");
}

#[test]
fn replace_one_then_all_and_undo_all_at_once() {
    let (mut harness, _file) = harness(FIND_SCRIPT);
    harness.state_mut().queued.push("app.find".into());
    harness.run_steps(3);
    if let Some(bar) = harness.state_mut().find.as_mut() {
        bar.text = "nora".into();
        bar.replacement = "Emma".into();
    }
    harness.run_steps(3);
    harness.get_by_label("Replace").click();
    harness.run_steps(3);
    assert_eq!(harness.state().session.script.elements[1].text, "Emma taps the glass. Nora waits.");
    assert_eq!(harness.state().editor.highlight, Some((1, 21, 25)), "Replace moves on to the next match");
    harness.get_by_label("Replace All").click();
    harness.run_steps(3);
    let script = &harness.state().session.script;
    assert_eq!(script.elements[1].text, "Emma taps the glass. Emma waits.");
    assert_eq!(script.elements[2].text, "EMMA", "a cue stays in capitals");
    assert_eq!(harness.state().find.as_ref().map(|b| b.status.as_str()), Some("Replaced 2 matches"));
    harness.state_mut().queued.push("edit.undo".into());
    harness.run_steps(3);
    let script = &harness.state().session.script;
    assert_eq!(script.elements[1].text, "Emma taps the glass. Nora waits.", "one Undo reverses Replace All");
    assert_eq!(script.elements[2].text, "NORA");
}

#[test]
fn rename_character_from_the_dialog() {
    let (mut harness, _file) = harness(FIND_SCRIPT);
    // The caret in the cue picks that character.
    harness.state_mut().editor.pending_focus = Some((2, 0));
    harness.run_steps(3);
    harness.state_mut().queued.push("app.rename_character".into());
    harness.run_steps(3);
    assert_eq!(harness.state().rename.as_ref().map(|d| d.from.as_str()), Some("NORA"));
    if let Some(draft) = harness.state_mut().rename.as_mut() {
        draft.to = "Emma".into();
        draft.everywhere = true;
    }
    harness.get_by_label("Rename").click();
    harness.run_steps(3);
    let state = harness.state();
    assert!(state.rename.is_none(), "the dialog closes");
    assert_eq!(state.session.script.elements[2].text, "EMMA");
    assert_eq!(state.session.script.elements[1].text, "Emma taps the glass. Emma waits.");
    assert_eq!(state.status, "Renamed NORA in 1 cue and 2 mentions");
}

#[test]
fn rename_character_explains_an_empty_name() {
    let (mut harness, _file) = harness(FIND_SCRIPT);
    harness.state_mut().queued.push("app.rename_character".into());
    harness.run_steps(3);
    harness.get_by_label("Rename").click();
    harness.run_steps(3);
    let error = harness.state().rename.as_ref().and_then(|d| d.error.clone());
    assert_eq!(error.as_deref(), Some("The new name is empty."));
}

#[test]
fn escape_closes_the_find_bar() {
    let (mut harness, _file) = harness(FIND_SCRIPT);
    harness.state_mut().queued.push("app.find".into());
    harness.run_steps(3);
    if let Some(bar) = harness.state_mut().find.as_mut() {
        bar.text = "glass".into();
    }
    harness.run_steps(3);
    assert!(harness.state().editor.highlight.is_some());
    harness.key_press(egui::Key::Escape);
    harness.run_steps(3);
    assert!(harness.state().find.is_none(), "Escape in the find field closes the bar");
    assert!(harness.state().editor.highlight.is_none());
}

#[test]
fn an_edit_or_undo_drops_a_stale_highlight() {
    let (mut harness, _file) = harness(FIND_SCRIPT);
    harness.state_mut().queued.push("app.find".into());
    harness.run_steps(3);
    if let Some(bar) = harness.state_mut().find.as_mut() {
        bar.text = "glass".into();
        bar.replacement = "window".into();
    }
    harness.run_steps(3);
    assert_eq!(harness.state().editor.highlight, Some((1, 14, 19)));
    // Replace All leaves no match, and Undo brings the text back; neither keeps an old highlight.
    harness.get_by_label("Replace All").click();
    harness.run_steps(3);
    assert!(harness.state().editor.highlight.is_none());
    harness.state_mut().queued.push("edit.undo".into());
    harness.run_steps(3);
    assert!(harness.state().editor.highlight.is_none());
    // An edit under the match.
    harness.get_by_label("›").click();
    harness.run_steps(3);
    let Some((element, ..)) = harness.state().editor.highlight else { panic!("Next finds a match again after Undo") };
    let params: serde_json::Map<String, serde_json::Value> =
        serde_json::json!({ "index": element, "text": "No match here now." }).as_object().cloned().unwrap_or_default();
    let state = harness.state_mut();
    let _ = state.registry.run("element.set_text", &mut state.session, &params);
    harness.run_steps(3);
    assert!(harness.state().editor.highlight.is_none(), "an edit under the match drops it");
}

#[test]
fn rename_onto_another_character_asks_to_merge() {
    let script = "INT. HUT - DAY\n\nNORA\nHello.\n\nEMMETT\nHi.\n";
    let (mut harness, _file) = harness(script);
    harness.state_mut().editor.pending_focus = Some((1, 0));
    harness.run_steps(3);
    harness.state_mut().queued.push("app.rename_character".into());
    harness.run_steps(3);
    if let Some(draft) = harness.state_mut().rename.as_mut() {
        draft.to = "Emmett".into();
    }
    harness.get_by_label("Rename").click();
    harness.run_steps(3);
    let error = harness.state().rename.as_ref().and_then(|d| d.error.clone());
    assert_eq!(error.as_deref(), Some("EMMETT is already a character. Merge NORA into EMMETT?"));
    assert_eq!(harness.state().session.script.elements[1].text, "NORA", "nothing changes before the writer agrees");
    harness.get_by_label("Merge").click();
    harness.run_steps(3);
    assert!(harness.state().rename.is_none());
    assert_eq!(harness.state().session.script.elements[1].text, "EMMETT");
}

#[test]
fn with_nothing_to_open_the_sample_is_shown_unsaved() {
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1280.0, 800.0))
        .with_max_steps(64)
        .build_eframe(|cc| StorytellerApp::without_menu(cc, None, None, Location::Memory, Default::default()));
    harness.run_steps(3);
    let app = harness.state();
    assert!(app.showing_sample(), "the sample opens");
    assert!(app.session.path.is_none(), "with no path, so Save never writes the bundled file");
    assert!(!app.session.dirty);
    assert!(app.session.script.elements.len() > 1000, "the whole of Field Station");
    assert_eq!(app.display_title(), "Field Station (sample)");
    assert_eq!(app.status, super::sample::SAMPLE_LINE);
    assert!(app.prefs.get().general.sample_seen, "shown once");
    assert!(app.prefs.get().view.check_spelling, "with checking on");
    assert!(
        app.editor.misspelled.is_empty(),
        "the showcase opens with no spelling underlines: {:?}",
        app.editor.misspelled.keys().take(5).collect::<Vec<_>>()
    );
    // A new script takes the sample's title and its line away.
    harness.state_mut().queued.push("file.new".into());
    harness.run_steps(3);
    assert_eq!(harness.state().display_title(), "Untitled");
    assert!(harness.state().status.is_empty(), "the sample's line goes with it: {:?}", harness.state().status);
}

#[test]
fn help_opens_the_sample_any_time() {
    let (mut harness, _file) = harness(SCRIPT);
    assert!(!harness.state().showing_sample(), "a file given at launch opens instead of the sample");
    assert!(harness.state().prefs.get().general.sample_seen, "and retires the first-launch sample unshown");
    harness.state_mut().queued.push("help.open_sample".into());
    harness.run_steps(3);
    assert!(harness.state().showing_sample(), "Help > Open Sample Script");
    assert!(harness.state().session.path.is_none());
}

#[test]
fn select_all_selects_the_paragraph_and_the_edit_menu_lists_the_clipboard() {
    let (mut harness, _file) = harness(SCRIPT);
    harness.state_mut().editor.pending_focus = Some((1, 0));
    harness.run_steps(3);
    harness.state_mut().queued.push("app.select_all".into());
    harness.run_steps(3);
    let id = egui::Id::new(("storyteller.element", 1));
    let range = egui::text_edit::TextEditState::load(&harness.ctx, id).and_then(|s| s.cursor.char_range());
    let (start, end) = range.map(|r| (r.primary.index.0.min(r.secondary.index.0), r.primary.index.0.max(r.secondary.index.0))).unwrap_or_default();
    assert_eq!((start, end), (0, "One.".chars().count()), "the whole paragraph is selected");
    assert!(harness.ctx.memory(|m| m.has_focus(id)), "and keeps the keyboard");
    let labels: Vec<String> = harness.state().menu_entries("Edit").into_iter().map(|e| e.label).collect();
    for wanted in ["Cut", "Copy", "Paste", "Select All"] {
        assert!(labels.iter().any(|l| l == wanted), "Edit has {wanted}: {labels:?}");
    }
    assert!(
        harness
            .state()
            .menu_entries("Edit")
            .iter()
            .filter(|e| ["app.cut", "app.copy", "app.paste", "app.select_all"].contains(&e.id.as_str()))
            .all(|e| e.shortcut.is_none()),
        "the keys stay with the text fields"
    );
}

#[test]
fn edit_cut_from_the_window_menu_cuts_the_selection() {
    let (mut harness, _file) = harness(SCRIPT);
    harness.state_mut().editor.pending_focus = Some((1, 0));
    harness.run_steps(3);
    harness.state_mut().queued.push("app.select_all".into());
    harness.run_steps(3);
    // Through the menu drawn in the window, as on Linux and Windows: the click takes the focus.
    harness.get_by_label("Edit").click();
    harness.run_steps(2);
    // The item shows its key beside the label (Cut ⌘X), so match the label's start.
    harness.get_by_label_contains("Cut").click();
    harness.run_steps(4);
    assert_eq!(harness.state().session.script.elements[1].text, "", "the selected paragraph is cut");
    let id = egui::Id::new(("storyteller.element", 1));
    assert!(harness.ctx.memory(|m| m.has_focus(id)), "and the paragraph has the keyboard again");
}

const SPELL_SCRIPT: &str = "INT. HUT - DAY\n\nNora taps the glsas. The seismograf is still.\n\nNORA\nOkay.\n";

#[test]
fn misspelled_words_are_underlined() {
    let (harness, _file) = harness(SPELL_SCRIPT);
    let marks = harness.state().editor.misspelled.get(&1).cloned().unwrap_or_default();
    let text = &harness.state().session.script.elements[1].text;
    let words: Vec<&str> = marks.iter().map(|&(s, e)| &text[s..e]).collect();
    assert_eq!(words, ["glsas", "seismograf"]);
    assert!(!harness.state().editor.misspelled.contains_key(&2), "a cue's name is known");
}

#[test]
fn the_spelling_dialog_changes_ignores_and_learns() {
    let (mut harness, _file) = harness(SPELL_SCRIPT);
    harness.state_mut().queued.push("app.spelling".into());
    harness.run_steps(3);
    let current = harness.state().spelling.as_ref().and_then(|d| d.current.clone()).map(|c| c.3);
    assert_eq!(current.as_deref(), Some("glsas"));
    assert_eq!(harness.state().spelling.as_ref().map(|d| d.change_to.clone()).as_deref(), Some("glass"));
    harness.get_by_label("Change").click();
    harness.run_steps(3);
    assert_eq!(harness.state().session.script.elements[1].text, "Nora taps the glass. The seismograf is still.");
    assert_eq!(harness.state().spelling.as_ref().and_then(|d| d.current.clone()).map(|c| c.3).as_deref(), Some("seismograf"));
    harness.get_by_label("Learn").click();
    harness.run_steps(3);
    assert_eq!(harness.state().prefs.get().general.learned_words, ["seismograf"], "learned words are kept in settings");
    assert_eq!(harness.state().spelling.as_ref().map(|d| d.status.clone()).as_deref(), Some("No misspellings."));
    assert!(harness.state().editor.misspelled.is_empty(), "nothing left to underline");
}

#[test]
fn enter_in_change_to_changes_and_ignore_is_kept_with_the_script() {
    let (mut harness, _file) = harness(SPELL_SCRIPT);
    harness.state_mut().queued.push("app.spelling".into());
    harness.run_steps(3);
    // The dialog takes the keyboard: Enter in Change to is Change.
    harness.key_press(egui::Key::Enter);
    harness.run_steps(3);
    assert_eq!(harness.state().session.script.elements[1].text, "Nora taps the glass. The seismograf is still.");
    let dirty_after_change = harness.state().session.dirty;
    assert!(dirty_after_change, "Change edits the script");
    harness.state_mut().session.dirty = false;
    harness.get_by_label("Ignore").click();
    harness.run_steps(3);
    assert!(harness.state().session.dirty, "Ignore is kept with the script, so there is something to save");
    assert_eq!(harness.state().spelling.as_ref().map(|d| d.status.clone()).as_deref(), Some("No misspellings."));
}

#[test]
fn the_word_being_typed_is_not_underlined_yet() {
    let (mut harness, _file) = harness(SPELL_SCRIPT);
    let end = SPELL_SCRIPT.lines().nth(2).map(|l| l.find("glsas").unwrap_or(0) + "glsas".len()).unwrap_or(0);
    harness.state_mut().session.caret = storyteller_engine::Caret { element: 1, offset: end };
    harness.run_steps(1);
    let marks = harness.state().editor.misspelled.get(&1).cloned().unwrap_or_default();
    let text = &harness.state().session.script.elements[1].text;
    let words: Vec<&str> = marks.iter().map(|&(s, e)| &text[s..e]).collect();
    assert_eq!(words, ["seismograf"], "the caret is at the end of glsas");
}

#[test]
fn open_sample_at_start() {
    // --check-for-updates is covered in sample.rs without running it: here it would reach the
    // network.
    let flags = super::StartFlags { open_sample: true, check_for_updates: false };
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1280.0, 800.0))
        .with_max_steps(64)
        .build_eframe(move |cc| StorytellerApp::without_menu(cc, None, None, Location::Memory, flags));
    harness.run_steps(3);
    assert!(harness.state().showing_sample(), "--open-sample opens the sample");
}

#[test]
fn a_scene_in_the_navigator_jumps_to_its_action() {
    let script = "INT. A - DAY\n\nOne.\n\nEXT. B - NIGHT\n\nNORA\nHello.\n\nShe waits.\n\nINT. C - DAY\n\nNORA\nOnly talk.\n\nINT. D - DAY\n";
    let (mut harness, _file) = harness(script);
    harness.state_mut().prefs.update(|p| p.view.navigator = true);
    harness.run_steps(3);
    let caret_after = |harness: &mut Harness<'static, StorytellerApp>, n: usize| {
        // The navigator paints its rows, so click a row where the app's own registry puts it.
        let id = format!("navigator.scene.{n}");
        let pos = harness.state().automation.find(&id).map(|w| w.rect.center()).unwrap_or_else(|| panic!("{id} is on screen"));
        let press = |pressed| egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE };
        harness.input_mut().events.extend([egui::Event::PointerMoved(pos), press(true)]);
        harness.step();
        harness.input_mut().events.push(press(false));
        harness.run_steps(4);
        harness.state().session.caret.element
    };
    let elements = |h: &Harness<'static, StorytellerApp>| h.state().session.script.elements.iter().map(|e| e.text.clone()).collect::<Vec<_>>();
    let texts = elements(&harness);
    let at = |text: &str| texts.iter().position(|t| t == text).unwrap_or(usize::MAX);
    assert_eq!(caret_after(&mut harness, 1), at("She waits."), "past the dialogue, to the scene's action");
    assert_eq!(caret_after(&mut harness, 0), at("One."));
    assert_eq!(caret_after(&mut harness, 2), at("NORA").max(at("Only talk.") - 1), "a scene with no action: the first line under the heading");
    assert_eq!(caret_after(&mut harness, 3), at("INT. D - DAY"), "an empty scene: its heading");
    let focused = harness.state().editor.focused;
    assert_eq!(focused, Some(at("INT. D - DAY")), "and the editor has the keyboard there");
    // Typing after a click goes straight into the scene's action.
    let waits = caret_after(&mut harness, 1);
    harness.input_mut().events.push(egui::Event::Text("Rain. ".into()));
    harness.run_steps(3);
    assert_eq!(harness.state().session.script.elements[waits].text, "Rain. She waits.");
}

#[test]
fn pasting_screenplay_text_into_a_line_makes_elements() {
    let (mut harness, _file) = harness("INT. HUT - DAY\n\nShe waits.\n");
    harness.state_mut().editor.pending_focus = Some((1, 0));
    harness.run_steps(3);
    harness.input_mut().events.push(egui::Event::Paste("EXT. RIDGE - DAWN\n\nNORA\nIt held.\n\n".into()));
    harness.run_steps(4);
    let kinds: Vec<storyteller_core::ElementKind> = harness.state().session.script.elements.iter().map(|e| e.kind).collect();
    use storyteller_core::ElementKind as K;
    assert_eq!(
        kinds,
        [K::SceneHeading, K::SceneHeading, K::Character, K::Dialogue, K::Action],
        "pasted at the start of the line: the elements, then the line"
    );
    assert_eq!(harness.state().session.script.elements[4].text, "She waits.");
    assert_eq!(harness.state().status, "Pasted 3 elements");
    // A paste within one paragraph stays an ordinary paste.
    harness.state_mut().editor.pending_focus = Some((4, 0));
    harness.run_steps(3);
    harness.input_mut().events.push(egui::Event::Paste("Still ".into()));
    harness.run_steps(3);
    assert_eq!(harness.state().session.script.elements[4].text, "Still She waits.");
    assert_eq!(harness.state().session.script.elements.len(), 5);
}

#[test]
fn copy_scene_puts_the_scene_on_the_clipboard() {
    let (mut harness, _file) = harness("INT. HUT - DAY\n\nOne.\n\nEXT. RIDGE - DAWN\n\nNORA\nIt held.\n");
    harness.state_mut().editor.pending_focus = Some((3, 0));
    harness.run_steps(3);
    harness.state_mut().queued.push("edit.copy_scene".into());
    harness.step();
    let copied: Vec<String> = harness
        .output()
        .platform_output
        .commands
        .iter()
        .filter_map(|c| if let egui::OutputCommand::CopyText(t) = c { Some(t.clone()) } else { None })
        .collect();
    assert_eq!(copied.len(), 1, "one copy");
    assert!(copied[0].starts_with("EXT. RIDGE - DAWN") && copied[0].contains("NORA\nIt held.") && !copied[0].contains("HUT"), "{copied:?}");
    harness.run_steps(2);
    assert_eq!(harness.state().status, "Copied the scene (3 elements)");
}

#[test]
fn go_to_by_typing_moves_the_highlight_and_the_page_tab_takes_the_keyboard() {
    let (mut harness, _file) = harness(SCRIPT);
    harness.state_mut().editor.pending_focus = Some((1, 0));
    harness.run_steps(3);
    assert_eq!(harness.state().current_scene(), 1);
    open_go_to(&mut harness);
    // The Page tab, clicked: the number field has the keyboard, so typing goes in.
    harness.get_by_label("Page").click();
    harness.run_steps(3);
    harness.get_by_label("Scene").click();
    harness.run_steps(3);
    harness.input_mut().events.push(egui::Event::Text("5".into()));
    harness.run_steps(2);
    assert_eq!(harness.state().go_to.as_ref().map(|d| d.text.clone()).as_deref(), Some("5"), "typing after a tab click reaches the field");
    harness.key_press(egui::Key::Enter);
    harness.run_steps(4);
    assert!(harness.state().go_to.is_none());
    let caret = harness.state().session.caret.element;
    assert_eq!(caret, 4, "scene 5's heading");
    assert_eq!(harness.state().current_scene(), 3, "the navigator highlights the scene gone to");
    assert_eq!(harness.state().editor.focused, Some(4), "and the editor has the keyboard there");
}

#[test]
fn cut_copy_paste_keys_are_listed_but_left_to_the_text_fields() {
    let (harness, _file) = harness(SCRIPT);
    let rows = harness.state().shortcut_rows(&harness.ctx);
    for label in ["Cut", "Copy", "Paste", "Select All"] {
        let row = rows.iter().find(|r| r.label == label).unwrap_or_else(|| panic!("Keyboard Shortcuts lists {label}"));
        assert!(!row.keys.is_empty(), "{label} shows its keys");
    }
    let bound: Vec<&str> = harness.state().shortcuts.iter().map(|(_, id)| *id).collect();
    for id in ["app.cut", "app.copy", "app.paste", "app.select_all"] {
        assert!(!bound.contains(&id), "{id} is not bound: the text fields keep its keys");
    }
}

#[test]
fn new_tv_script_asks_the_shape_and_remembers_it() {
    let (mut harness, _file) = harness(SCRIPT);
    harness.state_mut().queued.push("file.new_tv".into());
    harness.run_steps(3);
    assert_eq!(harness.state().new_tv, Some(storyteller_core::model::TvShape::OneHour), "one-hour first, the default");
    harness.get_by_label_contains("Half-hour").click();
    harness.run_steps(2);
    harness.get_by_label("Create").click();
    harness.run_steps(4);
    assert!(harness.state().new_tv.is_none(), "the dialog closes");
    let acts: Vec<String> =
        harness.state().session.script.elements.iter().filter(|e| e.kind == storyteller_core::ElementKind::NewAct).map(|e| e.text.clone()).collect();
    assert_eq!(acts, ["COLD OPEN", "ACT ONE", "ACT TWO", "ACT THREE", "TAG"]);
    assert_eq!(harness.state().session.caret.element, 1, "ready to write the cold open's first scene");
    assert_eq!(harness.state().prefs.get().general.tv_shape, storyteller_core::model::TvShape::HalfHour, "and the choice is remembered");
    // Next time it offers half-hour first.
    harness.state_mut().queued.push("file.new_tv".into());
    harness.run_steps(3);
    assert_eq!(harness.state().new_tv, Some(storyteller_core::model::TvShape::HalfHour));
    harness.key_press(egui::Key::Escape);
    harness.run_steps(2);
    assert!(harness.state().new_tv.is_none(), "Escape closes it");
}

#[test]
fn acts_head_their_scenes_in_the_navigator_and_go_to_finds_them() {
    let script = "> TEASER <\n\nINT. HUT - NIGHT\n\nRain.\n\n> END OF TEASER <\n\n> ACT ONE <\n\nEXT. RIDGE - DAWN\n\nWind.\n\n> END OF ACT ONE <\n\n> TAG <\n";
    let (mut harness, _file) = harness(script);
    harness.state_mut().prefs.update(|p| p.view.navigator = true);
    harness.run_steps(3);
    let rect = |harness: &Harness<'static, StorytellerApp>, id: &str| {
        harness.state().automation.find(id).map(|w| w.rect).unwrap_or_else(|| panic!("{id} is on screen"))
    };
    // Each act sits above its scenes; the TAG, with none yet, still shows at the end.
    assert!(rect(&harness, "navigator.act.0").bottom() <= rect(&harness, "navigator.scene.0").top());
    assert!(rect(&harness, "navigator.scene.0").bottom() <= rect(&harness, "navigator.act.1").top());
    assert!(rect(&harness, "navigator.act.1").bottom() <= rect(&harness, "navigator.scene.1").top());
    assert!(rect(&harness, "navigator.scene.1").bottom() <= rect(&harness, "navigator.act.2").top());
    let click = |harness: &mut Harness<'static, StorytellerApp>, id: &str| {
        let pos = rect(harness, id).center();
        let press = |pressed| egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE };
        harness.input_mut().events.extend([egui::Event::PointerMoved(pos), press(true)]);
        harness.step();
        harness.input_mut().events.push(press(false));
        harness.run_steps(4);
        let s = &harness.state().session;
        s.script.elements[s.caret.element].text.clone()
    };
    assert_eq!(click(&mut harness, "navigator.act.1"), "Wind.", "an act click goes into its first scene's action");
    assert_eq!(click(&mut harness, "navigator.act.2"), "TAG", "an act with no scene: its own line");

    open_go_to(&mut harness);
    if let Some(draft) = harness.state_mut().go_to.as_mut() {
        draft.mode = super::GoToMode::Act;
        draft.text = "teaser".into();
    }
    harness.run_steps(2);
    harness.get_by_label("Go").click();
    harness.run_steps(3);
    assert!(harness.state().go_to.is_none(), "the dialog closes on success");
    let s = &harness.state().session;
    assert_eq!(s.script.elements[s.caret.element].text, "TEASER");
}

#[test]
fn typing_on_an_act_line_brings_its_end_of_act_line() {
    let (mut harness, _file) = harness("> ACT ONE <\n\nINT. HUT - DAY\n\nRain.\n");
    harness.state_mut().editor.pending_focus = Some((0, "ACT ONE".len()));
    harness.run_steps(3);
    harness.input_mut().events.push(egui::Event::Text(" B".into()));
    harness.run_steps(4);
    let texts: Vec<String> = harness.state().session.script.elements.iter().map(|e| e.text.clone()).collect();
    assert_eq!(texts, ["ACT ONE B", "INT. HUT - DAY", "Rain.", "END OF ACT ONE B"]);
    assert_eq!(harness.state().session.caret.element, 0, "the caret stays on the act line");
}

#[test]
fn new_tv_script_offers_a_multi_camera_half_hour() {
    let (mut harness, _file) = harness(SCRIPT);
    harness.state_mut().queued.push("file.new_tv".into());
    harness.run_steps(3);
    harness.get_by_label_contains("Multi-camera").click();
    harness.run_steps(2);
    harness.get_by_label("Create").click();
    harness.run_steps(4);
    let session = &harness.state().session;
    assert!(session.script.is_lettered(), "lettered scenes");
    assert_eq!(session.layout.dialogue.spacing, 2, "double-spaced dialogue");
    assert_eq!(harness.state().prefs.get().general.tv_shape, storyteller_core::model::TvShape::MultiCam);
}

#[test]
fn format_bold_styles_the_selection_and_keeps_it_selected() {
    let (mut harness, _file) = harness("INT. HUT - DAY\n\nShe runs home.\n");
    harness.state_mut().editor.pending_focus = Some((1, 0));
    harness.run_steps(3);
    // Select "runs" in the text field, as a drag would.
    let id = egui::Id::new(("storyteller.element", 1));
    let mut state = egui::text_edit::TextEditState::load(&harness.ctx, id).unwrap_or_default();
    state.cursor.set_char_range(Some(egui::text::CCursorRange::two(egui::text::CCursor::new(4), egui::text::CCursor::new(8))));
    state.store(&harness.ctx, id);
    harness.run_steps(2);
    harness.state_mut().queued.push("app.bold".into());
    harness.run_steps(4);
    assert_eq!(harness.state().session.script.elements[1].text, "She **runs** home.");
    let selected = harness.state().editor.selection.map(|(i, r)| (i, usize::from(r.primary.index).min(r.secondary.index.into())));
    assert_eq!(selected, Some((1, 6)), "the word stays selected, inside the markers");
    // With no selection, the word the caret is in.
    let mut state = egui::text_edit::TextEditState::load(&harness.ctx, id).unwrap_or_default();
    let at = egui::text::CCursor::new("She **runs** ho".chars().count());
    state.cursor.set_char_range(Some(egui::text::CCursorRange::one(at)));
    state.store(&harness.ctx, id);
    harness.run_steps(2);
    harness.state_mut().queued.push("app.underline".into());
    harness.run_steps(4);
    assert_eq!(harness.state().session.script.elements[1].text, "She **runs** _home_.");
}

#[test]
fn close_leaves_an_empty_script_and_open_recent_brings_it_back() {
    let (mut harness, file) = harness(SCRIPT);
    let path = file.path().display().to_string();
    harness.state_mut().prefs.update(|p| p.push_recent(&path));
    harness.run_steps(2);
    let labels: Vec<String> = harness.state().menu_entries("File").into_iter().map(|e| e.label).collect();
    let open = labels.iter().position(|l| l == "Open…").unwrap();
    assert_eq!(labels.get(open + 1).map(String::as_str), Some("Open Recent"), "{labels:?}");
    assert!(labels.iter().any(|l| l == "Close"));

    harness.state_mut().queued.push("app.close".into());
    harness.run_steps(3);
    assert!(harness.state().session.path.is_none(), "the script is closed");
    assert!(harness.state().session.script.elements.iter().all(|e| e.text.is_empty()), "and an empty one is open");

    let recent = harness.state().recent_items();
    assert_eq!(recent.first().map(|(id, _)| id.as_str()), Some("app.open_recent.0"));
    let name = file.path().file_name().unwrap().to_string_lossy().into_owned();
    assert!(recent[0].1.starts_with(&name), "{:?} names the file", recent[0].1);
    harness.state_mut().queued.push("app.open_recent.0".into());
    harness.run_steps(3);
    assert_eq!(harness.state().session.path.as_deref(), Some(file.path()), "Open Recent opens it again");

    harness.state_mut().queued.push("app.clear_recent".into());
    harness.run_steps(2);
    assert!(harness.state().recent_items().is_empty());
    assert!(!harness.state().menu_entries("File").into_iter().find(|e| e.label == "Open Recent").unwrap().enabled, "an empty list is greyed out");
}
