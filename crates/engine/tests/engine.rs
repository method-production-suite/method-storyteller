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
    // Scene number margins are a script setting, one Undo.
    assert_eq!(run(&registry, &mut s, "scene.number_margins", json!({ "left": false }))["value"], json!({ "left": false, "right": true }));
    assert_eq!(s.script.scene_number_margins(), (false, true));
    assert!(s.dirty);
    s.undo();
    assert_eq!(s.script.scene_number_margins(), (true, true));
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

fn range(from: (usize, usize), to: (usize, usize)) -> Value {
    json!({ "from_index": from.0, "from_offset": from.1, "to_index": to.0, "to_offset": to.1 })
}

fn texts(session: &Session) -> Vec<&str> {
    session.script.elements.iter().map(|e| e.text.as_str()).collect()
}

const RANGE_SCRIPT: &str = "INT. HUT - DAY\n\nShe waits.\n\nNORA\nIt held.\n\nRain falls.\n";

#[test]
fn a_range_copies_its_elements_with_their_types() {
    let registry = Registry::new();
    let mut s = Session::new(storyteller_core::fountain::parse(RANGE_SCRIPT).unwrap());
    let before = s.edits();
    // The end of the heading to the start of the dialogue, selected backwards.
    let copied = run(&registry, &mut s, "edit.copy_range", range((3, 2), (0, "INT. ".len())));
    assert_eq!(s.edits(), before, "copying edits nothing");
    let text = copied["value"]["text"].as_str().unwrap().to_string();
    // Heading text cut short still says it is a heading, in Fountain, for any other app.
    assert!(text.starts_with(".HUT - DAY"), "{text}");
    assert!(text.contains("She waits.") && text.contains("NORA\nIt"), "{text}");
    assert!(!text.contains("held"), "{text}");

    // Pasted here, the same types, on a line of its own.
    let at = s.script.elements.len() - 1;
    run(&registry, &mut s, "edit.paste_screenplay", json!({ "index": at, "start": 0, "end": 0, "text": text }));
    assert_eq!(kinds(&s)[at..], [K::SceneHeading, K::Action, K::Character, K::Dialogue, K::Action]);
    assert_eq!(s.script.elements[at + 3].text, "It");
    assert_eq!(s.script.elements[at + 4].text, "Rain falls.", "the line it went in front of");

    // Dialogue alone: Fountain cannot say it, but the copy pasted back here still can.
    let mut s = Session::new(storyteller_core::fountain::parse("NORA\nOne.\n\nBEN\nTwo.\n").unwrap());
    let copied = run(&registry, &mut s, "edit.copy_range", range((1, 0), (2, 3)));
    let text = copied["value"]["text"].as_str().unwrap().to_string();
    assert!(s.pastes_as_elements(&text));
    run(&registry, &mut s, "element.insert", json!({ "index": 3, "kind": "action" }));
    run(&registry, &mut s, "edit.paste_screenplay", json!({ "index": 4, "start": 0, "end": 0, "text": text }));
    assert_eq!(kinds(&s)[4..], [K::Dialogue, K::Character]);
    // A nothing at the range's end is left out.
    let mut s = Session::new(storyteller_core::fountain::parse(RANGE_SCRIPT).unwrap());
    let copied = run(&registry, &mut s, "edit.copy_range", range((1, 0), (2, 0)));
    assert!(!copied["value"]["text"].as_str().unwrap().contains("NORA"), "{copied}");
}

