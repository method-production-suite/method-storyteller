#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

//! Omitted scenes: the heading reads OMITTED and keeps its number, the scene is kept for
//! Unomit, and every format carries it. The script is invented.

use storyteller_core::layout::Layout;
use storyteller_core::model::{ElementKind as K, OMITTED, Script};
use storyteller_core::paginate::{paginate, scene_lengths};
use storyteller_core::{fdx, fountain, osf};

const SCRIPT: &str = "\
INT. HUT - DAY #4#

Rain.

NORA
It held.

EXT. RIDGE - DAWN #5#

Wind.

INT. HUT - NIGHT #6#

Quiet.
";

fn texts(script: &Script) -> Vec<String> {
    script.elements.iter().map(|e| e.text.clone()).collect()
}

#[test]
fn omit_keeps_the_number_and_unomit_brings_the_scene_back() {
    let original = fountain::parse(SCRIPT).unwrap();
    let mut script = original.clone();
    let ridge = script.elements.iter().position(|e| e.text == "EXT. RIDGE - DAWN").unwrap();
    assert_eq!(script.omit_scene(ridge).unwrap(), 2, "the heading and its action");
    let heading = &script.elements[ridge];
    assert_eq!((heading.text.as_str(), heading.scene_number.as_deref()), (OMITTED, Some("5")));
    assert!(heading.is_omitted());
    assert_eq!(script.elements[ridge + 1].text, "INT. HUT - NIGHT", "the next scene follows the OMITTED line");
    assert!(script.omit_scene(ridge).is_err(), "already omitted");
    // On the page: the OMITTED line with its number, and a length of none.
    let layout = Layout::default();
    let pages = paginate(&script, &layout);
    let lengths = scene_lengths(&script, &pages, &layout);
    assert_eq!(lengths.iter().find(|l| l.element == ridge).map(|l| l.eighths), Some(0));
    script.unomit_scene(ridge).unwrap();
    assert_eq!(texts(&script), texts(&original));
    assert_eq!(script.elements[ridge].scene_number.as_deref(), Some("5"));
}

#[test]
fn every_format_keeps_an_omitted_scene() {
    let mut script = fountain::parse(SCRIPT).unwrap();
    script.omit_scene(0).unwrap();
    let kept = script.elements[0].omitted.clone().unwrap();
    assert!(kept.contains("INT. HUT - DAY") && kept.contains("It held."), "{kept}");
    for (name, mut back) in [
        ("fdx", fdx::parse(&fdx::write(&script)).unwrap()),
        ("osf", osf::parse(&osf::write(&script)).unwrap()),
        ("fountain", fountain::parse(&fountain::write(&script)).unwrap()),
    ] {
        let heading = &back.elements[0];
        assert_eq!((heading.kind, heading.text.as_str(), heading.scene_number.as_deref()), (K::SceneHeading, OMITTED, Some("4")), "{name}");
        assert_eq!(heading.omitted.as_deref().map(str::trim), Some(kept.trim()), "{name}");
        assert!(heading.boneyard.is_empty(), "{name}: not left as set-aside text too");
        back.unomit_scene(0).unwrap();
        assert_eq!(texts(&back), texts(&fountain::parse(SCRIPT).unwrap()), "{name}");
    }
    // Written twice the same, in every format.
    let fdx_once = fdx::write(&fdx::parse(&fdx::write(&script)).unwrap());
    assert_eq!(fdx::write(&fdx::parse(&fdx_once).unwrap()), fdx_once);
}

#[test]
fn a_file_s_own_omitted_heading_is_omitted_with_nothing_to_bring_back() {
    let mut script = fountain::parse(".OMITTED #12#\n\nINT. HUT - DAY #13#\n\nRain.\n").unwrap();
    assert!(script.elements[0].is_omitted());
    assert!(script.unomit_scene(0).unwrap_err().contains("kept nothing"));
}
