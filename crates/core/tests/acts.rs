#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

//! Television acts: New Act and End of Act, in every format and on the page. The script is
//! invented.

use storyteller_core::layout::Layout;
use storyteller_core::model::{ElementKind as K, Script, act_kind};
use storyteller_core::paginate::paginate;
use storyteller_core::{fdx, fountain, osf};

const EPISODE: &str = "\
> TEASER <

INT. FIELD STATION - NIGHT

The radio crackles.

> END OF TEASER <

> ACT ONE <

EXT. RIDGE - DAWN

Wind.

> END OF ACT ONE <

> TAG <

INT. FIELD STATION - DAY

Quiet.
";

fn kinds(script: &Script) -> Vec<(K, String)> {
    script.elements.iter().map(|e| (e.kind, e.text.clone())).collect()
}

#[test]
fn act_lines_are_recognised() {
    for opens in ["TEASER", "Cold Open", "ACT ONE", "act 2", "ACT FIVE", "TAG", "PROLOGUE", "_ACT THREE_"] {
        assert_eq!(act_kind(opens), Some(K::NewAct), "{opens}");
    }
    for closes in ["END OF ACT ONE", "End of Teaser", "END OF ACT 4"] {
        assert_eq!(act_kind(closes), Some(K::EndOfAct), "{closes}");
    }
    for neither in ["THE END", "INTERMISSION", "ACT", "END OF", "ACTION PACKED SEQUENCE", "ACT ONE, SCENE TWO: THE RIDGE"] {
        assert_eq!(act_kind(neither), None, "{neither}");
    }
}

#[test]
fn fountain_reads_centred_act_lines_as_acts_and_writes_them_back() {
    let script = fountain::parse(EPISODE).unwrap();
    let k: Vec<K> = script.elements.iter().map(|e| e.kind).collect();
    assert_eq!(
        k,
        [
            K::NewAct,
            K::SceneHeading,
            K::Action,
            K::EndOfAct,
            K::NewAct,
            K::SceneHeading,
            K::Action,
            K::EndOfAct,
            K::NewAct,
            K::SceneHeading,
            K::Action
        ]
    );
    let again = fountain::parse(&fountain::write(&script)).unwrap();
    assert_eq!(kinds(&again), kinds(&script), "Fountain keeps the acts");
    // Other centred lines stay centred.
    let centred = fountain::parse("> INTERMISSION <\n").unwrap();
    assert_eq!(centred.elements[0].kind, K::Centered);
}

#[test]
fn fdx_and_osf_keep_the_acts_both_ways() {
    let script = fountain::parse(EPISODE).unwrap();
    let fdx_text = fdx::write(&script);
    assert!(fdx_text.contains("Type=\"New Act\"") && fdx_text.contains("Type=\"End of Act\""));
    assert_eq!(kinds(&fdx::parse(&fdx_text).unwrap()), kinds(&script), ".fdx keeps the acts");
    let osf_text = osf::write(&script);
    assert!(osf_text.contains("<style basestylename=\"New Act\"/>"), "OSF names the style");
    assert!(osf_text.contains("<style name=\"New Act\""), "a new OSF document defines the style");
    assert_eq!(kinds(&osf::parse(&osf_text).unwrap()), kinds(&script), "OSF keeps the acts");
    // And across: .fdx to OSF to .fdx.
    let across = fdx::parse(&fdx::write(&osf::parse(&osf_text).unwrap())).unwrap();
    assert_eq!(kinds(&across), kinds(&script));
}

#[test]
fn each_act_starts_a_new_page() {
    let script = fountain::parse(EPISODE).unwrap();
    let pages = paginate(&script, &Layout::default());
    let page_of = |text: &str| pages.element_pages[script.elements.iter().position(|e| e.text == text).unwrap()];
    assert_eq!(page_of("TEASER"), 1, "an act at the very top adds no blank page");
    assert_eq!(page_of("END OF TEASER"), 1);
    assert_eq!(page_of("ACT ONE"), 2);
    assert_eq!(page_of("END OF ACT ONE"), 2);
    assert_eq!(page_of("TAG"), 3);
    assert_eq!(pages.page_count(), 3);
}

#[test]
fn scene_letters_run_a_to_z_then_double() {
    use storyteller_core::model::scene_letter;
    assert_eq!([0, 1, 25, 26, 27, 51, 52].map(scene_letter), ["A", "B", "Z", "AA", "AB", "AZ", "BA"]);
}

/// A multi-camera script with two scenes written.
fn multi_cam() -> Script {
    use storyteller_core::model::TvShape;
    let mut script = Script::tv(TvShape::MultiCam);
    let written = fountain::parse(
        "INT. FIELD STATION - NIGHT\n\nNora tunes the radio.\n\nNORA\nAnything?\n\nOWEN\nStatic.\n\nNORA (O.S.)\nKeep trying.\n\nEXT. RIDGE - DAWN\n\nWind.\n",
    )
    .unwrap();
    // Into the cold open, in place of its empty heading.
    script.elements.splice(1..2, written.elements);
    script.keep_letters_in_step();
    script
}