#[test]
fn replacing_a_range_joins_its_ends_in_one_undo() {
    let registry = Registry::new();
    let mut s = Session::new(storyteller_core::fountain::parse(RANGE_SCRIPT).unwrap());
    let plain = range((1, "She ".len()), (3, "It ".len()));
    run(&registry, &mut s, "edit.replace_range", plain);
    assert_eq!(texts(&s), ["INT. HUT - DAY", "She held.", "Rain falls."]);
    assert_eq!(s.script.elements[1].kind, K::Action, "the first paragraph's type");
    assert_eq!((s.caret.element, s.caret.offset), (1, "She ".len()));
    run(&registry, &mut s, "edit.undo", json!({}));
    assert_eq!(texts(&s), ["INT. HUT - DAY", "She waits.", "NORA", "It held.", "Rain falls."], "one Undo");

    // Text takes its place, in the first paragraph's capitals.
    let mut params = range((0, "INT. ".len()), (1, "She ".len()));
    params["text"] = json!("ridge ");
    run(&registry, &mut s, "edit.replace_range", params);
    assert_eq!(texts(&s)[0], "INT. RIDGE WAITS.");
    assert_eq!(s.caret.offset, "INT. RIDGE ".len());
    run(&registry, &mut s, "edit.undo", json!({}));

    // Screenplay text becomes elements, inside the same one Undo.
    let mut params = range((1, "She ".len()), (3, "It ".len()));
    params["text"] = json!("EXT. RIDGE - DAWN\n\nWind.");
    run(&registry, &mut s, "edit.replace_range", params);
    assert_eq!(texts(&s), ["INT. HUT - DAY", "She", "EXT. RIDGE - DAWN", "Wind.", "held.", "Rain falls."]);
    run(&registry, &mut s, "edit.undo", json!({}));
    assert_eq!(s.script.elements.len(), 5, "one Undo for the paste too");

    // The whole script leaves one empty paragraph.
    let last = s.script.elements.len() - 1;
    let end = s.script.elements[last].text.len();
    run(&registry, &mut s, "edit.replace_range", range((0, 0), (last, end)));
    assert_eq!(texts(&s), [""]);
    assert_eq!(s.script.elements[0].kind, K::SceneHeading);

    // A range past the script is refused.
    let mut s = Session::new(storyteller_core::fountain::parse(RANGE_SCRIPT).unwrap());
    assert!(registry.run("edit.replace_range", &mut s, range((1, 0), (99, 0)).as_object().unwrap()).is_err());
    assert!(registry.run("edit.copy_range", &mut s, json!({ "from_index": 1 }).as_object().unwrap()).is_err());
    assert_eq!(s.script.elements.len(), 5);
}

#[test]
fn a_range_within_one_paragraph_keeps_what_is_anchored_after_it() {
    use storyteller_core::model::{PageStart, Revised, ScriptNote};
    let registry = Registry::new();
    let mut s = Session::new(storyteller_core::fountain::parse("Rain falls on the hut tonight.\n").unwrap());
    // "tonight" is revised, a page starts at "hut" and a note covers "tonight".
    let at = |word: &str| u32::try_from("Rain falls on the hut tonight.".find(word).unwrap()).unwrap();
    let element = &mut s.script.elements[0];
    element.revised.push(Revised { from: at("tonight"), to: at("."), set: 1 });
    element.page_starts.push(PageStart { at: at("hut"), label: "2".into(), ..PageStart::default() });
    element.notes.push(ScriptNote { text: "Which night?".into(), range: Some((at("tonight"), at("."))), ..ScriptNote::default() });

    // Cut "on the " out.
    run(&registry, &mut s, "edit.replace_range", range((0, "Rain falls ".len()), (0, "Rain falls on the ".len())));
    let element = &s.script.elements[0];
    assert_eq!(element.text, "Rain falls hut tonight.");
    let at = |word: &str| u32::try_from("Rain falls hut tonight.".find(word).unwrap()).unwrap();
    assert_eq!(element.revised, [Revised { from: at("tonight"), to: at("."), set: 1 }]);
    assert_eq!(element.page_starts.iter().map(|p| p.at).collect::<Vec<_>>(), [at("hut")]);
    assert_eq!(element.notes[0].range, Some((at("tonight"), at("."))));

    // Replaced with text, they move by its length.
    run(&registry, &mut s, "edit.undo", json!({}));
    let mut params = range((0, "Rain falls ".len()), (0, "Rain falls on the ".len()));
    params["text"] = json!("near the old ");
    run(&registry, &mut s, "edit.replace_range", params);
    let element = &s.script.elements[0];
    assert_eq!(element.text, "Rain falls near the old hut tonight.");
    let at = |word: &str| u32::try_from("Rain falls near the old hut tonight.".find(word).unwrap()).unwrap();
    assert_eq!(element.page_starts.iter().map(|p| p.at).collect::<Vec<_>>(), [at("hut")]);
    assert_eq!(element.notes[0].range, Some((at("tonight"), at("."))));
    assert!(element.revised.contains(&Revised { from: at("tonight"), to: at("."), set: 1 }), "{:?}", element.revised);
}

#[test]
fn in_revision_mode_a_replaced_range_is_marked() {
    let registry = Registry::new();
    let mut s = Session::new(storyteller_core::fountain::parse(RANGE_SCRIPT).unwrap());
    let set = run(&registry, &mut s, "revisions.add", json!({ "date": "10/21/26" }))["value"]["id"].as_u64().unwrap();
    run(&registry, &mut s, "revisions.set_active", json!({ "id": set }));
    run(&registry, &mut s, "revisions.mode", json!({ "on": true }));
    run(&registry, &mut s, "edit.replace_range", range((1, "She ".len()), (3, "It ".len())));
    let joined = &s.script.elements[1];
    assert_eq!(joined.text, "She held.");
    assert_eq!(joined.deletions.iter().map(|d| d.at).collect::<Vec<_>>(), ["She ".len() as u32], "a mark where the text went");

    // What came with the last paragraph keeps its own place.
    let mut s = Session::new(storyteller_core::fountain::parse(RANGE_SCRIPT).unwrap());
    let set = run(&registry, &mut s, "revisions.add", json!({ "date": "10/21/26" }))["value"]["id"].as_u64().unwrap();
    run(&registry, &mut s, "revisions.set_active", json!({ "id": set }));
    run(&registry, &mut s, "revisions.mark", json!({ "index": 3, "start": 3, "end": 7 }));
    assert_eq!(s.script.elements[3].revised.len(), 1, "{:?}", s.script.elements[3].revised);
    run(&registry, &mut s, "edit.replace_range", range((1, "She ".len()), (3, "It ".len())));
    let mark = &s.script.elements[1].revised[0];
    assert_eq!((mark.from, mark.to), (4, 8), "\"held\" is still the revised text");
}

