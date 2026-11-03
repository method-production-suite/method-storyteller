#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

use serde_json::{Map, Value, json};
use storyteller_engine::{Registry, Session, reports};

fn run(registry: &Registry, session: &mut Session, id: &str, params: Value) -> Result<Value, String> {
    let params: Map<String, Value> = params.as_object().cloned().unwrap_or_default();
    registry.run(id, session, &params).map(|o| serde_json::to_value(o).unwrap()).map_err(|e| e.to_string())
}

const SCRIPT: &str = "\
INT. HUT - DAY #1#

NORA
It held.

OWEN (O.S.)
Did it?

NORA (CONT'D)
For now, yes.

EXT. RIDGE - DAWN #2#

OWEN
Go.

I/E. HUT - NIGHT #2A#

Wind. *Nobody* speaks.

.MEMORY - THE SAME ROOM, YEARS EARLIER #3#

DANA
\"Again,\" she says.

.4. EXT. RIDGE - LATER #4#
";

fn session() -> Session {
    Session::new(storyteller_core::fountain::parse(SCRIPT).unwrap())
}

#[test]
fn the_scene_report_splits_headings_and_lists_who_speaks() {
    let mut s = session();
    let rows = reports::session_scenes(&mut s);
    let labels: Vec<String> = rows.iter().map(reports::SceneRow::label).collect();
    assert_eq!(labels, ["1", "2", "2A", "3", "4"]);
    let fields: Vec<(Option<&str>, &str, Option<&str>)> = rows.iter().map(|r| (r.int_ext, r.location.as_str(), r.time.as_deref())).collect();
    assert_eq!(
        fields,
        [
            (Some("INT"), "HUT", Some("DAY")),
            (Some("EXT"), "RIDGE", Some("DAWN")),
            (Some("INT/EXT"), "HUT", Some("NIGHT")),
            (None, "MEMORY - THE SAME ROOM, YEARS EARLIER", None),
            (Some("EXT"), "RIDGE", Some("LATER")),
        ],
        "a heading without INT. or EXT. is all location; a scene number typed in front is passed over"
    );
    assert_eq!(rows[0].characters, ["NORA", "OWEN"], "first-seen order, extensions dropped, once each");
    assert!(rows[2].characters.is_empty());
    assert!(rows.iter().all(|r| r.page == "1" && r.eighths > 0));
    let lengths: Vec<u32> = s.scene_lengths().iter().map(|l| l.eighths).collect();
    assert_eq!(rows.iter().map(|r| r.eighths).collect::<Vec<_>>(), lengths, "the lengths the scene list shows");
}

#[test]
fn locations_and_cast_come_from_the_scenes() {
    let mut s = session();
    let scenes = reports::session_scenes(&mut s);
    let locations = reports::locations(&scenes);
    let hut = locations.iter().find(|l| l.location == "HUT").unwrap();
    assert_eq!((hut.scenes, hut.int, hut.ext, hut.int_ext, hut.unmarked), (2, 1, 0, 1, 0));
    assert_eq!(hut.eighths, scenes[0].eighths + scenes[2].eighths);
    assert_eq!(locations.iter().map(|l| l.location.as_str()).collect::<Vec<_>>(), ["HUT", "MEMORY - THE SAME ROOM, YEARS EARLIER", "RIDGE"]);

    let cast = reports::cast(&scenes);
    let by_name: Vec<(&str, Vec<&str>)> = cast.iter().map(|r| (r.name.as_str(), r.scenes.iter().map(String::as_str).collect())).collect();
    assert_eq!(by_name, [("NORA", vec!["1"]), ("OWEN", vec!["1", "2"]), ("DANA", vec!["3"])]);
}

#[test]
fn an_omitted_scene_counts_no_pages_and_no_cast() {
    let registry = Registry::new();
    let mut s = session();
    s.caret.element = s.script.scene_indices()[1];
    run(&registry, &mut s, "scene.omit", json!({})).unwrap();
    let scenes = reports::session_scenes(&mut s);
    let ridge = scenes.iter().find(|r| r.label() == "2").unwrap();
    assert!(ridge.omitted);
    assert_eq!((ridge.eighths, ridge.characters.len()), (0, 0));
    let locations = reports::locations(&scenes);
    let ridge = locations.iter().find(|l| l.location == "RIDGE").unwrap();
    assert_eq!(ridge.scenes, 1, "only the scene that is not omitted");
    let owen = reports::cast(&scenes).into_iter().find(|r| r.name == "OWEN").unwrap();
    assert_eq!(owen.scenes, ["1"]);
}

#[test]
fn reports_write_csv_with_a_header_and_quoting() {
    let registry = Registry::new();
    let mut s = session();
    let dir = tempfile::tempdir().unwrap();
    for (id, header) in [
        ("report.scenes", "Scene,Heading,INT/EXT,Location,Time,Page,Length,Eighths,Characters,Title,Omitted"),
        ("report.locations", "Location,Scenes,Pages,Eighths,INT,EXT,INT/EXT,Unmarked"),
        ("report.cast", "Character,Scenes,Scene list"),
        ("character.table", "Character,Speeches,Scenes,Words,First scene,Scene list"),
    ] {
        let path = dir.path().join(format!("{id}.csv"));
        let outcome = run(&registry, &mut s, id, json!({ "path": path.display().to_string() })).unwrap();
        assert!(outcome["message"].as_str().unwrap().starts_with("Exported"), "{id}");
        assert!(!outcome["value"].is_null(), "{id} still returns its rows");
        let text = std::fs::read_to_string(&path).unwrap();
        assert_eq!(text.split("\r\n").next(), Some(header), "{id}");
        assert!(text.ends_with("\r\n"));
    }
    let scenes = std::fs::read_to_string(dir.path().join("report.scenes.csv")).unwrap();
    assert!(scenes.contains("\r\n1,INT. HUT - DAY,INT,HUT,DAY,1,"), "{scenes}");
    assert!(scenes.contains(",\"NORA, OWEN\","), "a field with a comma is quoted");
    assert!(scenes.contains("\"MEMORY - THE SAME ROOM, YEARS EARLIER\""));
    let cast = std::fs::read_to_string(dir.path().join("report.cast.csv")).unwrap();
    assert!(cast.contains("\r\nOWEN,2,\"1, 2\"\r\n"), "{cast}");

    assert_eq!(reports::csv(&["A"], [vec!["say \"hi\"".to_string()]]), "A\r\n\"say \"\"hi\"\"\"\r\n");
    // Script text that starts like a formula is never read as one.
    let cells = ["=HYPERLINK(\"x\")", "+1", "-2", "@SUM(A1)", "\tTAB", "\rCR", "\u{ff1d}1", "\u{ff20}A", "A=1", "7"];
    let row = reports::csv(&["A"], cells.iter().map(|c| vec![(*c).to_string()]));
    let lines: Vec<&str> = row.split("\r\n").skip(1).collect();
    assert_eq!(&lines[..4], ["\"'=HYPERLINK(\"\"x\"\")\"", "'+1", "'-2", "'@SUM(A1)"]);
    assert_eq!(lines[4], "'\tTAB");
    assert!(row.contains("\"'\rCR\""), "a carriage return is quoted too");
    assert!(row.contains("\r\n'\u{ff1d}1\r\n'\u{ff20}A\r\nA=1\r\n7\r\n"), "full-width forms guarded; text that only contains them is not");
    let wrong = run(&registry, &mut s, "report.scenes", json!({ "path": dir.path().join("x.txt").display().to_string() }));
    assert!(wrong.unwrap_err().contains(".csv"));
}

#[test]
fn reports_print_as_pdf_tables() {
    let registry = Registry::new();
    let mut s = session();
    let dir = tempfile::tempdir().unwrap();
    for id in ["report.scenes", "report.locations", "report.cast", "character.table"] {
        let path = dir.path().join(format!("{id}.pdf"));
        let outcome = run(&registry, &mut s, id, json!({ "path": path.display().to_string() })).unwrap();
        assert!(outcome["message"].as_str().unwrap().starts_with("Exported"), "{id}");
        let bytes = std::fs::read(&path).unwrap();
        assert!(bytes.starts_with(b"%PDF-"), "{id}");
    }
    let wrong = run(&registry, &mut s, "report.cast", json!({ "path": dir.path().join("x.txt").display().to_string() }));
    assert!(wrong.unwrap_err().contains(".pdf"));

    // A long report runs onto more pages than a short one, each a page of its own in the file.
    let long: String = (1..=150).map(|n| format!("INT. ROOM {n} - DAY\n\nNORA\nLine {n}.\n\n")).collect();
    let mut big = Session::new(storyteller_core::fountain::parse(&long).unwrap());
    let table = reports::table_for(&mut big, "scenes").unwrap();
    let short = reports::table_for(&mut s, "scenes").unwrap().pdf(None, (612.0, 792.0)).unwrap();
    let many = table.pdf(Some("Field Station"), (612.0, 792.0)).unwrap();
    assert!(many.len() > short.len());
}

#[test]
fn the_printed_scene_table_leaves_out_what_the_csv_repeats() {
    let registry = Registry::new();
    let mut s = session();
    let table = reports::table_for(&mut s, "scenes").unwrap();
    let shown: Vec<&str> = table.shown().iter().map(|&c| table.header[c]).collect();
    assert_eq!(shown, ["Scene", "INT/EXT", "Location", "Time", "Page", "Length", "Characters"], "no scene has a title");
    assert_eq!(table.summary.split(", ").next(), Some("5 scenes"));
    assert_eq!(reports::table_for(&mut s, "locations").unwrap().summary, "3 locations");
    assert_eq!(reports::table_for(&mut s, "cast").unwrap().summary, "3 speaking characters");

    s.caret.element = s.script.scene_indices()[1];
    run(&registry, &mut s, "scene.omit", json!({})).unwrap();
    let table = reports::table_for(&mut s, "scenes").unwrap();
    assert!(table.shown().iter().any(|&c| table.header[c] == "Omitted"), "printed once a scene is omitted");
    assert!(table.summary.starts_with("4 scenes,") && table.summary.ends_with(", 1 omitted"), "{}", table.summary);
}

#[test]
fn the_cast_command_gives_both_directions() {
    let registry = Registry::new();
    let mut s = session();
    let value = run(&registry, &mut s, "report.cast", json!({})).unwrap()["value"].clone();
    assert_eq!(value["scenes"][0], json!({ "scene": "1", "characters": ["NORA", "OWEN"] }));
    assert_eq!(value["characters"][1], json!({ "name": "OWEN", "scenes": ["1", "2"] }));
}

#[test]
fn the_character_table_lists_its_scenes() {
    let script = storyteller_core::fountain::parse(SCRIPT).unwrap();
    let table = storyteller_engine::commands::character_table(&script);
    let owen = table.iter().find(|r| r.name == "OWEN").unwrap();
    assert_eq!(owen.scene_list, ["1", "2"]);
}

#[test]
fn plain_text_reads_as_the_page_with_no_markup() {
    let registry = Registry::new();
    let mut s = session();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("script.txt");
    run(&registry, &mut s, "file.export_text", json!({ "path": path.display().to_string() })).unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.contains("Wind. Nobody speaks."), "emphasis markup is gone:\n{text}");
    assert!(!text.contains('*') && !text.contains('#'));
    let heading = text.lines().find(|l| l.contains("INT. HUT - DAY")).unwrap();
    assert!(heading.trim_start().starts_with("1   INT."), "the scene number in the margin, then the heading: {heading:?}");
    let cue = text.lines().find(|l| l.trim() == "NORA").unwrap();
    assert!(cue.starts_with("      "), "a cue sits indented, as on the page: {cue:?}");
    assert!(!text.lines().next().unwrap_or("").is_empty(), "no blank lines before the first text");

    // A longer script breaks pages with a form feed.
    let long = format!("INT. A - DAY\n\n{}", "Action.\n\n".repeat(120));
    let mut s = Session::new(storyteller_core::fountain::parse(&long).unwrap());
    let pages = s.paginated().page_count();
    assert!(pages > 1);
    assert_eq!(reports::plain_text(s.paginated()).matches('\u{c}').count(), pages - 1);
}
