#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

use serde_json::{Map, Value, json};
use storyteller_core::ElementKind as K;
use storyteller_engine::control::{Headless, handle_line};
use storyteller_engine::{Registry, Session};

fn run(registry: &Registry, session: &mut Session, id: &str, params: Value) -> Value {
    let params: Map<String, Value> = params.as_object().cloned().unwrap_or_default();
    serde_json::to_value(registry.run(id, session, &params).unwrap_or_else(|e| panic!("{id}: {e}"))).unwrap()
}

fn kinds(session: &Session) -> Vec<K> {
    session.script.elements.iter().map(|e| e.kind).collect()
}

/// Type a scene the way a writer would: text, Enter, Tab.
#[test]
fn final_draft_flow() {
    let registry = Registry::new();
    let mut s = Session::default();
    let type_text = |s: &mut Session, text: &str| {
        let index = s.caret.element;
        let current = s.script.elements[index].text.clone();
        let (head, tail) = current.split_at(s.caret.offset);
        let new_text = format!("{head}{text}{tail}");
        run(&registry, s, "element.set_text", json!({ "text": new_text, "typing": true }));
        s.caret.offset = (head.len() + text.len()).min(s.script.elements[index].text.len());
    };
    type_text(&mut s, "int. kitchen - night");
    run(&registry, &mut s, "flow.enter", json!({}));
    type_text(&mut s, "Rain on the window.");
    run(&registry, &mut s, "flow.enter", json!({}));
    run(&registry, &mut s, "flow.tab", json!({})); // empty action → character
    type_text(&mut s, "mara");
    run(&registry, &mut s, "flow.enter", json!({})); // → dialogue
    run(&registry, &mut s, "flow.tab", json!({})); // dialogue → parenthetical "()"
    assert_eq!(s.current().unwrap().text, "()");
    assert_eq!(s.caret.offset, 1);
    type_text(&mut s, "whispering");
    s.caret.offset = s.current().unwrap().text.len() - 1;
    run(&registry, &mut s, "flow.enter", json!({})); // → dialogue
    type_text(&mut s, "It's starting again.");
    run(&registry, &mut s, "flow.enter", json!({})); // dialogue → action
    // Enter on the empty line asks for the element picker and changes nothing.
    let picker = run(&registry, &mut s, "flow.enter", json!({}));
    assert_eq!(picker["value"]["picker"], json!(true));
    assert_eq!(s.current().unwrap().kind, K::Action);
    type_text(&mut s, "She waits.");
    assert_eq!(kinds(&s), [K::SceneHeading, K::Action, K::Character, K::Parenthetical, K::Dialogue, K::Action]);
    let texts: Vec<&str> = s.script.elements.iter().map(|e| e.text.as_str()).collect();
    assert_eq!(texts, ["INT. KITCHEN - NIGHT", "Rain on the window.", "MARA", "(whispering)", "It's starting again.", "She waits."]);

    // Enter in the middle splits; Backspace at the start joins again.
    s.caret = storyteller_engine::Caret { element: 1, offset: 5 };
    run(&registry, &mut s, "flow.enter", json!({}));
    assert_eq!(s.script.elements[1].text, "Rain");
    assert_eq!(s.script.elements[2].text, "on the window.");
    run(&registry, &mut s, "element.merge_previous", json!({}));
    assert_eq!(s.script.elements[1].text, "Rainon the window.");

    // Undo walks it all back; typing merged into single steps.
    let mut steps = 0;
    while s.can_undo() {
        run(&registry, &mut s, "edit.undo", json!({}));
        steps += 1;
    }
    assert!(steps < 25, "{steps} undo steps");
    assert_eq!(s.script.elements.len(), 1);
    run(&registry, &mut s, "edit.redo", json!({}));
}

#[test]
fn format_shortcuts_and_scene_numbers() {
    let registry = Registry::new();
    let mut s = Session::default();
    run(&registry, &mut s, "element.set_text", json!({ "text": "ext. sea - day" }));
    run(&registry, &mut s, "element.insert", json!({ "kind": "action", "text": "Waves." }));
    run(&registry, &mut s, "convert.transition", json!({}));
    assert_eq!(s.current().unwrap().kind, K::Transition);
    assert_eq!(s.current().unwrap().text, "WAVES.");
    run(&registry, &mut s, "element.insert", json!({ "kind": "scene_heading", "text": "INT. BOAT - DAY" }));
    run(&registry, &mut s, "scene.number", json!({}));
    let numbers: Vec<_> = s.script.elements.iter().filter_map(|e| e.scene_number.clone()).collect();
    assert_eq!(numbers, ["1", "2"]);
    let suggestions = run(&registry, &mut s, "autocomplete.suggest", json!({ "index": 2 }));
    assert_eq!(suggestions["value"], json!([]));
    assert!(registry.run("nope", &mut s, &Map::new()).is_err());
    assert!(registry.run("element.set_text", &mut s, &Map::new()).is_err());
    assert!(registry.run("element.set_text", &mut s, json!({"index": 99, "text": "x"}).as_object().unwrap()).is_err());
    assert!(registry.run("file.save", &mut s, &Map::new()).is_err(), "save without a path is disabled");
}

#[test]
fn save_open_export_round_trip() {
    let dir = tempfile::tempdir().unwrap();
    let registry = Registry::new();
    let mut s = Session::default();
    run(&registry, &mut s, "element.set_text", json!({ "text": "INT. ATTIC - NIGHT" }));
    run(&registry, &mut s, "element.insert", json!({ "kind": "action", "text": "Dust everywhere." }));
    run(&registry, &mut s, "title_page.set", json!({ "key": "Title", "value": "Attic" }));
    for name in ["attic.fountain", "attic.fdx", "attic.osf", "attic.fadein"] {
        let path = dir.path().join(name);
        run(&registry, &mut s, "file.save_as", json!({ "path": path }));
        assert!(!s.dirty);
        let original = s.script.clone();
        let mut opened = Session::default();
        run(&registry, &mut opened, "file.open", json!({ "path": path }));
        // An opened .fdx also keeps Final Draft data for writing back (scene properties
        // included); compare the script itself.
        let content = |s: &storyteller_core::Script| {
            (s.title_page.clone(), s.elements.iter().map(|e| (e.kind, e.text.clone(), e.scene_number.clone(), e.dual)).collect::<Vec<_>>())
        };
        assert_eq!(content(&opened.script), content(&original), "{name}");
    }
    let pdf = dir.path().join("attic.pdf");
    run(&registry, &mut s, "file.export_pdf", json!({ "path": pdf }));
    assert!(std::fs::read(&pdf).unwrap().starts_with(b"%PDF-"));
    // Show Revisions is a view choice the export follows, unless it names its own.
    assert_eq!(run(&registry, &mut s, "view.show_revisions", json!({ "show": "active" }))["value"]["show"], "active");
    assert!(registry.run("view.show_revisions", &mut s, json!({ "show": "some" }).as_object().unwrap()).is_err());
    assert_eq!(run(&registry, &mut s, "view.page_colours", json!({}))["value"]["show"], true);
    run(&registry, &mut s, "file.export_pdf", json!({ "path": pdf, "revisions": "none", "colour": true }));
    assert!(std::fs::read(&pdf).unwrap().starts_with(b"%PDF-"));
    assert!(registry.run("file.export_pdf", &mut s, json!({ "path": pdf, "revisions": "blue" }).as_object().unwrap()).is_err());
    run(&registry, &mut s, "file.export_pdf", json!({ "path": pdf, "pages": "1", "clean": true }));
    let missing = registry.run("file.export_pdf", &mut s, json!({ "path": pdf, "pages": "999" }).as_object().unwrap()).unwrap_err();
    assert!(missing.to_string().contains("there is no page 999"), "{missing}");
    // Opening something hostile fails cleanly.
    let junk = dir.path().join("junk.fdx");
    std::fs::write(&junk, b"<FinalDraft><Content></Oops>").unwrap();
    assert!(registry.run("file.open", &mut s, json!({ "path": junk }).as_object().unwrap()).is_err());
    std::fs::write(&junk, [0xff, 0xfe, 0x00]).unwrap();
    assert!(registry.run("file.open", &mut s, json!({ "path": junk }).as_object().unwrap()).is_err());
}