#[test]
fn moving_a_scene_takes_its_paragraphs_in_one_undo() {
    let registry = Registry::new();
    let script = "INT. A - DAY #1#\n\nOne.\n\nEXT. B - DAY #2#\n\nNORA\nTwo.\n\nINT. C - DAY #3#\n\nThree.\n";
    let mut s = Session::new(storyteller_core::fountain::parse(script).unwrap());
    let original: Vec<String> = texts(&s).iter().map(ToString::to_string).collect();
    // C to the front.
    let out = run(&registry, &mut s, "scene.move", json!({ "index": 5, "to": 0 }));
    assert_eq!(out["value"]["moved"], true);
    assert_eq!(texts(&s), ["INT. C - DAY", "Three.", "INT. A - DAY", "One.", "EXT. B - DAY", "NORA", "Two."]);
    assert_eq!(s.script.elements[0].scene_number.as_deref(), Some("3"), "its number goes with it");
    assert_eq!(s.caret.element, 0, "the caret goes with it too");
    run(&registry, &mut s, "edit.undo", json!({}));
    assert_eq!(texts(&s), original, "one Undo");
    // A to the end of the script, past C.
    let end = s.script.elements.len();
    run(&registry, &mut s, "scene.move", json!({ "index": 0, "to": end }));
    assert_eq!(texts(&s), ["EXT. B - DAY", "NORA", "Two.", "INT. C - DAY", "Three.", "INT. A - DAY", "One."]);
    assert_eq!(s.caret.element, 5);
    run(&registry, &mut s, "edit.undo", json!({}));
    // Onto itself, or just after itself, moves nothing and leaves no Undo.
    let edits = s.edits();
    for to in [2, 3, 4, 5] {
        assert_eq!(run(&registry, &mut s, "scene.move", json!({ "index": 2, "to": to }))["value"]["moved"], false, "to {to}");
        assert_eq!(s.edits(), edits);
    }
    // Only a heading moves, and only within the script.
    assert!(registry.run("scene.move", &mut s, json!({ "index": 1, "to": 0 }).as_object().unwrap()).is_err());
    assert!(registry.run("scene.move", &mut s, json!({ "index": 0, "to": 99 }).as_object().unwrap()).is_err());
}

#[test]
fn moving_a_scene_leaves_act_lines_and_locked_pages_where_they_are() {
    let registry = Registry::new();
    let mut s = Session::new(storyteller_core::fountain::parse("INT. A - DAY\n\nOne.\n\nEXT. B - DAY\n\nTwo.\n\nINT. C - DAY\n\nThree.\n").unwrap());
    run(&registry, &mut s, "element.insert", json!({ "index": 3, "kind": "end_of_act" }));
    run(&registry, &mut s, "element.set_text", json!({ "index": 4, "text": "END OF ACT ONE" }));
    run(&registry, &mut s, "element.insert", json!({ "index": 4, "kind": "new_act" }));
    run(&registry, &mut s, "element.set_text", json!({ "index": 5, "text": "ACT TWO" }));
    let at = |s: &Session, wanted: &str| s.script.elements.iter().position(|e| e.text == wanted).unwrap();
    // The last scene of an act leaves its closing line behind.
    assert_eq!(texts(&s)[2..4].len(), 2);
    let b = at(&s, "EXT. B - DAY");
    let end = s.script.elements.len();
    run(&registry, &mut s, "scene.move", json!({ "index": b, "to": end }));
    assert_eq!(s.script.elements.last().unwrap().text, "Two.");
    assert_eq!(at(&s, "INT. C - DAY"), at(&s, "ACT TWO") + 1, "the act keeps its place, and C is now first in it");
    run(&registry, &mut s, "edit.undo", json!({}));

    // In revision mode a moved scene reads as revised; a locked page start stays behind.
    let mut s = Session::new(storyteller_core::fountain::parse("INT. A - DAY\n\nOne.\n\nEXT. B - DAY\n\nTwo.\n\nINT. C - DAY\n\nThree.\n").unwrap());
    let set = run(&registry, &mut s, "revisions.add", json!({ "date": "10/21/26" }))["value"]["id"].as_u64().unwrap();
    run(&registry, &mut s, "revisions.set_active", json!({ "id": set }));
    run(&registry, &mut s, "revisions.mode", json!({ "on": true }));
    s.script.elements[2].page_starts.push(storyteller_core::model::PageStart { at: 0, label: "2".into(), ..Default::default() });
    run(&registry, &mut s, "scene.move", json!({ "index": 2, "to": 0 }));
    assert!(!s.script.elements[0].revised.is_empty() && !s.script.elements[1].revised.is_empty());
    assert!(s.script.elements[0].page_starts.is_empty());
    let c = at(&s, "INT. C - DAY");
    assert_eq!(s.script.elements[c].page_starts[0].label, "2");
    assert!(s.script.elements[at(&s, "One.")].revised.is_empty(), "what stayed is not marked");
}

