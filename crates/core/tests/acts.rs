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
