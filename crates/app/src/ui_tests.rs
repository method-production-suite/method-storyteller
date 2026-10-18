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
