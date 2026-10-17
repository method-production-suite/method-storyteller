#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

use storyteller_core::fountain;
use storyteller_core::model::{Element, ElementKind as K, Script, TitlePage};

fn fixture() -> String {
    std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/lighthouse.fountain")).unwrap()
}

fn kinds(script: &Script) -> Vec<K> {
    script.elements.iter().map(|e| e.kind).collect()
}

#[test]
fn parses_the_fixture() {
    let script = fountain::parse(&fixture()).unwrap();
    assert_eq!(script.title_page.get("Title"), Some("**THE LIGHTHOUSE KEEPER'S DAUGHTER**"));
    assert_eq!(script.title_page.get("Contact"), Some("storyteller@example.com\nNowhere, Nevada"));
    assert_eq!(script.title_page.get("Draft date"), Some("15 October 2026"));
    assert_eq!(
        kinds(&script),
        vec![
            K::Action,
            K::SceneHeading,
            K::Action,
            K::Action,
            K::Character,
            K::Parenthetical,
            K::Dialogue,
            K::Action,
            K::SceneHeading,
            K::Action,
            K::Character,
            K::Dialogue,
            K::Character,
            K::Dialogue,
            K::Character,
            K::Dialogue,
            K::Character,
            K::Dialogue,
            K::Action,
            K::Action,
            K::Transition,
            K::SceneHeading,
            K::Lyrics,
            K::Centered,
            K::PageBreak,
            K::Section,
            K::Synopsis,
            K::SceneHeading,
            K::Action,
        ]
    );
    let e = &script.elements;
    assert_eq!(e[0].text, "FADE IN:");
    assert_eq!(e[1].text, "EXT. LIGHTHOUSE - NIGHT");
    assert_eq!(e[1].scene_number.as_deref(), Some("1"));
    assert_eq!(e[8].scene_number.as_deref(), Some("2"));
    assert_eq!(e[14].text, "OWEN (CONT'D)");
    assert!(e[16].dual);
    assert_eq!(e[16].text, "MARA");
    assert_eq!(e[19].text, "The lens stops.");
    assert_eq!(e[20].text, "CUT TO:");
    assert_eq!(e[21].text, "MEMORY - THE SAME ROOM, YEARS EARLIER");
    assert_eq!(e[22].text, "Turn, little light, turn for the sea,\nTurn till the morning comes back to me.");
    assert_eq!(e[23].text, "THE END");
    assert_eq!((e[25].text.as_str(), e[25].level), ("Act Two", 1));
    assert_eq!(e[26].text, "Mara learns the clockwork.");
    assert_eq!(e[28].text, "SILENCE. The boats rock at their moorings.");
}

#[test]
fn fixture_round_trips() {
    let first = fountain::parse(&fixture()).unwrap();
    let written = fountain::write(&first);
    let second = fountain::parse(&written).unwrap();
    assert_eq!(first, second, "written:\n{written}");
    assert_eq!(fountain::write(&second), written, "writing is stable");
}

fn round_trip(script: Script) {
    let written = fountain::write(&script);
    let back = fountain::parse(&written).unwrap();
    assert_eq!(back, script, "written:\n{written}");
}

#[test]
fn forced_elements_round_trip() {
    let mut title = TitlePage::default();
    title.set("Title", "Forced");
    round_trip(Script {
        title_page: TitlePage::default(),
        elements: vec![
            Element::new(K::Action, "Title: this is action, not a title page"),
            Element::new(K::SceneHeading, "THE MOON - NIGHT"),
            Element::new(K::Action, "INT. THIS LOOKS LIKE A HEADING"),
            Element::new(K::Action, "BOB\nlooks like a cue but is action"),
            Element::new(K::Character, "McCLANE"),
            Element::new(K::Dialogue, "Yippee."),
            Element::new(K::Character, "NOBODY"),
            Element::new(K::Transition, "FADE OUT."),
            Element::new(K::Transition, "CUT TO:"),
            Element::new(K::Action, "Line one\n\nline three after an empty line"),
            Element::new(K::Action, "   indented action"),
            Element::new(K::Action, "#hashtag is not a section"),
            Element::new(K::Action, "=== is not a page break here"),
            Element::new(K::Action, ""),
            Element::new(K::SceneHeading, ""),
        ],
        fdx: None,
        osf: None,
    });
    round_trip(Script {
        title_page: title,
        elements: vec![
            Element::new(K::SceneHeading, "INT. HOUSE - DAY").with_scene_number("12A"),
            Element::new(K::Character, "ANNA (V.O.)"),
            Element::new(K::Parenthetical, "(beat)"),
            Element::new(K::Dialogue, "First line\nsecond line"),
            Element::new(K::Parenthetical, "(then)"),
            Element::new(K::Dialogue, "More."),
            Element::new(K::Character, "BEN").with_dual(true),
            Element::new(K::Dialogue, "*Together* **now** _please_"),
            Element::new(K::Lyrics, "La la la"),
            Element::new(K::Centered, "INTERMISSION"),
            Element::new(K::PageBreak, ""),
        ],
        fdx: None,
        osf: None,
    });
}

#[test]
fn spec_examples() {
    // A scene heading needs a blank line after it; otherwise it is action.
    let script = fountain::parse("Int. house - day\nstill action").unwrap();
    assert_eq!(kinds(&script), [K::Action]);
    // `FADE IN:` does not end in `TO:`, so it is action (printed at the action margin).
    let script = fountain::parse("FADE IN:\n\nEXT. SEA - DAY").unwrap();
    assert_eq!(kinds(&script), [K::Action, K::SceneHeading]);
    // A cue needs dialogue after it.
    let script = fountain::parse("BOB\n\nAction.").unwrap();
    assert_eq!(kinds(&script), [K::Action, K::Action]);
    // Boneyard is removed; notes are kept inline.
    let script = fountain::parse("Visible /* hidden\nstill hidden */ text [[note]]").unwrap();
    assert_eq!(script.elements[0].text, "Visible  text [[note]]");
    // CRLF and a byte-order mark are accepted.
    let script = fountain::parse("\u{feff}EXT. FIELD - DAY\r\n\r\nGrass.\r\n").unwrap();
    assert_eq!(kinds(&script), [K::SceneHeading, K::Action]);
    // Forced transition and centered text.
    let script = fountain::parse("> Burn to white.\n\n> THE END <").unwrap();
    assert_eq!(kinds(&script), [K::Transition, K::Centered]);
    // A two-space line keeps dialogue going.
    let script = fountain::parse("DEALER\nTen.\n  \nJack.").unwrap();
    assert_eq!(script.elements[1].text, "Ten.\n\nJack.");
}