#[test]
fn a_multi_camera_script_is_lettered_and_set_out_its_own_way() {
    use storyteller_core::model::TvShape;
    let fresh = Script::tv(TvShape::MultiCam);
    let acts: Vec<&str> = fresh.elements.iter().filter(|e| e.kind == K::NewAct).map(|e| e.text.as_str()).collect();
    assert_eq!(acts, ["COLD OPEN", "ACT ONE", "ACT TWO", "TAG"]);
    assert!(fresh.is_lettered());
    let layout = Layout::for_script(&fresh);
    assert!(layout.scene_heading.new_page && layout.action.caps && layout.dialogue.spacing == 2);
    assert_eq!(Layout::for_script(&Script::tv(TvShape::HalfHour)), Layout::default(), "the other shapes keep the defaults");

    let script = multi_cam();
    let letters: Vec<_> = script.elements.iter().filter(|e| e.kind == K::SceneHeading).map(|e| e.scene_number.clone().unwrap()).collect();
    assert_eq!(letters, ["A", "B", "C", "D", "E"], "a new scene gets its letter, the rest move on");
    let heading = script.elements.iter().position(|e| e.text == "INT. FIELD STATION - NIGHT").unwrap();
    assert_eq!(script.scene_cast(heading), ["NORA", "OWEN"], "in order of speaking, once each, without extensions");

    let pages = paginate(&script, &Layout::for_script(&script));
    use storyteller_core::paginate::LineRole;
    let page_of = |text: &str| {
        pages.pages.iter().find(|p| p.lines.iter().any(|l| l.text() == text)).map(|p| p.number).unwrap_or_else(|| panic!("{text} is printed"))
    };
    assert_ne!(page_of("INT. FIELD STATION - NIGHT"), page_of("EXT. RIDGE - DAWN"), "each scene on its own page");
    let first = pages.pages.iter().find(|p| p.lines.iter().any(|l| l.text() == "INT. FIELD STATION - NIGHT")).unwrap();
    let body: Vec<_> = first.lines.iter().filter(|l| !matches!(l.role, LineRole::PageNumber | LineRole::SceneNumber)).collect();
    let at = body.iter().position(|l| l.text() == "INT. FIELD STATION - NIGHT").unwrap();
    assert_eq!((body[at + 1].role, body[at + 1].text()), (LineRole::Cast, "(NORA, OWEN)".to_string()), "the cast under the heading");
    assert_eq!(body[at + 1].row, body[at].row + 1);
    assert!(first.lines.iter().any(|l| l.role == LineRole::SceneNumber && l.text() == "A"), "the letter in the margin");
    assert!(body.iter().any(|l| l.text() == "NORA TUNES THE RADIO."), "action in capitals");
}

#[test]
fn a_multi_camera_script_keeps_its_letters_and_settings_in_fdx_and_osf() {
    let script = multi_cam();
    let layout = Layout::for_script(&script);
    for (name, back) in [("fdx", fdx::parse(&fdx::write(&script)).unwrap()), ("osf", osf::parse(&osf::write(&script)).unwrap())] {
        assert!(back.is_lettered(), "{name}");
        let letters = |s: &Script| s.elements.iter().filter_map(|e| e.scene_number.clone()).collect::<Vec<_>>();
        assert_eq!(letters(&back), letters(&script), "{name}");
        assert_eq!(Layout::for_script(&back), layout, "{name}");
    }
}

#[test]
fn numbering_letters_inserted_scenes() {
    use storyteller_core::model::{Numbering, letters};
    assert_eq!([0, 7, 8, 13, 14, 25, 26].map(|n| letters(n, false)), ["A", "H", "I", "N", "O", "Z", "AA"]);
    assert_eq!([7, 8, 12, 13, 23, 24].map(|n| letters(n, true)), ["H", "J", "N", "P", "Z", "AA"], "without I and O");
    let numbers = |script: &Script| {
        script.elements.iter().filter(|e| e.kind == K::SceneHeading).map(|e| e.scene_number.clone().unwrap_or_default()).collect::<Vec<_>>()
    };
    let mut script = fountain::parse(
        "INT. NEW - DAY\n\nINT. A - DAY #1#\n\nINT. NEW - DAY\n\nINT. NEW - DAY\n\nINT. B - DAY #2#\n\nINT. C - DAY #2A#\n\nINT. NEW - DAY\n\nINT. D - DAY #2B#\n\nINT. NEW - DAY\n",
    )
    .unwrap();
    let changed = script.number_with(Numbering { keep_existing: true, ..Numbering::default() });
    assert_eq!(numbers(&script), ["A1", "1", "1A", "1B", "2", "2A", "2AA", "2B", "2C"]);
    assert_eq!(changed, 5);
    // From scene 3 on, fresh, starting at 10.
    let from = script.scene_indices()[2];
    script.number_with(Numbering { start: 10, from, ..Numbering::default() });
    assert_eq!(numbers(&script), ["A1", "1", "10", "11", "12", "13", "14", "15", "16"]);
    // By hand: a number another scene has is refused.
    let first = script.scene_indices()[0];
    assert!(script.set_scene_number(first, "12").unwrap_err().contains("scene 5"));
    script.set_scene_number(first, "0a").unwrap();
    assert_eq!(numbers(&script)[0], "0A");
    script.set_scene_number(first, "").unwrap();
    assert_eq!(numbers(&script)[0], "");
}
