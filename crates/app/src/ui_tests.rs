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
        .build_eframe(move |cc| StorytellerApp::without_menu(cc, Some(path), None, Location::Memory));
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
        draft.page = true;
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
