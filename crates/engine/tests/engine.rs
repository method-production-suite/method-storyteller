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
    run(&registry, &mut s, "flow.enter", json!({})); // → character
    run(&registry, &mut s, "flow.enter", json!({})); // empty character → action
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
    run(&registry, &mut s, "format.transition", json!({}));
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