#[test]
fn locked_pages_a_moved_scene_leaves_behind_print_as_one() {
    use storyteller_core::model::PageStart;
    let registry = Registry::new();
    let mut s = Session::new(storyteller_core::fountain::parse("INT. A - DAY\n\nOne.\n\nEXT. B - DAY\n\nTwo.\n\nINT. C - DAY\n\nThree.\n").unwrap());
    let lock = |label: &str, at: u32| PageStart { at, label: label.into(), ..PageStart::default() };
    s.script.elements[0].page_starts.push(lock("1", 0));
    s.script.elements[2].page_starts.push(lock("2", 0));
    s.script.elements[3].page_starts.push(lock("3", 2));
    s.script.elements[4].page_starts.push(lock("4", 0));
    // B and both its pages move to the end: pages 2 and 3 now start where 4 does.
    run(&registry, &mut s, "scene.move", json!({ "index": 2, "to": 6 }));
    let c = s.script.elements.iter().position(|e| e.text == "INT. C - DAY").unwrap();
    assert_eq!(s.script.elements[c].page_starts.iter().map(|p| (p.at, p.label.as_str())).collect::<Vec<_>>(), [(0, "2"), (0, "3"), (0, "4")]);
    let pages = storyteller_core::paginate::paginate(&s.script, &s.layout).pages;
    let labels: Vec<String> = pages.iter().map(storyteller_core::paginate::Page::label).collect();
    // Pages 2 and 3 are empty and 4's text fits on page 1, so they print as one page.
    assert_eq!(labels, ["1-4"], "docs/production-spec.md: an empty locked page prints with the one before");
}

