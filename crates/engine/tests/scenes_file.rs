#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

use serde_json::{Map, Value, json};
use storyteller_core::ElementKind;
use storyteller_engine::{Registry, Session, scenes_file};

const SCRIPT: &str = "\
Title: Field Station
Episode: 101
Author: Field Station writers
Draft date: Blue draft

ACT ONE

INT. FIELD STATION - RADIO ROOM - NIGHT #1#

MARA
Ridge base, come in.

TEO (O.S.)
Nothing.

MARA (CONT'D)
Again.

EXT. RIDGE ROAD - NIGHT #1A#

TEO
Go.

END OF ACT ONE

INT. HUT - DAY #2#

DANA
Gone.

.THE SAME ROOM, YEARS EARLIER

Wind.
";

fn session() -> Session {
    let mut script = storyteller_core::fountain::parse(SCRIPT).unwrap();
    for element in &mut script.elements {
        match element.text.as_str() {
            "ACT ONE" => element.kind = ElementKind::NewAct,
            "END OF ACT ONE" => element.kind = ElementKind::EndOfAct,
            _ => {}
        }
    }
    let first = script.elements.iter().position(|e| e.kind == ElementKind::SceneHeading).unwrap();
    let meta = &mut script.elements[first].scene;
    meta.title = "The call".into();
    meta.summary = "Mara tries the ridge base.".into();
    meta.color = Some("#5b8fc9".into());
    let third = script.elements.iter().enumerate().filter(|(_, e)| e.kind == ElementKind::SceneHeading).nth(2).unwrap().0;
    script.omit_scene(third).unwrap();
    Session::new(script)
}

fn file(session: &mut Session) -> Value {
    serde_json::from_str(&scenes_file::write(session, "2026-11-04")).unwrap()
}

#[test]
fn the_scenes_file_carries_each_scene_with_its_own_pages() {
    let mut s = session();
    let v = file(&mut s);
    assert_eq!(v["format"], "method-script-scenes");
    assert_eq!(v["version"], 1);
    assert_eq!(v["title"], "Field Station");
    assert_eq!(v["episode"], "101");
    assert_eq!(v["written_by"], "Field Station writers");
    assert_eq!(v["draft"], "Blue draft");
    assert_eq!(v["generated"], "2026-11-04");
    assert_eq!(v["pages"], 1);
    let scenes = v["scenes"].as_array().unwrap();
    assert_eq!(scenes.len(), 4);
    let sum: u64 = scenes.iter().map(|s| s["eighths"].as_u64().unwrap()).sum();
    assert_eq!(v["eighths"].as_u64(), Some(sum));

    let a = &scenes[0];
    assert_eq!(a["index"], 1);
    assert_eq!(a["number"], "1");
    assert_eq!(a["heading"], "INT. FIELD STATION - RADIO ROOM - NIGHT");
    assert_eq!((&a["int_ext"], &a["set"], &a["time"]), (&json!("INT"), &json!("FIELD STATION - RADIO ROOM"), &json!("NIGHT")));
    assert_eq!(a["page"], "1");
    assert_eq!(a["cast"], json!(["MARA", "TEO"]), "first-seen order, extensions dropped");
    assert_eq!((&a["title"], &a["summary"], &a["color"]), (&json!("The call"), &json!("Mara tries the ridge base."), &json!("#5B8FC9")));
    assert_eq!(a["act"], "ACT ONE");

    assert_eq!(scenes[1]["number"], "1A");
    assert_eq!(scenes[1]["act"], "ACT ONE");
    // Past the end of the act, and omitted: no cast, one eighth.
    let c = &scenes[2];
    assert!(c.get("act").is_none());
    assert_eq!(c["omitted"], true);
    assert_eq!(c["cast"], json!([]));
    assert_eq!(c["eighths"], 1);
    // No number and no INT. or EXT.: Method Schedule numbers it by place and splits it itself.
    let d = &scenes[3];
    assert_eq!(d["index"], 4);
    for key in ["number", "int_ext", "set", "time", "omitted", "title"] {
        assert!(d.get(key).is_none(), "{key} is left out, not null");
    }
    assert!(d["eighths"].as_u64().unwrap() >= 1);
}

#[test]
fn the_file_is_pretty_json_in_the_agreed_order_with_no_nulls() {
    let mut s = session();
    let text = scenes_file::write(&mut s, "2026-11-04");
    assert!(text.ends_with("}\n") && !text.ends_with("\n\n"));
    assert!(!text.contains("null"));
    let order = [
        "\"format\"",
        "\"version\"",
        "\"title\"",
        "\"episode\"",
        "\"written_by\"",
        "\"draft\"",
        "\"generated\"",
        "\"source_hash\"",
        "\"pages\"",
        "\"eighths\"",
        "\"scenes\"",
    ];
    let at: Vec<usize> = order.iter().map(|k| text.find(k).unwrap()).collect();
    assert!(at.windows(2).all(|w| w[0] < w[1]), "top-level keys in order");
    let scene = [
        "\"index\"",
        "\"number\"",
        "\"heading\"",
        "\"int_ext\"",
        "\"set\"",
        "\"time\"",
        "\"page\"",
        "\"eighths\"",
        "\"cast\"",
        "\"title\"",
        "\"summary\"",
        "\"color\"",
        "\"act\"",
    ];
    let first = &text[text.find("\"scenes\"").unwrap()..];
    let at: Vec<usize> = scene.iter().map(|k| first.find(k).unwrap()).collect();
    assert!(at.windows(2).all(|w| w[0] < w[1]), "scene keys in order");
}

#[test]
fn the_hash_follows_the_script_not_the_day() {
    let hash = |s: &mut Session, day: &str| -> String {
        let v: Value = serde_json::from_str(&scenes_file::write(s, day)).unwrap();
        v["source_hash"].as_str().unwrap().to_string()
    };
    let mut s = session();
    let first = hash(&mut s, "2026-11-04");
    assert_eq!(first.len(), 16);
    assert!(first.chars().all(|c| c.is_ascii_hexdigit()));
    assert_eq!(hash(&mut s, "2027-01-01"), first);
    s.script.elements.last_mut().unwrap().text = "Rain.".into();
    s.invalidate_layout();
    assert_ne!(hash(&mut s, "2026-11-04"), first);
}

#[test]
fn the_command_writes_the_file() {
    let dir = std::env::temp_dir().join(format!("storyteller-mscenes-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("Field Station.mscenes");
    let mut s = session();
    let mut params = Map::new();
    params.insert("path".into(), json!(path.display().to_string()));
    Registry::new().run("file.export_scenes", &mut s, &params).unwrap();
    let v: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(v["scenes"].as_array().unwrap().len(), 4);
    assert_eq!(v["generated"].as_str().unwrap().len(), 10);
    std::fs::remove_dir_all(&dir).unwrap();
}