#[test]
fn control_protocol() {
    let registry = Registry::new();
    let mut s = Session::default();
    let ask = |s: &mut Session, line: &str| -> Value { serde_json::from_str(&handle_line(line, s, &registry, &mut Headless)).unwrap() };
    assert_eq!(ask(&mut s, r#"{"id":1,"method":"app.ping"}"#)["result"], "pong");
    let list = ask(&mut s, r#"{"id":2,"method":"command.list"}"#);
    assert!(list["result"].as_array().unwrap().iter().any(|c| c["id"] == "flow.enter"));
    let reply = ask(&mut s, r#"{"id":3,"method":"command.run","params":{"command":"element.set_text","params":{"text":"int. hall - day"}}}"#);
    assert!(reply.get("error").is_none(), "{reply}");
    let doc = ask(&mut s, r#"{"id":4,"method":"doc.inspect"}"#);
    assert_eq!(doc["result"]["elements"][0]["text"], "INT. HALL - DAY");
    assert_eq!(doc["result"]["pages"], 1);
    let pages = ask(&mut s, r#"{"id":5,"method":"doc.paginate","params":{"text":true}}"#);
    assert!(pages["result"]["text"].as_str().unwrap().contains("INT. HALL - DAY"));
    assert!(ask(&mut s, r#"{"id":6,"method":"ui.screenshot"}"#)["error"].is_string());
    assert!(ask(&mut s, "not json")["error"].is_string());
    assert!(ask(&mut s, r#"{"id":7}"#)["error"].is_string());
    assert!(ask(&mut s, r#"{"id":8,"method":"command.run","params":{"command":"edit.redo"}}"#)["error"].is_string());
}

#[test]
fn control_server_over_tcp() {
    use std::io::{BufRead, BufReader, Write};
    let port = 47_000 + (std::process::id() % 1000) as u16;
    let Ok(receiver) = storyteller_engine::control::serve(port, || {}) else { return };
    let owner = std::thread::spawn(move || {
        let registry = Registry::new();
        let mut session = Session::default();
        if let Ok(request) = receiver.recv() {
            let _ = request.reply.send(handle_line(&request.line, &mut session, &registry, &mut Headless));
        }
    });
    let mut stream = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
    writeln!(stream, r#"{{"id":"x","method":"app.ping"}}"#).unwrap();
    let mut line = String::new();
    BufReader::new(stream).read_line(&mut line).unwrap();
    assert_eq!(serde_json::from_str::<Value>(&line).unwrap()["result"], "pong");
    owner.join().unwrap();
}

#[test]
fn tab_starts_the_next_element_and_shortcuts_insert() {
    let registry = Registry::new();
    let mut s = Session::default();
    run(&registry, &mut s, "element.set_text", json!({ "text": "int.", "typing": true }));
    assert_eq!(s.current().unwrap().kind, K::SceneHeading, "a scene heading stays one");
    run(&registry, &mut s, "element.set_kind", json!({ "kind": "action" }));
    // Turned back into action, it stays action as the writer types on; cleared and retyped, it
    // becomes a heading again.
    run(&registry, &mut s, "element.set_text", json!({ "text": "INT. ", "typing": true }));
    assert_eq!(s.current().unwrap().kind, K::Action, "a heading turned back into action stays action");
    run(&registry, &mut s, "element.set_text", json!({ "text": "", "typing": true }));
    run(&registry, &mut s, "element.set_text", json!({ "text": "ext.", "typing": true }));
    assert_eq!((s.current().unwrap().kind, s.current().unwrap().text.as_str()), (K::SceneHeading, "EXT."), "typing ext. in action makes a heading");
    run(&registry, &mut s, "edit.undo", json!({}));
    assert_eq!((s.current().unwrap().kind, s.current().unwrap().text.as_str()), (K::Action, "ext."), "one undo turns it back into action");
    run(&registry, &mut s, "element.set_text", json!({ "text": "Est.", "typing": true }));
    assert_eq!(s.current().unwrap().kind, K::Action, "Est. stays action");

    // A whole heading arriving at once (fast keys, a paste) after a speech becomes a heading,
    // and the next speech by the same character starts afresh, without (CONT'D).
    let mut s = Session::new(storyteller_core::fountain::parse("INT. HUT - DAY\n\nNORA\nHello.\n\nShe waits.\n").unwrap());
    let action = s.script.elements.len() - 1;
    run(&registry, &mut s, "element.set_text", json!({ "index": action, "text": "", "typing": true }));
    run(&registry, &mut s, "element.set_text", json!({ "index": action, "text": "EXT. RIDGE - DAWN", "typing": true }));
    assert_eq!(s.script.elements[action].kind, K::SceneHeading, "EXT. RIDGE - DAWN typed in one go");
    s.script.elements.push(storyteller_core::Element::new(K::Character, "NORA"));
    s.script.elements.push(storyteller_core::Element::new(K::Dialogue, "Again."));
    assert!(!storyteller_core::paginate::auto_contd_at(&s.script, s.script.elements.len() - 2), "no (CONT'D) across a scene heading");

    let mut s = Session::default();
    run(&registry, &mut s, "element.set_kind", json!({ "kind": "character" }));
    run(&registry, &mut s, "flow.tab", json!({}));
    assert_eq!(s.current().unwrap().kind, K::Transition, "Tab in an empty cue gives a transition");
    run(&registry, &mut s, "element.set_kind", json!({ "kind": "character" }));
    run(&registry, &mut s, "element.set_text", json!({ "text": "ANNA" }));
    run(&registry, &mut s, "flow.tab", json!({}));
    assert_eq!(kinds(&s), [K::Character, K::Parenthetical], "Tab after a cue starts a parenthetical");
    assert_eq!((s.current().unwrap().text.as_str(), s.caret.offset), ("()", 1));

    run(&registry, &mut s, "element.set_text", json!({ "text": "(quietly)" }));
    run(&registry, &mut s, "format.shot", json!({}));
    assert_eq!(kinds(&s), [K::Character, K::Parenthetical, K::Shot], "a shortcut after text starts a new element");
    run(&registry, &mut s, "convert.action", json!({ "index": 1 }));
    assert_eq!(kinds(&s), [K::Character, K::Action, K::Shot], "the change-to shortcut converts in place");
}

/// Go To lands on a scene by printed number or position, and on a page's first element.
#[test]
fn go_to_scene_and_page() {
    let registry = Registry::new();
    let long = "Action that fills the page. ".repeat(120);
    let text = format!("INT. A - DAY #4#\n\n{long}\n\nEXT. B - NIGHT #4A#\n\nShort.\n\nINT. C - DAY #5#\n\nEnd.\n");
    let mut s = Session::new(storyteller_core::fountain::parse(&text).unwrap());
    let go = |s: &mut Session, params: Value| {
        let params: Map<String, Value> = params.as_object().cloned().unwrap_or_default();
        registry.run("nav.go_to", s, &params).map(|_| s.caret.element)
    };
    assert_eq!(go(&mut s, json!({ "scene": "4a" })).unwrap(), 2);
    assert_eq!(go(&mut s, json!({ "scene": "4" })).unwrap(), 0);
    // Numbered: a missing number is an error, never the scene in that position.
    assert!(go(&mut s, json!({ "scene": "3" })).is_err());
    assert!(go(&mut s, json!({ "scene": "9" })).is_err());
    // Unnumbered: by position.
    let mut plain =
        Session::new(storyteller_core::fountain::parse("INT. A - DAY\n\nOne.\n\nEXT. B - NIGHT\n\nTwo.\n\nINT. C - DAY\n\nThree.\n").unwrap());
    assert_eq!(go(&mut plain, json!({ "scene": "3" })).unwrap(), 4);
    assert!(go(&mut plain, json!({ "scene": "4" })).is_err());
    assert_eq!(go(&mut s, json!({ "page": 1 })).unwrap(), 0);
    let pages = s.paginated().page_count();
    assert!(pages >= 2, "{pages} pages");
    // Page 2 starts inside the long action, which runs on from page 1.
    let element = go(&mut s, json!({ "page": 2 })).unwrap();
    assert!(element <= 2, "{element}");
    assert!(go(&mut s, json!({ "page": 0 })).is_err());
    assert!(go(&mut s, json!({ "page": pages + 1 })).is_err());
    assert!(go(&mut s, json!({})).is_err());
}

/// Find steps through matches and wraps; Replace changes one; Replace All is one undo step.
#[test]
fn find_and_replace() {
    let registry = Registry::new();
    let script = "INT. HUT - DAY\n\nNora taps the glass. nora waits.\n\nNORA\nThe glass is still.\n\nEXT. FLATS - DAY\n\nGlassy ground.\n";
    let mut s = Session::new(storyteller_core::fountain::parse(script).unwrap());
    let next = |s: &mut Session, extra: Value| {
        let mut params = json!({ "text": "nora" }).as_object().cloned().unwrap();
        params.extend(extra.as_object().cloned().unwrap_or_default());
        serde_json::to_value(registry.run("find.next", s, &params).unwrap()).unwrap()["value"].clone()
    };
    let first = next(&mut s, json!({}));
    assert_eq!(
        (first["element"].as_u64(), first["start"].as_u64(), first["number"].as_u64(), first["total"].as_u64()),
        (Some(1), Some(0), Some(1), Some(3))
    );
    let second = next(&mut s, json!({}));
    assert_eq!((second["element"].as_u64(), second["start"].as_u64()), (Some(1), Some(21)));
    let third = next(&mut s, json!({}));
    assert_eq!(third["element"].as_u64(), Some(2));
    let wrapped = next(&mut s, json!({}));
    assert_eq!(wrapped["number"].as_u64(), Some(1));
    // Case, and one element type.
    assert_eq!(next(&mut s, json!({ "case": true, "text": "NORA" }))["element"].as_u64(), Some(2));
    assert_eq!(next(&mut s, json!({ "kind": "character" }))["total"].as_u64(), Some(1));
    // Whole words: "glass" but not "Glassy".
    assert_eq!(next(&mut s, json!({ "text": "glass", "whole_word": true }))["total"].as_u64(), Some(2));
    assert_eq!(next(&mut s, json!({ "text": "missing" }))["total"].as_u64(), Some(0));

    // Replace one, at an explicit match; a cue stays in capitals.
    run(&registry, &mut s, "find.replace", json!({ "text": "nora", "replacement": "Emma", "index": 2, "start": 0 }));
    assert_eq!(s.script.elements[2].text, "EMMA");
    let none = run(&registry, &mut s, "find.replace", json!({ "text": "nora", "replacement": "Emma", "index": 2, "start": 0 }));
    assert_eq!(none["value"]["replaced"], json!(false));

    // Replace All, then one Undo puts every match back.
    let before = s.script.elements[1].text.clone();
    let all = run(&registry, &mut s, "find.replace_all", json!({ "text": "glass", "replacement": "window", "whole_word": true }));
    assert_eq!(all["value"]["count"], json!(2));
    assert_eq!(s.script.elements[1].text, "Nora taps the window. nora waits.");
    assert_eq!(s.script.elements[3].text, "The window is still.");
    assert_eq!(s.script.elements[5].text, "Glassy ground.");
    run(&registry, &mut s, "edit.undo", json!({}));
    assert_eq!(s.script.elements[1].text, before);
    assert_eq!(s.script.elements[3].text, "The glass is still.");

    // A Replace All that changes nothing is no edit: no undo step, the script stays clean.
    s.dirty = false;
    let undoable = s.can_undo();
    let same = run(&registry, &mut s, "find.replace_all", json!({ "text": "glass", "replacement": "glass", "whole_word": true }));
    assert_eq!(same["value"]["count"], json!(0));
    assert!(!s.dirty);
    assert_eq!(s.can_undo(), undoable);
}

/// Rename a character: cues keep their extensions; with `everywhere`, mentions follow in their
/// own case; one Undo puts it all back.
#[test]
fn rename_character() {
    let registry = Registry::new();
    let script = "INT. HUT - DAY\n\nNORA taps the glass. Nora's hands shake.\n\nNORA\nHello.\n\nNORA (V.O.)\nStill here.\n\nEMMETT\nNora?\n\nNORAH\nWrong one.\n";
    let mut s = Session::new(storyteller_core::fountain::parse(script).unwrap());
    let original = s.script.clone();
    let cues = run(&registry, &mut s, "character.rename", json!({ "from": "nora", "to": "Emma" }));
    assert_eq!(cues["value"]["cues"], json!(2));
    let texts: Vec<&str> = s.script.elements.iter().map(|e| e.text.as_str()).collect();
    assert_eq!(texts[2], "EMMA");
    assert_eq!(texts[4], "EMMA (V.O.)");
    assert_eq!(texts[1], "NORA taps the glass. Nora's hands shake.", "cues only by default");
    assert_eq!(texts[8], "NORAH");
    run(&registry, &mut s, "edit.undo", json!({}));
    assert_eq!(s.script, original);

    let all = run(&registry, &mut s, "character.rename", json!({ "from": "NORA", "to": "Emma", "everywhere": true }));
    assert_eq!((all["value"]["cues"].clone(), all["value"]["mentions"].clone()), (json!(2), json!(3)));
    let texts: Vec<&str> = s.script.elements.iter().map(|e| e.text.as_str()).collect();
    assert_eq!(texts[1], "EMMA taps the glass. Emma's hands shake.");
    assert_eq!(texts[7], "Emma?");
    assert_eq!(texts[8], "NORAH", "a different name is left alone");
    run(&registry, &mut s, "edit.undo", json!({}));
    assert_eq!(s.script, original, "one Undo reverses the rename");

    // The new name typed in capitals still takes each mention's own case.
    run(&registry, &mut s, "character.rename", json!({ "from": "NORA", "to": "EMMA", "everywhere": true }));
    assert_eq!(s.script.elements[1].text, "EMMA taps the glass. Emma's hands shake.");
    run(&registry, &mut s, "edit.undo", json!({}));

    // Only capitalised mentions: a name that is also a word leaves the word alone.
    let words = "INT. HUT - DAY\n\nWill says he will. Don't.\n\nWILL\nI will.\n\nDON\nWe don't.\n";
    let mut w = Session::new(storyteller_core::fountain::parse(words).unwrap());
    run(&registry, &mut w, "character.rename", json!({ "from": "WILL", "to": "Theo", "everywhere": true }));
    assert_eq!(w.script.elements[1].text, "Theo says he will. Don't.");
    assert_eq!(w.script.elements[3].text, "I will.");
    run(&registry, &mut w, "character.rename", json!({ "from": "DON", "to": "Ray", "everywhere": true }));
    assert_eq!(w.script.elements[1].text, "Theo says he will. Don't.", "DON is not in Don't");

    // Renaming onto another character's name asks first; brackets and ^ are refused.
    let refuse = |s: &mut Session, params: Value| {
        let params: Map<String, Value> = params.as_object().cloned().unwrap_or_default();
        registry.run("character.rename", s, &params).err().map(|e| e.to_string())
    };
    let merge = refuse(&mut s, json!({ "from": "NORA", "to": "Emmett" })).unwrap_or_default();
    assert!(merge.contains("EMMETT is already a character"), "{merge}");
    assert!(refuse(&mut s, json!({ "from": "NORA", "to": "Emma (V.O.)" })).is_some());
    assert!(refuse(&mut s, json!({ "from": "NORA", "to": "Emma^" })).is_some());
    let merged = run(&registry, &mut s, "character.rename", json!({ "from": "NORA", "to": "Emmett", "merge": true }));
    assert_eq!(merged["value"]["cues"], json!(2));
    run(&registry, &mut s, "edit.undo", json!({}));

    let names = run(&registry, &mut s, "character.list", json!({}));
    assert_eq!(names["value"][0], json!(["NORA", 2]));
    let params: Map<String, Value> = json!({ "from": "NOBODY", "to": "X" }).as_object().cloned().unwrap();
    assert!(registry.run("character.rename", &mut s, &params).is_err());
}

/// Scene lengths are cached with the pagination and refreshed by an edit.
#[test]
fn scene_lengths_follow_edits() {
    let registry = Registry::new();
    let script = storyteller_core::fountain::parse("INT. A - DAY\n\nShort.\n\nEXT. B - NIGHT\n\nAlso short.\n").unwrap();
    let mut s = Session::new(script);
    let before: Vec<(usize, u32)> = s.scene_lengths().iter().map(|l| (l.element, l.eighths)).collect();
    assert_eq!(before.len(), 2);
    assert_eq!(before[0].0, 0);
    assert_eq!(before[1].0, 2);
    let long = "A long line of action that goes on. ".repeat(60);
    s.caret.element = 1;
    run(&registry, &mut s, "element.set_text", json!({ "text": long }));
    let after = s.scene_lengths()[0].eighths;
    assert!(after > before[0].1, "{after} should be longer than {}", before[0].1);
}

/// File errors carry their kind from the error's type, so the app can word them without
/// reading the operating system's text.
#[test]
fn file_errors_carry_their_kind() {
    use storyteller_engine::files::FileProblem;
    let registry = Registry::new();
    let dir = tempfile::tempdir().expect("temp dir");
    let problem = |session: &mut Session, id: &str, path: std::path::PathBuf| {
        let mut params = Map::new();
        params.insert("path".into(), json!(path.display().to_string()));
        registry.run(id, session, &params).err().and_then(|e| e.file_problem())
    };
    let mut session = Session::default();
    assert_eq!(problem(&mut session, "file.open", dir.path().join("missing.fountain")), Some(FileProblem::NotFound));
    let pdf = dir.path().join("draft.pdf");
    std::fs::write(&pdf, b"%PDF-1.7\n").expect("write");
    assert_eq!(problem(&mut session, "file.open", pdf), Some(FileProblem::Pdf));
    let not_text = dir.path().join("binary.fountain");
    std::fs::write(&not_text, [0xFF, 0xFE, 0xFD, 0x00, 0xC3]).expect("write");
    assert_eq!(problem(&mut session, "file.open", not_text), Some(FileProblem::NotAScript));
    assert_eq!(problem(&mut session, "file.save_as", dir.path().join("no folder/x.fountain")), Some(FileProblem::NotFound));
    // Other commands' errors are not file errors.
    assert_eq!(registry.run("app.nothing", &mut session, &Map::new()).err().and_then(|e| e.file_problem()), None);
}

/// Spell check steps through misspellings with suggestions; Ignore, Learn and Change.
#[test]
fn spell_check_steps_ignores_learns_and_changes() {
    let registry = Registry::new();
    let script = "INT. HUT - DAY\n\nNora taps the glsas. The seismograf is still.\n\nNORA\nZorbly.\n";
    let mut s = Session::new(storyteller_core::fountain::parse(script).unwrap());
    let next = |s: &mut Session| run(&registry, s, "spell.next", json!({}))["value"].clone();
    let first = next(&mut s);
    assert_eq!(first["word"], "glsas");
    assert_eq!(first["total"], 3);
    assert!(first["suggestions"].as_array().unwrap().iter().any(|w| w == "glass"), "{first}");
    // Change it, undoably.
    run(&registry, &mut s, "spell.change", json!({ "index": 1, "start": 14, "word": "glsas", "replacement": "glass" }));
    assert_eq!(s.script.elements[1].text, "Nora taps the glass. The seismograf is still.");
    assert_eq!(next(&mut s)["word"], "seismograf");
    // Ignore for the session; learn for good.
    s.dirty = false;
    run(&registry, &mut s, "spell.ignore", json!({ "word": "seismograf" }));
    assert_eq!(s.script.spelling_ignored, ["seismograf"], "ignored words stay with the script");
    assert!(s.dirty, "so the file has something new to save");
    let learned = run(&registry, &mut s, "spell.learn", json!({ "word": "Zorbly" }));
    assert_eq!(learned["value"]["learned"], "zorbly");
    assert_eq!(next(&mut s)["total"], 0, "nothing left");
    run(&registry, &mut s, "edit.undo", json!({}));
    assert!(s.script.elements[1].text.contains("glsas"), "Change is one Undo");
}

/// Saving says what a format will not keep, and nothing when it keeps everything.
#[test]
fn save_warnings_name_what_a_format_drops() {
    use storyteller_core::Element;
    use storyteller_engine::files::{FileFormat, save_warnings, saved_message};
    let mut s = storyteller_core::fountain::parse("# ACT ONE\n\nINT. A - DAY\n\nx\n").unwrap();
    s.elements.push(Element::new(K::Shot, "CLOSE ON THE NEEDLE"));
    s.elements.push(Element::new(K::Character, "NORA"));
    s.elements.push(Element::new(K::Dialogue, "One."));
    s.elements.push(Element::new(K::Dialogue, "Two."));
    s.elements.push(Element::new(K::Dialogue, "Three."));
    let fountain = save_warnings(&s, FileFormat::Fountain);
    assert_eq!(fountain.len(), 2, "{fountain:?}");
    assert!(fountain[0].starts_with("1 shot will read back as action"));
    assert!(fountain[1].starts_with("1 speech in more than one paragraph"));
    let osf = save_warnings(&s, FileFormat::Osf);
    assert_eq!(osf.len(), 1);
    assert!(osf[0].starts_with("1 section is kept for Method Storyteller"));
    assert!(save_warnings(&s, FileFormat::Fdx).is_empty());
    let message = saved_message("Saved", std::path::Path::new("a.fountain"), &s, FileFormat::Fountain);
    assert!(message.ends_with("Save as Open Screenplay Format to keep them."), "{message}");
    let plain = storyteller_core::fountain::parse("INT. A - DAY\n\nx\n").unwrap();
    assert_eq!(saved_message("Saved", std::path::Path::new("a.osf"), &plain, FileFormat::Osf), "Saved a.osf");
}

/// Saving as Fountain says that revisions, script notes and ignored words do not go with it.
#[test]
fn fountain_save_warns_about_revisions_and_notes() {
    use storyteller_core::model::{Revised, ScriptNote};
    use storyteller_engine::files::{FileFormat, save_warnings};
    let mut s = storyteller_core::fountain::parse("INT. A - DAY\n\nThe pump stops.\n").unwrap();
    s.elements[1].revised.push(Revised { from: 9, to: 14, set: 1 });
    s.elements[1].notes.push(ScriptNote { text: "Louder.".into(), color: Some("#FF0000".into()), ..ScriptNote::default() });
    let fountain = save_warnings(&s, FileFormat::Fountain);
    assert!(fountain.iter().any(|w| w.starts_with("1 revised passage will lose")), "{fountain:?}");
    assert!(fountain.iter().any(|w| w.starts_with("1 script note with a colour")), "{fountain:?}");
    s.elements[0].page_starts.push(storyteller_core::model::PageStart { at: 0, label: "1".into(), ..Default::default() });
    assert!(save_warnings(&s, FileFormat::Fountain).iter().any(|w| w.starts_with("the locked pages will be unlocked")));
    s.spelling_ignored = vec!["seismograf".into(), "zorbly".into()];
    let fountain = save_warnings(&s, FileFormat::Fountain);
    assert!(
        fountain.iter().any(|w| w == "2 words ignored by spell check will be checked again next time (Fountain has no ignore list)"),
        "{fountain:?}"
    );
    assert!(save_warnings(&s, FileFormat::Osf).is_empty() && save_warnings(&s, FileFormat::Fdx).is_empty());
}

/// Typing, Enter and Backspace keep revised text and notes on the words they mark.
#[test]
fn edits_keep_revisions_and_notes_on_their_text() {
    use storyteller_core::model::{Revised, ScriptNote};
    let mut session = storyteller_engine::session::Session::new(storyteller_core::fountain::parse("The pump stops dead.\n").unwrap());
    session.script.elements[0].revised.push(Revised { from: 9, to: 14, set: 1 });
    session.script.elements[0].notes.push(ScriptNote { text: "Why?".into(), range: Some((15, 19)), ..ScriptNote::default() });
    // Text typed before the revised word moves it along.
    storyteller_engine::commands::set_text(&mut session, 0, "The old pump stops dead.", true).unwrap();
    assert_eq!(session.script.elements[0].revised, [Revised { from: 13, to: 18, set: 1 }]);
    assert_eq!(session.script.elements[0].notes[0].range, Some((19, 23)));
    // Enter after "stops" takes the note to the new paragraph.
    storyteller_engine::commands::enter(&mut session, 0, "The old pump stops".len()).unwrap();
    assert_eq!(session.script.elements[0].notes.len(), 0);
    assert_eq!(session.script.elements[1].notes[0].range, Some((0, 4)));
    // Backspace joins it back, note and all.
    storyteller_engine::commands::merge_previous(&mut session, 1).unwrap();
    assert_eq!(session.script.elements[0].notes[0].range, Some((18, 22)));
}

/// A word ignored with capitals, as a file may hold it, is ignored in any case and not added twice.
#[test]
fn ignored_words_keep_their_case_and_match_any_case() {
    let registry = storyteller_engine::commands::Registry::new();
    let mut s = storyteller_engine::session::Session::new(storyteller_core::fountain::parse("McKenzie waits. MCKENZIE waits.\n").unwrap());
    s.script.spelling_ignored = vec!["McKenzie".into()];
    assert!(
        storyteller_engine::spell::misspelled_in(
            &s.script.elements[0].text,
            &Default::default(),
            &s.spelling,
            &storyteller_engine::spell::ignored_set(&s.script.spelling_ignored)
        )
        .is_empty()
    );
    run(&registry, &mut s, "spell.ignore", json!({ "word": "mckenzie" }));
    assert_eq!(s.script.spelling_ignored, ["McKenzie"]);
    // Spaces around a word are not part of it.
    run(&registry, &mut s, "spell.ignore", json!({ "word": " Zorbly " }));
    run(&registry, &mut s, "spell.ignore", json!({ "word": "zorbly" }));
    assert_eq!(s.script.spelling_ignored, ["McKenzie", "Zorbly"]);
}

#[test]
fn pasting_screenplay_text_makes_elements() {
    let registry = Registry::new();
    let pasted = "EXT. RIDGE - DAWN\n\nWind.\n\nNORA\n(quietly)\nIt held.\n";
    assert!(storyteller_engine::commands::is_screenplay_paste(pasted));
    assert!(!storyteller_engine::commands::is_screenplay_paste("one line\nand its second line"), "no blank line: an ordinary paste");

    // Into an empty line: the line becomes the pasted elements.
    let mut s = Session::new(storyteller_core::fountain::parse("INT. HUT - DAY\n\nShe waits.\n").unwrap());
    run(&registry, &mut s, "element.insert", json!({ "index": 1, "kind": "action" }));
    let empty = 2;
    let out = run(&registry, &mut s, "edit.paste_screenplay", json!({ "index": empty, "text": pasted }));
    assert_eq!(out["value"]["elements"], 5);
    assert_eq!(kinds(&s), [K::SceneHeading, K::Action, K::SceneHeading, K::Action, K::Character, K::Parenthetical, K::Dialogue]);
    assert_eq!(s.caret.element, 6, "the caret ends the paste");
    run(&registry, &mut s, "edit.undo", json!({}));
    assert_eq!(kinds(&s), [K::SceneHeading, K::Action, K::Action], "one Undo takes the whole paste back");

    // Into the middle of a line: it splits around the paste, and a selection is replaced.
    let mut s = Session::new(storyteller_core::fountain::parse("INT. HUT - DAY\n\nShe waits by the door.\n").unwrap());
    let line = "She waits by the door.";
    let (start, end) = (line.find("by").unwrap(), line.find(" the door").unwrap());
    run(&registry, &mut s, "edit.paste_screenplay", json!({ "index": 1, "start": start, "end": end, "text": "NORA\nNow.\n\nThe kettle sings." }));
    let texts: Vec<&str> = s.script.elements.iter().map(|e| e.text.as_str()).collect();
    assert_eq!(texts, ["INT. HUT - DAY", "She waits", "NORA", "Now.", "The kettle sings.", "the door."]);
    assert_eq!(s.script.elements[5].kind, K::Action, "the rest keeps the line's type");

    // A title-page-looking line is script text, not a title page.
    let mut s = Session::default();
    run(&registry, &mut s, "edit.paste_screenplay", json!({ "index": 0, "text": "Title: the sign says\n\nNORA\nHi." }));
    assert!(s.script.title_page.get("Title").is_none());
    assert_eq!(s.script.elements[0].text, "Title: the sign says");
}

#[test]
fn copy_scene_gives_fountain_that_pastes_back_the_same() {
    let registry = Registry::new();
    let mut s = Session::new(
        storyteller_core::fountain::parse(
            "INT. HUT - DAY\n\nOne.\n\nEXT. RIDGE - DAWN\n\nWind.\n\nNORA\n(quietly)\nIt held.\n\nINT. HUT - NIGHT\n\nTwo.\n",
        )
        .unwrap(),
    );
    let copied = run(&registry, &mut s, "edit.copy_scene", json!({ "index": 4 }));
    let text = copied["value"]["text"].as_str().unwrap().to_string();
    assert!(text.starts_with("EXT. RIDGE - DAWN") && text.contains("NORA\n(quietly)\nIt held.") && !text.contains("HUT"), "{text}");
    let scene: Vec<(K, String)> = s.script.elements[2..7].iter().map(|e| (e.kind, e.text.clone())).collect();
    let mut t = Session::default();
    run(&registry, &mut t, "edit.paste_screenplay", json!({ "index": 0, "text": text }));
    let back: Vec<(K, String)> = t.script.elements.iter().map(|e| (e.kind, e.text.clone())).collect();
    assert_eq!(back, scene, "element types and text come back the same");
}

#[test]
fn a_new_tv_script_is_laid_out_in_acts() {
    let registry = Registry::new();
    let mut s = Session::default();
    run(&registry, &mut s, "file.new_tv", json!({ "shape": "one_hour" }));
    let acts: Vec<&str> = s.script.elements.iter().filter(|e| e.kind == K::NewAct).map(|e| e.text.as_str()).collect();
    assert_eq!(acts, ["TEASER", "ACT ONE", "ACT TWO", "ACT THREE", "ACT FOUR", "ACT FIVE", "TAG"]);
    assert_eq!(&kinds(&s)[..3], [K::NewAct, K::SceneHeading, K::EndOfAct], "each act: its line, a scene heading, its end");
    assert_eq!(s.script.elements[2].text, "END OF TEASER");
    assert_eq!(s.caret.element, 1, "the caret in the first act's scene heading");
    assert!(!s.dirty && s.path.is_none());

    run(&registry, &mut s, "file.new_tv", json!({ "shape": "half_hour" }));
    let acts: Vec<&str> = s.script.elements.iter().filter(|e| e.kind == K::NewAct).map(|e| e.text.as_str()).collect();
    assert_eq!(acts, ["COLD OPEN", "ACT ONE", "ACT TWO", "ACT THREE", "TAG"]);

    let refused = registry.run("file.new_tv", &mut s, &serde_json::from_value(json!({ "shape": "feature" })).unwrap());
    assert!(refused.is_err(), "an unknown shape is refused");
}

#[test]
fn go_to_finds_an_act_by_position_or_name() {
    let registry = Registry::new();
    let mut s = Session::default();
    run(&registry, &mut s, "file.new_tv", json!({ "shape": "one_hour" }));
    let line = |s: &Session| s.script.elements[s.caret.element].text.clone();
    for (wanted, act) in [("2", "ACT ONE"), ("tag", "TAG"), ("Act Three", "ACT THREE"), ("five", "ACT FIVE"), (" 1 ", "TEASER")] {
        run(&registry, &mut s, "nav.go_to", json!({ "act": wanted }));
        assert_eq!(line(&s), act, "{wanted}");
    }
    for missing in ["8", "0", "ACT SIX", "intermission"] {
        let refused = registry.run("nav.go_to", &mut s, &serde_json::from_value(json!({ "act": missing })).unwrap());
        assert!(refused.is_err(), "{missing} is refused");
    }
}

#[test]
fn end_of_act_lines_follow_their_acts() {
    let registry = Registry::new();
    let mut s = Session::default();
    run(&registry, &mut s, "file.new_tv", json!({ "shape": "half_hour" }));
    let texts = |s: &Session| s.script.elements.iter().map(|e| e.text.clone()).collect::<Vec<_>>();
    let at = |s: &Session, text: &str| s.script.elements.iter().position(|e| e.text == text).unwrap();
    // Renaming an act renames its End of Act line, in the same Undo step.
    let two = at(&s, "ACT TWO");
    run(&registry, &mut s, "element.set_text", json!({ "index": two, "text": "act two b" }));
    assert!(texts(&s).contains(&"END OF ACT TWO B".to_string()), "{:?}", texts(&s));
    assert!(!texts(&s).contains(&"END OF ACT TWO".to_string()));
    run(&registry, &mut s, "edit.undo", json!({}));
    assert_eq!(s.script.elements[two + 2].text, "END OF ACT TWO", "one Undo puts both back");

    // The writer's own end line stays as written.
    let end_tag = at(&s, "END OF TAG");
    run(&registry, &mut s, "element.set_text", json!({ "index": end_tag, "text": "END OF SHOW" }));
    let tag = at(&s, "TAG");
    run(&registry, &mut s, "element.set_text", json!({ "index": tag, "text": "EPILOGUE" }));
    assert_eq!(s.script.elements[end_tag].text, "END OF SHOW");

    // A new act gets its End of Act line, after the caret, which stays on the act.
    let last = s.script.elements.len() - 1;
    run(&registry, &mut s, "element.insert", json!({ "index": last, "kind": "new_act", "text": "ACT FOUR" }));
    assert_eq!(s.script.elements[last + 1].text, "ACT FOUR");
    assert_eq!(s.script.elements[last + 2].text, "END OF ACT FOUR");
    assert_eq!(s.script.elements[last + 2].kind, K::EndOfAct);
    assert_eq!(s.caret.element, last + 1);
    // Typing on: it keeps in step, and no second line appears.
    run(&registry, &mut s, "element.set_text", json!({ "index": last + 1, "text": "ACT FOUR A", "typing": true }));
    assert_eq!(s.script.elements[last + 2].text, "END OF ACT FOUR A");
    assert_eq!(s.script.elements.iter().filter(|e| e.kind == K::EndOfAct).count(), 6, "five acts and the new one");
}

#[test]
fn an_opened_script_keeps_its_end_lines_as_written() {
    let registry = Registry::new();
    let script = storyteller_core::fountain::parse("> ACT ONE <\n\nINT. HUT - DAY\n\nRain.\n\n> END OF ACT TWO <\n").unwrap();
    let mut s = Session::new(script);
    run(&registry, &mut s, "caret.set", json!({ "index": 2, "offset": 0 }));
    assert_eq!(s.script.elements[3].text, "END OF ACT TWO", "moving the caret changes nothing");
    run(&registry, &mut s, "element.set_text", json!({ "index": 2, "text": "Rain falls." }));
    assert_eq!(s.script.elements[3].text, "END OF ACT ONE", "the first edit brings it in step");
}

#[test]
fn a_type_the_script_prints_in_capitals_is_typed_in_capitals() {
    let registry = Registry::new();
    let mut script = storyteller_core::fountain::parse("INT. HUT - DAY\n\nRain.\n").unwrap();
    script.format.elements.push(storyteller_core::format::ElementFormat { name: "Action".into(), all_caps: true, ..Default::default() });
    let mut s = Session::new(script);
    run(&registry, &mut s, "element.set_text", json!({ "index": 1, "text": "Nora runs.", "typing": true }));
    assert_eq!(s.script.elements[1].text, "NORA RUNS.");
}

#[test]
fn a_new_scene_in_a_multi_camera_script_gets_its_letter() {
    let registry = Registry::new();
    let mut s = Session::default();
    run(&registry, &mut s, "file.new_tv", json!({ "shape": "multi_cam" }));
    assert!(s.layout.dialogue.spacing == 2 && s.layout.scene_heading.new_page && s.layout.action.caps);
    let letters = |s: &Session| {
        s.script.elements.iter().filter(|e| e.kind == K::SceneHeading).map(|e| e.scene_number.clone().unwrap_or_default()).collect::<Vec<_>>()
    };
    assert_eq!(letters(&s), ["A", "B", "C", "D"]);
    // A second scene in the cold open: it becomes B, and the rest move on.
    run(&registry, &mut s, "element.insert", json!({ "index": 1, "kind": "scene_heading", "text": "INT. HUT - NIGHT" }));
    assert_eq!(letters(&s), ["A", "B", "C", "D", "E"]);
    assert_eq!(s.script.elements[2].scene_number.as_deref(), Some("B"));
    run(&registry, &mut s, "element.insert", json!({ "index": 2, "kind": "action", "text": "Nora waits." }));
    assert_eq!(s.script.elements[3].text, "NORA WAITS.", "action in capitals");
}

#[test]
fn format_style_turns_bold_on_and_off_in_one_undo_each() {
    let registry = Registry::new();
    let mut s = Session::new(storyteller_core::fountain::parse("INT. HUT - DAY\n\nShe runs home.\n").unwrap());
    let out = run(&registry, &mut s, "format.style", json!({ "index": 1, "start": 4, "end": 8, "style": "bold" }));
    assert_eq!(s.script.elements[1].text, "She **runs** home.");
    assert_eq!((out["value"]["start"].as_u64(), out["value"]["end"].as_u64()), (Some(6), Some(10)), "the same words, inside the markers");
    run(&registry, &mut s, "format.style", json!({ "index": 1, "start": 6, "end": 10, "style": "bold" }));
    assert_eq!(s.script.elements[1].text, "She runs home.");
    run(&registry, &mut s, "edit.undo", json!({}));
    assert_eq!(s.script.elements[1].text, "She **runs** home.");
    let refused =
        registry.run("format.style", &mut s, &serde_json::from_value(json!({ "index": 1, "start": 0, "end": 3, "style": "loud" })).unwrap());
    assert!(refused.is_err(), "an unknown style is refused");
}

#[test]
fn a_script_saves_as_a_package_and_opens_again_the_same() {
    use storyteller_engine::files::{self, FileFormat};
    let registry = Registry::new();
    let dir = tempfile::tempdir().unwrap();
    let text = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../examples/field-station.fountain")).unwrap();
    let mut s = Session::new(storyteller_core::fountain::parse(&text).unwrap());
    let path = dir.path().join("Field Station.mstory");
    run(&registry, &mut s, "file.save_as", json!({ "path": path.display().to_string() }));
    assert!(!s.dirty);
    assert_eq!(s.format, FileFormat::Mstory);
    let bytes = std::fs::read(&path).unwrap();
    assert!(storyteller_engine::container::is_package(&bytes), "one ZIP file holding script.osf");
    let (script, format) = files::open(&path).unwrap();
    assert_eq!(format, FileFormat::Mstory);
    let osf = |script: &storyteller_core::Script| storyteller_core::osf::write(script);
    assert_eq!(osf(&script), osf(&s.script), "the script inside is the Open Screenplay Format document");
    // Saving it again keeps the production data and gives the same bytes.
    files::save(&script, &path, FileFormat::Mstory, &s.layout).unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    assert_eq!(FileFormat::from_path(std::path::Path::new("a.MSTORY")), FileFormat::Mstory);
}

#[test]
fn spelling_follows_the_writers_english() {
    use storyteller_engine::spell::{Language, Personal, is_right};
    let none = std::collections::HashSet::new();
    let us = Personal { language: Language::Us, ..Default::default() };
    let uk = Personal { language: Language::Uk, ..Default::default() };
    for (word, in_us, in_uk) in [
        ("color", true, false),
        ("colour", false, true),
        ("organise", false, true),
        ("organize", true, false),
        ("theater", true, false),
        ("theatre", false, true),
        ("script", true, true),
    ] {
        assert_eq!(is_right(word, &none, &us, &none), in_us, "{word} in US English");
        assert_eq!(is_right(word, &none, &uk, &none), in_uk, "{word} in UK English");
    }
    assert!(
        storyteller_engine::spell::suggestions("colur", Language::Uk, 6).iter().any(|s| s == "colour"),
        "{:?}",
        storyteller_engine::spell::suggestions("colur", Language::Uk, 6)
    );
    assert_eq!(Language::for_region("GB"), Language::Uk);
    assert_eq!(Language::for_region("au"), Language::Uk);
    assert_eq!(Language::for_region("US"), Language::Us);
    assert_eq!(Language::for_region("DE"), Language::Us);
    assert_eq!(Language::from_id("en_GB"), Some(Language::Uk));
}

#[test]
fn scene_properties_set_in_one_undo_and_fountain_says_it_drops_them() {
    let registry = Registry::new();
    let mut s = Session::new(storyteller_core::fountain::parse("INT. HUT - DAY\n\nRain.\n\nEXT. RIDGE - DAWN\n\nWind.\n").unwrap());
    // Any element of the scene names it.
    run(
        &registry,
        &mut s,
        "scene.set_properties",
        json!({ "index": 3, "title": "The ridge\nat dawn", "summary": "Nora climbs.", "color": "#93c47d" }),
    );
    let meta = s.script.elements[2].scene.clone();
    assert_eq!((meta.title.as_str(), meta.summary.as_str(), meta.color.as_deref()), ("The ridge at dawn", "Nora climbs.", Some("#93C47D")));
    assert!(s.script.elements[0].scene.is_empty(), "only that scene");
    let warnings = storyteller_engine::files::save_warnings(&s.script, storyteller_engine::files::FileFormat::Fountain);
    assert!(warnings.iter().any(|w| w.contains("title, summary or colour")), "{warnings:?}");
    run(&registry, &mut s, "scene.set_properties", json!({ "index": 2, "color": "" }));
    assert_eq!(s.script.elements[2].scene.color, None);
    run(&registry, &mut s, "edit.undo", json!({}));
    assert_eq!(s.script.elements[2].scene.color.as_deref(), Some("#93C47D"));
    let refused = registry.run("scene.set_properties", &mut s, &serde_json::from_value(json!({ "index": 2, "color": "green" })).unwrap());
    assert!(refused.is_err(), "a colour is #RRGGBB");
}

#[test]
fn the_character_table_counts_speeches_scenes_and_words() {
    let script = storyteller_core::fountain::parse(
        "INT. HUT - DAY\n\nNORA\nIt held.\n\nOWEN (O.S.)\nDid it?\n\nNORA (CONT'D)\n(quietly)\nFor now, yes.\n\nEXT. RIDGE - DAWN #4#\n\nOWEN\nGo.\n",
    )
    .unwrap();
    let table = storyteller_engine::commands::character_table(&script);
    let row = |name: &str| table.iter().find(|r| r.name == name).unwrap();
    assert_eq!(table.iter().map(|r| r.name.as_str()).collect::<Vec<_>>(), ["NORA", "OWEN"], "two speeches each, then by name");
    assert_eq!((row("NORA").speeches, row("NORA").scenes, row("NORA").words), (2, 1, 5));
    assert_eq!((row("OWEN").speeches, row("OWEN").scenes, row("OWEN").words), (2, 2, 3));
    assert_eq!(row("NORA").first_scene.as_deref(), Some("1"), "unnumbered: by position");
    assert_eq!(row("OWEN").first, 3, "the first cue, extension and all");
    let registry = Registry::new();
    let mut s = Session::new(script);
    assert_eq!(run(&registry, &mut s, "character.table", json!({}))["value"].as_array().map(Vec::len), Some(2));
}

#[test]
fn number_scenes_keeps_numbers_letters_new_scenes_and_edits_one() {
    let registry = Registry::new();
    let mut s = Session::new(storyteller_core::fountain::parse("INT. A - DAY #5#\n\nINT. NEW - DAY\n\nINT. B - DAY #6#\n").unwrap());
    let numbers = |s: &Session| {
        s.script.elements.iter().filter(|e| e.kind == K::SceneHeading).map(|e| e.scene_number.clone().unwrap_or_default()).collect::<Vec<_>>()
    };
    let out = run(&registry, &mut s, "scene.number", json!({ "keep_existing": true }));
    assert_eq!(numbers(&s), ["5", "5A", "6"]);
    assert_eq!(out["message"], "Numbered 1 scene");
    run(&registry, &mut s, "edit.undo", json!({}));
    assert_eq!(numbers(&s), ["5", "", "6"], "one Undo");
    run(&registry, &mut s, "scene.number", json!({ "start": 20 }));
    assert_eq!(numbers(&s), ["20", "21", "22"]);
    run(&registry, &mut s, "scene.set_number", json!({ "index": 1, "number": "21a" }));
    assert_eq!(numbers(&s)[1], "21A");
    let taken = registry.run("scene.set_number", &mut s, &serde_json::from_value(json!({ "index": 1, "number": "22" })).unwrap());
    assert!(taken.is_err(), "another scene has 22");
}

#[test]
fn a_new_scene_with_no_number_between_its_neighbours_is_left_for_the_writer() {
    let registry = Registry::new();
    let mut s = Session::new(storyteller_core::fountain::parse("INT. A - DAY #2#\n\nINT. NEW - DAY\n\nINT. B - DAY #2A#\n").unwrap());
    let out = run(&registry, &mut s, "scene.number", json!({ "keep_existing": true }));
    assert_eq!(s.script.elements[1].scene_number, None);
    assert_eq!(out["message"], "There's no number between 2 and 2A for scene 2; set it by hand (Production > Edit Scene Number…).");
    assert!(!s.can_undo(), "nothing changed, so nothing to undo");
}

#[test]
fn omit_and_unomit_a_scene_one_undo_each() {
    let registry = Registry::new();
    let mut s = Session::new(storyteller_core::fountain::parse("INT. HUT - DAY #4#\n\nRain.\n\nEXT. RIDGE - DAWN #5#\n\nWind.\n").unwrap());
    s.caret = storyteller_engine::Caret { element: 1, offset: 0 };
    assert!(registry.get("scene.unomit").is_some_and(|c| !(c.enabled)(&s)), "Unomit is for omitted scenes");
    run(&registry, &mut s, "scene.omit", json!({}));
    assert_eq!(s.script.elements[0].text, "OMITTED");
    assert_eq!(s.script.elements[1].text, "EXT. RIDGE - DAWN");
    assert!(registry.get("scene.omit").is_some_and(|c| !(c.enabled)(&s)), "an omitted scene can't be omitted again");
    run(&registry, &mut s, "scene.unomit", json!({}));
    assert_eq!(s.script.elements[1].text, "Rain.");
    assert_eq!(s.script.elements[0].scene_number.as_deref(), Some("4"));
    run(&registry, &mut s, "edit.undo", json!({}));
    assert_eq!(s.script.elements[0].text, "OMITTED", "one Undo back to omitted");
}

#[test]
fn revision_sets_are_added_changed_and_deleted_one_undo_each() {
    let registry = Registry::new();
    let mut s = Session::new(storyteller_core::fountain::parse("INT. HUT - DAY\n\nRain.\n").unwrap());
    run(&registry, &mut s, "revisions.add", json!({ "date": "10/21/26" }));
    let blue = run(&registry, &mut s, "revisions.add", json!({ "date": "10/22/26" }))["value"]["id"].as_u64().unwrap();
    let list = run(&registry, &mut s, "revisions.list", json!({}));
    assert_eq!(list["value"][1]["name"], "Blue Revision 10/22/26");
    assert_eq!(list["value"][1]["active"], true);
    run(&registry, &mut s, "revisions.set_active", json!({ "id": 1 }));
    assert_eq!(s.script.revisions.active, Some(1));
    run(&registry, &mut s, "revisions.update", json!({ "id": blue, "mark": "#", "page_color": "#c9daf8" }));
    assert_eq!(s.script.revisions.sets[1].mark, "#");
    assert!(registry.run("revisions.update", &mut s, &serde_json::from_value(json!({ "id": blue, "page_color": "blue" })).unwrap()).is_err());
    run(&registry, &mut s, "revisions.delete", json!({ "id": blue }));
    assert_eq!(s.script.revisions.sets.len(), 1);
    run(&registry, &mut s, "edit.undo", json!({}));
    assert_eq!(s.script.revisions.sets.len(), 2, "one Undo brings it back");
    assert!(registry.run("revisions.delete", &mut s, &serde_json::from_value(json!({ "id": 99 })).unwrap()).is_err());
}

#[test]
fn revision_mode_marks_typing_and_mark_and_clear_act_on_a_range() {
    let registry = Registry::new();
    let mut s = Session::new(storyteller_core::fountain::parse("INT. HUT - DAY\n\nRain **falls**.\n").unwrap());
    let out = run(&registry, &mut s, "revisions.mode", json!({}));
    assert!(s.revision_mode);
    assert_eq!(s.script.revisions.sets.len(), 1, "a first set, so there is somewhere to mark");
    assert!(out["message"].as_str().unwrap_or_default().starts_with("Revision mode: changes are marked in White Revision"));
    let white = s.script.revisions.active.unwrap();
    run(&registry, &mut s, "element.set_text", json!({ "index": 1, "text": "Rain **falls** hard.", "typing": true }));
    assert_eq!(s.script.elements[1].revised, [storyteller_core::Revised { from: 10, to: 15, set: white }]);
    run(&registry, &mut s, "revisions.mode", json!({ "on": false }));
    run(&registry, &mut s, "revisions.clear", json!({ "index": 1 }));
    assert!(s.script.elements[1].revised.is_empty(), "the whole paragraph");
    // Byte offsets in the markup; the printed characters "falls" are marked.
    let start = "Rain **".len();
    run(&registry, &mut s, "revisions.mark", json!({ "index": 1, "start": start, "end": start + 5 }));
    assert_eq!(s.script.elements[1].revised, [storyteller_core::Revised { from: 5, to: 10, set: white }]);
    run(&registry, &mut s, "edit.undo", json!({}));
    assert!(s.script.elements[1].revised.is_empty());
}