#[test]
fn bookmarks_go_to_undo_and_keep_to_one_place() {
    let registry = Registry::new();
    let mut s = Session::new(storyteller_core::fountain::parse(RANGE_SCRIPT).unwrap());
    assert!(registry.run("bookmark.remove", &mut s, &Map::new()).is_err(), "nothing to remove where the caret is");
    run(&registry, &mut s, "bookmark.add", json!({ "index": 3, "name": "  The turn " }));
    run(&registry, &mut s, "bookmark.add", json!({ "index": 1, "name": "Top" }));
    let list = run(&registry, &mut s, "bookmark.list", json!({}));
    assert_eq!(list["value"][0]["name"], "Top");
    assert_eq!(list["value"][1]["name"], "The turn", "trimmed, and in script order");
    assert_eq!(list["value"][1]["element"], 3);
    assert_eq!(list["value"][1]["page"], 1);

    // By name in any case, or enough of its start.
    for name in ["the turn", "THE", "t h"] {
        let found = registry.run("nav.go_to", &mut s, json!({ "bookmark": name }).as_object().unwrap());
        if name == "t h" {
            assert!(found.is_err());
        } else {
            assert_eq!(found.unwrap().value["element"], 3, "{name}");
            assert_eq!(s.caret.element, 3);
        }
    }
    run(&registry, &mut s, "bookmark.add", json!({ "index": 4, "name": "Tail" }));
    assert!(registry.run("nav.go_to", &mut s, json!({ "bookmark": "t" }).as_object().unwrap()).is_err(), "more than one starts so");

    // A name used again moves; one Undo puts it back. Removing is one Undo too.
    run(&registry, &mut s, "bookmark.add", json!({ "index": 0, "name": "TOP" }));
    assert!(s.script.elements[1].bookmark.is_none());
    assert_eq!(s.script.elements[0].bookmark.as_deref(), Some("TOP"));
    run(&registry, &mut s, "edit.undo", json!({}));
    assert_eq!(s.script.elements[1].bookmark.as_deref(), Some("Top"));
    run(&registry, &mut s, "bookmark.remove", json!({ "index": 1 }));
    assert!(s.script.elements[1].bookmark.is_none());
    run(&registry, &mut s, "edit.undo", json!({}));
    assert!(s.script.elements[1].bookmark.is_some());
    assert!(registry.run("bookmark.add", &mut s, json!({ "name": "  " }).as_object().unwrap()).is_err(), "a name is needed");

    // Open Screenplay Format and .fdx keep them, escaped; Fountain says it cannot.
    run(&registry, &mut s, "bookmark.add", json!({ "index": 2, "name": "A & \"B\" <c>" }));
    let osf = storyteller_core::osf::parse(&storyteller_core::osf::write(&s.script)).unwrap();
    let fdx = storyteller_core::fdx::parse(&storyteller_core::fdx::write(&s.script)).unwrap();
    for script in [&osf, &fdx] {
        let names: Vec<Option<&str>> = script.elements.iter().map(|e| e.bookmark.as_deref()).collect();
        assert_eq!(names[..5], [None, Some("Top"), Some("A & \"B\" <c>"), Some("The turn"), Some("Tail")]);
    }
    let warnings = storyteller_engine::files::save_warnings(&s.script, storyteller_engine::files::FileFormat::Fountain);
    assert!(warnings.iter().any(|w| w.contains("3 bookmarks") || w.contains("4 bookmarks")), "{warnings:?}");
    assert!(storyteller_engine::files::save_warnings(&s.script, storyteller_engine::files::FileFormat::Osf).iter().all(|w| !w.contains("bookmark")));
    // A second round trip through either is the same text.
    assert_eq!(
        storyteller_core::osf::write(&osf),
        storyteller_core::osf::write(&storyteller_core::osf::parse(&storyteller_core::osf::write(&osf)).unwrap())
    );
    assert_eq!(
        storyteller_core::fdx::write(&fdx),
        storyteller_core::fdx::write(&storyteller_core::fdx::parse(&storyteller_core::fdx::write(&fdx)).unwrap())
    );
    // A copied range does not bring a bookmark along.
    let copied = run(&registry, &mut s, "edit.copy_range", range((1, 0), (2, 3)));
    let text = copied["value"]["text"].as_str().unwrap();
    assert_eq!(s.copied_elements(text).unwrap().iter().filter(|e| e.bookmark.is_some()).count(), 0);
}

#[test]
fn a_highland_file_imports_as_a_new_unsaved_script() {
    use std::io::Write;
    let registry = Registry::new();
    let dir = tempfile::tempdir().unwrap();
    let zip_of = |member: &str, text: &str| {
        // A stored (uncompressed) ZIP with one member, as the format allows.
        let (name, data) = (member.as_bytes(), text.as_bytes());
        let crc = {
            let mut crc = !0u32;
            for byte in data {
                crc ^= u32::from(*byte);
                for _ in 0..8 {
                    crc = if crc & 1 == 1 { (crc >> 1) ^ 0xEDB8_8320 } else { crc >> 1 };
                }
            }
            !crc
        };
        let (n, d) = (name.len() as u16, data.len() as u32);
        let mut out = Vec::new();
        out.write_all(b"PK\x03\x04\x14\0\0\0\0\0\0\0\0\0").unwrap();
        out.write_all(&crc.to_le_bytes()).unwrap();
        out.write_all(&d.to_le_bytes()).unwrap();
        out.write_all(&d.to_le_bytes()).unwrap();
        out.write_all(&n.to_le_bytes()).unwrap();
        out.write_all(&[0, 0]).unwrap();
        out.write_all(name).unwrap();
        out.write_all(data).unwrap();
        let central = out.len() as u32;
        out.write_all(b"PK\x01\x02\x14\0\x14\0\0\0\0\0\0\0\0\0").unwrap();
        out.write_all(&crc.to_le_bytes()).unwrap();
        out.write_all(&d.to_le_bytes()).unwrap();
        out.write_all(&d.to_le_bytes()).unwrap();
        out.write_all(&n.to_le_bytes()).unwrap();
        out.write_all(&[0; 12]).unwrap();
        out.write_all(&0u32.to_le_bytes()).unwrap();
        out.write_all(name).unwrap();
        let size = out.len() as u32 - central;
        out.write_all(b"PK\x05\x06\0\0\0\0\x01\0\x01\0").unwrap();
        out.write_all(&size.to_le_bytes()).unwrap();
        out.write_all(&central.to_le_bytes()).unwrap();
        out.write_all(&[0, 0]).unwrap();
        out
    };
    for member in ["Pilot.textbundle/text.fountain", "Pilot.textbundle/text.md", "text.markdown"] {
        let path = dir.path().join("Pilot.highland");
        std::fs::write(&path, zip_of(member, "INT. HUT - DAY\n\nNORA\nIt held.\n")).unwrap();
        let mut s = Session::default();
        let opened = registry.run("file.open", &mut s, json!({ "path": path }).as_object().unwrap()).unwrap();
        assert!(opened.message.unwrap().starts_with("Imported 3 elements"), "{member}");
        assert_eq!(kinds(&s), [K::SceneHeading, K::Character, K::Dialogue]);
        assert!(s.path.is_none(), "nothing writes back to the .highland file");
        assert!(s.dirty, "so closing it asks");
    }
    // An archive with no text in it says so, and leaves the script open as it was.
    let path = dir.path().join("Empty.highland");
    std::fs::write(&path, zip_of("Pilot.textbundle/info.json", "{}")).unwrap();
    let mut s = Session::new(storyteller_core::fountain::parse(RANGE_SCRIPT).unwrap());
    assert!(registry.run("file.open", &mut s, json!({ "path": path }).as_object().unwrap()).is_err());
    assert_eq!(s.script.elements.len(), 5);
}

#[test]
fn autocomplete_lists_are_edited_learned_and_kept_in_the_file() {
    let registry = Registry::new();
    let mut s = Session::new(storyteller_core::fountain::parse(RANGE_SCRIPT).unwrap());
    run(&registry, &mut s, "lists.add", json!({ "list": "characters", "entry": " martha " }));
    assert_eq!(s.script.lists.characters, ["MARTHA"], "trimmed and in capitals");
    assert_eq!(run(&registry, &mut s, "lists.add", json!({ "list": "characters", "entry": "Martha" }))["value"]["added"], false, "once");
    assert!(registry.run("lists.add", &mut s, json!({ "list": "props", "entry": "x" }).as_object().unwrap()).is_err());
    run(&registry, &mut s, "edit.undo", json!({}));
    assert!(s.script.lists.characters.is_empty(), "one Undo");
    run(&registry, &mut s, "lists.add", json!({ "list": "transitions", "entry": "Smash to:" }));
    run(&registry, &mut s, "lists.add", json!({ "list": "times", "entry": "golden hour" }));
    run(&registry, &mut s, "lists.add", json!({ "list": "characters", "entry": "Gone" }));
    assert_eq!(run(&registry, &mut s, "lists.remove", json!({ "list": "characters", "entry": "gone" }))["value"]["removed"], true);
    assert!(s.script.lists.characters.is_empty());

    // Open Screenplay Format and .fdx keep them, and both round trips are stable.
    let osf = storyteller_core::osf::parse(&storyteller_core::osf::write(&s.script)).unwrap();
    assert_eq!(osf.lists, s.script.lists);
    let fdx = storyteller_core::fdx::parse(&storyteller_core::fdx::write(&s.script)).unwrap();
    assert_eq!(fdx.lists.transitions, s.script.lists.transitions);
    assert_eq!(fdx.lists.times, s.script.lists.times);
    assert_eq!(fdx.lists.intro_separator.as_deref(), Some(". "), "the sections .fdx expects");
    assert_eq!(fdx.lists.time_separator.as_deref(), Some(" - "));
    assert_eq!(
        storyteller_core::fdx::write(&fdx),
        storyteller_core::fdx::write(&storyteller_core::fdx::parse(&storyteller_core::fdx::write(&fdx)).unwrap())
    );
    assert_eq!(
        storyteller_core::osf::write(&osf),
        storyteller_core::osf::write(&storyteller_core::osf::parse(&storyteller_core::osf::write(&osf)).unwrap())
    );

    // Saving an edited script learns what it uses; an unedited one is written back as it came.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("a.osf");
    run(&registry, &mut s, "file.save_as", json!({ "path": path }));
    assert_eq!(s.script.lists.characters, ["NORA"], "learned");
    assert_eq!(s.script.lists.locations, ["HUT"]);
    let saved = std::fs::read_to_string(&path).unwrap();
    assert!(saved.contains("method_storyteller_lists") && saved.contains("NORA"));
    run(&registry, &mut s, "file.open", json!({ "path": path }));
    assert_eq!(s.script.lists.transitions, ["SMASH TO:"]);
    run(&registry, &mut s, "file.save", json!({}));
    assert_eq!(std::fs::read_to_string(&path).unwrap(), saved);

    // An imported .fdx section stays byte for byte until the lists change.
    let source = "<?xml version=\"1.0\"?>\n<FinalDraft DocumentType=\"Script\" Version=\"5\">\n  <Content>\n    <Paragraph Type=\"Action\"><Text>One.</Text></Paragraph>\n  </Content>\n  <SmartType><Characters><Character>ANA</Character></Characters><Extensions/><SceneIntros Separator=\". \"><SceneIntro>INT</SceneIntro></SceneIntros><Locations/><TimesOfDay Separator=\" - \"><TimeOfDay>DAY</TimeOfDay></TimesOfDay><Transitions/></SmartType>\n</FinalDraft>\n";
    let script = storyteller_core::fdx::parse(source).unwrap();
    assert_eq!(script.lists.characters, ["ANA"]);
    assert_eq!(script.lists.intros, ["INT"]);
    assert!(storyteller_core::fdx::write(&script).contains("<Character>ANA</Character></Characters><Extensions/>"), "as it came");
    let mut changed = script.clone();
    changed.lists.characters.push("BEN".into());
    let written = storyteller_core::fdx::write(&changed);
    assert!(written.contains("<Character>BEN</Character>") && written.contains("Separator=\". \""));
    assert_eq!(written.matches("<SmartType").count(), 1);

    // A renamed character takes the list's entry along.
    let mut s = Session::new(storyteller_core::fountain::parse(RANGE_SCRIPT).unwrap());
    run(&registry, &mut s, "lists.add", json!({ "list": "characters", "entry": "Nora" }));
    run(&registry, &mut s, "character.rename", json!({ "from": "NORA", "to": "Emma" }));
    assert_eq!(s.script.lists.characters, ["EMMA"]);

    // A rebuild is what the script uses now, and offered suggestions include the lists.
    let mut s = Session::new(storyteller_core::fountain::parse(RANGE_SCRIPT).unwrap());
    run(&registry, &mut s, "lists.add", json!({ "list": "characters", "entry": "Gone" }));
    run(&registry, &mut s, "lists.rebuild", json!({}));
    assert_eq!(s.script.lists.characters, ["NORA"]);
    assert_eq!(s.script.lists.intros, ["INT."]);
}

#[test]
fn tab_after_a_name_can_open_the_extensions() {
    let registry = Registry::new();
    let mut s = Session::new(storyteller_core::fountain::parse("NORA\nIt held.\n").unwrap());
    // By default Tab starts a parenthetical (decision 3).
    run(&registry, &mut s, "flow.tab", json!({ "index": 0 }));
    assert_eq!(kinds(&s)[1], K::Parenthetical);
    let mut s = Session::new(storyteller_core::fountain::parse("NORA\nIt held.\n").unwrap());
    s.tab_opens_extensions = true;
    run(&registry, &mut s, "flow.tab", json!({ "index": 0 }));
    assert_eq!(s.script.elements[0].text, "NORA (");
    assert_eq!((s.caret.element, s.caret.offset), (0, "NORA (".len()));
    assert_eq!(storyteller_core::autocomplete::suggest(&s.script, K::Character, "NORA (")[0], "NORA (V.O.)");
    // With an extension already there, Tab goes on as usual.
    s.script.elements[0].text = "NORA (V.O.)".into();
    run(&registry, &mut s, "flow.tab", json!({ "index": 0 }));
    assert_eq!(kinds(&s)[1], K::Parenthetical);
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
fn revision_mode_marks_new_paragraphs_whole_and_marks_joins() {
    use storyteller_core::{Deletion, Revised};
    let registry = Registry::new();
    let mut s = Session::new(storyteller_core::fountain::parse("INT. HUT - DAY\n\nRain falls. Wind rises.\n").unwrap());
    // Out of revision mode nothing is marked.
    run(&registry, &mut s, "flow.enter", json!({ "index": 1, "offset": "Rain falls.".len() }));
    assert!(s.script.elements[2].revised.is_empty());
    run(&registry, &mut s, "element.merge_previous", json!({ "index": 2 }));
    assert!(s.script.elements[1].deletions.is_empty());
    assert_eq!(s.script.elements[1].text, "Rain falls.Wind rises.");
    run(&registry, &mut s, "element.set_text", json!({ "index": 1, "text": "Rain falls. Wind rises." }));

    run(&registry, &mut s, "revisions.mode", json!({}));
    let white = s.script.revisions.active.unwrap();
    // Enter splits off a paragraph that is new: it is revised whole; the one it left is not.
    run(&registry, &mut s, "flow.enter", json!({ "index": 1, "offset": "Rain falls.".len() }));
    assert_eq!(s.script.elements[2].text, "Wind rises.");
    assert_eq!(s.script.elements[2].revised, [Revised { from: 0, to: 11, set: white }]);
    assert!(s.script.elements[1].revised.is_empty());
    // Backspace joins it back: its marks come along and the join itself is marked.
    run(&registry, &mut s, "element.merge_previous", json!({ "index": 2 }));
    assert_eq!(s.script.elements[1].revised, [Revised { from: 11, to: 22, set: white }]);
    assert_eq!(s.script.elements[1].deletions, [Deletion { at: 11, set: white }]);
    run(&registry, &mut s, "edit.undo", json!({}));
    assert!(s.script.elements[1].deletions.is_empty() && s.script.elements.len() == 3, "one Undo");
    // An inserted paragraph is revised whole.
    let index =
        run(&registry, &mut s, "element.insert", json!({ "index": 2, "kind": "action", "text": "Thunder." }))["value"]["index"].as_u64().unwrap();
    assert_eq!(s.script.elements[index as usize].revised, [Revised { from: 0, to: 8, set: white }]);
    // So is every paragraph a paste makes, the text after the paste included.
    run(&registry, &mut s, "edit.paste_screenplay", json!({ "index": 1, "start": "Rain".len(), "text": "MARA\nGo.\n\nShe runs." }));
    let marked: Vec<_> = s.script.elements.iter().map(|e| (e.text.as_str(), !e.revised.is_empty())).collect();
    assert_eq!(
        marked[1..],
        [("Rain", false), ("MARA", true), ("Go.", true), ("She runs.", true), ("falls.", true), ("Wind rises.", true), ("Thunder.", true)]
    );
}

#[test]
fn lock_pages_and_unlock_all_pages_are_one_undo_each() {
    let registry = Registry::new();
    let text: String = (1..=60).map(|i| format!("Line {i}.\n\n")).collect();
    let mut s = Session::new(storyteller_core::fountain::parse(&format!("INT. HUT - DAY\n\n{text}")).unwrap());
    let enabled = |s: &Session, id: &str| registry.get(id).is_some_and(|c| (c.enabled)(s));
    assert!(enabled(&s, "pages.lock") && !enabled(&s, "pages.unlock_all"));
    let out = run(&registry, &mut s, "pages.lock", json!({ "revision_mode": true }));
    let pages = out["value"]["pages"].as_u64().unwrap();
    assert!(pages >= 2);
    assert_eq!(out["message"], format!("Locked {pages} pages"));
    assert!(s.script.pages_locked() && s.script.revisions.mode, "locked, in revision mode");
    assert!(!enabled(&s, "pages.lock") && enabled(&s, "pages.unlock_all"));
    assert!(registry.run("pages.lock", &mut s, &Default::default()).is_err(), "already locked");
    run(&registry, &mut s, "edit.undo", json!({}));
    assert!(!s.script.pages_locked() && !s.script.revisions.mode, "one Undo takes both back");
    run(&registry, &mut s, "pages.lock", json!({}));
    assert!(s.script.pages_locked() && !s.script.revisions.mode);
    assert_eq!(run(&registry, &mut s, "pages.unlock_all", json!({}))["value"]["pages"].as_u64(), Some(pages));
    assert!(!s.script.pages_locked());
    run(&registry, &mut s, "edit.undo", json!({}));
    assert!(s.script.pages_locked());
}

#[test]
fn lock_a_pages_and_unlock_a_pages() {
    let registry = Registry::new();
    let text: String = (1..=50).map(|i| format!("Line {i}.\n\n")).collect();
    let mut s = Session::new(storyteller_core::fountain::parse(&format!("INT. HUT - DAY\n\n{text}")).unwrap());
    let enabled = |s: &Session, id: &str| registry.get(id).is_some_and(|c| (c.enabled)(s));
    assert!(!enabled(&s, "pages.lock_a_pages") && !enabled(&s, "pages.unlock_a_pages"));
    run(&registry, &mut s, "pages.lock", json!({}));
    assert!(registry.run("pages.lock_a_pages", &mut s, &Default::default()).is_err(), "no A-pages yet");
    // Text added on page 1 flows onto 1A.
    run(&registry, &mut s, "element.insert", json!({ "index": 2, "kind": "action", "text": "A long new paragraph. ".repeat(40) }));
    let out = run(&registry, &mut s, "pages.lock_a_pages", json!({}));
    assert_eq!(out["message"], "Locked 1 A-page");
    assert_eq!(s.script.lock_level(), 1);
    assert!(enabled(&s, "pages.unlock_a_pages"));
    run(&registry, &mut s, "edit.undo", json!({}));
    assert_eq!(s.script.lock_level(), 0, "one Undo");
    run(&registry, &mut s, "edit.redo", json!({}));
    assert_eq!(run(&registry, &mut s, "pages.unlock_a_pages", json!({}))["value"]["pages"], 1);
    assert!(s.script.pages_locked() && s.script.lock_level() == 0, "Lock Pages' own locks stay");
}

#[test]
fn revision_mode_marks_typing_and_mark_and_clear_act_on_a_range() {
    let registry = Registry::new();
    let mut s = Session::new(storyteller_core::fountain::parse("INT. HUT - DAY\n\nRain **falls**.\n").unwrap());
    let out = run(&registry, &mut s, "revisions.mode", json!({}));
    assert!(s.script.revisions.mode);
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
