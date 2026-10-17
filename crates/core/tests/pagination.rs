#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

//! Golden pagination tests. Each `tests/golden/<name>.fountain` is paginated with the default
//! layout and compared with `<name>.pages.txt`. Run with `UPDATE_GOLDEN=1` to regenerate, then
//! review the diff.

use std::path::{Path, PathBuf};

use storyteller_core::fountain;
use storyteller_core::layout::Layout;
use storyteller_core::model::{ElementKind as K, Script};
use storyteller_core::paginate::{LineRole, Paginated, paginate, render_text};

fn golden_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden")
}

fn load(name: &str) -> Script {
    let text = std::fs::read_to_string(golden_dir().join(format!("{name}.fountain"))).unwrap();
    fountain::parse(&text).unwrap()
}

fn check_golden(name: &str) -> Paginated {
    let script = load(name);
    let paginated = paginate(&script, &Layout::default());
    let rendered = render_text(&paginated);
    let path = golden_dir().join(format!("{name}.pages.txt"));
    if std::env::var_os("UPDATE_GOLDEN").is_some() {
        std::fs::write(&path, &rendered).unwrap();
    } else {
        let expected = std::fs::read_to_string(&path).unwrap_or_else(|_| panic!("missing golden {path:?}; run with UPDATE_GOLDEN=1"));
        assert_eq!(rendered, expected, "golden mismatch for {name}");
    }
    check_rules(&script, &paginated);
    paginated
}

fn body_lines(paginated: &Paginated, page: usize) -> Vec<(LineRole, String)> {
    paginated.pages[page]
        .lines
        .iter()
        .filter(|line| !matches!(line.role, LineRole::PageNumber | LineRole::SceneNumber))
        .map(|line| (line.role, line.text()))
        .collect()
}

/// Words, split at hyphens too: a line may break after the hyphen in `thirty-five`.
fn words(text: &str) -> Vec<String> {
    text.split(|c: char| c.is_whitespace() || c == '-').filter(|w| !w.is_empty()).map(str::to_string).collect()
}

/// Invariants every pagination must satisfy.
fn check_rules(script: &Script, paginated: &Paginated) {
    let layout = Layout::default();
    for (index, page) in paginated.pages.iter().enumerate() {
        let body: Vec<_> = page.lines.iter().filter(|l| !matches!(l.role, LineRole::PageNumber | LineRole::SceneNumber)).collect();
        for line in &body {
            // `(MORE)` may hang one row into the bottom margin and a continued cue sits one row
            // above the body, as in Final Draft.
            let top = if line.role == LineRole::Contd { layout.body_top - 1 } else { layout.body_top };
            let end = layout.body_top + layout.lines_per_page + u16::from(line.role == LineRole::More);
            assert!(line.row >= top && line.row < end, "page {} line outside body: {line:?}", page.number);
            assert!(usize::from(line.col) + line.text().chars().count() <= 85, "line past the right edge: {line:?}");
        }
        let last = body.last().map(|l| l.role);
        if index + 1 < paginated.pages.len() {
            assert_ne!(last, Some(LineRole::Element(K::SceneHeading)), "page {} ends with a scene heading", page.number);
            assert_ne!(last, Some(LineRole::Element(K::Character)), "page {} ends with a character cue", page.number);
            assert_ne!(last, Some(LineRole::Element(K::Parenthetical)), "page {} ends with a parenthetical", page.number);
        }
        if last == Some(LineRole::More) {
            let next = paginated.pages.get(index + 1).and_then(|p| p.lines.iter().find(|l| l.role != LineRole::PageNumber));
            assert_eq!(next.map(|l| l.role), Some(LineRole::Contd), "(MORE) without (CONT'D) on the next page");
        }
        // Page numbers: none on page 1, `N.` on the rest.
        let numbers: Vec<_> = page.lines.iter().filter(|l| l.role == LineRole::PageNumber).map(|l| l.text()).collect();
        if page.number == 1 {
            assert!(numbers.is_empty());
        } else {
            assert_eq!(numbers, vec![format!("{}.", page.number)]);
        }
    }
    // No printed word is lost or duplicated.
    let mut printed = Vec::new();
    for page in &paginated.pages {
        for line in &page.lines {
            // A `NAME (CONT'D)` cue repeats the name, so it is not counted.
            if let LineRole::Element(_) = line.role {
                printed.extend(words(&line.text()));
            }
        }
    }
    let mut expected = Vec::new();
    for element in &script.elements {
        if element.kind.is_printed() {
            let text = storyteller_core::inline::plain(&element.text);
            expected.extend(words(&if element.kind.is_caps() { text.to_uppercase() } else { text }));
        }
    }
    // Dual dialogue prints both columns row by row, so compare as multisets.
    printed.sort();
    expected.sort();
    assert_eq!(printed, expected, "printed words differ from the script");
}

#[test]
fn scene_heading_keeps_with_next() {
    let paginated = check_golden("scene_heading_keep");
    assert_eq!(paginated.pages.len(), 2);
    assert_eq!(body_lines(&paginated, 1)[0], (LineRole::Element(K::SceneHeading), "INT. NEW ROOM - DAY".into()));
}

#[test]
fn dialogue_splits_with_more_and_contd() {
    let paginated = check_golden("dialogue_split");
    let first = body_lines(&paginated, 0);
    assert_eq!(first.last().unwrap(), &(LineRole::More, "(MORE)".into()));
    let second = body_lines(&paginated, 1);
    assert_eq!(second[0], (LineRole::Contd, "OWEN (CONT'D)".into()));
    // The split falls between sentences.
    let before_more = &first[first.len() - 2].1;
    assert!(before_more.ends_with('.') || before_more.ends_with('?') || before_more.ends_with('!'), "{before_more}");
}

#[test]
fn short_dialogue_moves_whole() {
    let paginated = check_golden("dialogue_moves");
    assert!(body_lines(&paginated, 0).iter().all(|(role, _)| *role == LineRole::Element(K::Action)));
    assert_eq!(body_lines(&paginated, 1)[0], (LineRole::Element(K::Character), "MARA".into()));
}

/// Final Draft lets a parenthetical be the last line before `(MORE)` (measured from its PDFs).
#[test]
fn parenthetical_may_end_the_top_of_a_split() {
    let paginated = check_golden("parenthetical_not_last");
    let first = body_lines(&paginated, 0);
    assert_eq!(first.last().unwrap().0, LineRole::More);
    assert_eq!(first[first.len() - 2], (LineRole::Element(K::Parenthetical), "(louder)".into()));
    assert_eq!(body_lines(&paginated, 1)[0].0, LineRole::Contd);
}

#[test]
fn action_splits_between_sentences() {
    let paginated = check_golden("action_split");
    let first = body_lines(&paginated, 0);
    let last = &first.last().unwrap().1;
    assert!(last.ends_with('.'), "page one ends mid-sentence: {last}");
    assert!(body_lines(&paginated, 1)[0].1.starts_with(char::is_uppercase));
}

#[test]
fn transition_never_opens_a_page() {
    let paginated = check_golden("transition_not_first");
    let second = body_lines(&paginated, 1);
    assert_eq!(second[0], (LineRole::Element(K::Action), "Short action two.".into()));
    assert_eq!(second[1].0, LineRole::Element(K::Transition));
}

#[test]
fn forced_page_break() {
    let paginated = check_golden("forced_break");
    assert_eq!(paginated.pages.len(), 2);
}

#[test]
fn dual_dialogue_side_by_side() {
    let paginated = check_golden("dual_dialogue");
    let page = &paginated.pages[0];
    let mara = page.lines.iter().find(|l| l.text() == "MARA").unwrap();
    let owen = page.lines.iter().find(|l| l.text() == "OWEN").unwrap();
    assert_eq!(mara.row, owen.row);
    assert!(owen.col > mara.col + 20);
}

#[test]
fn dual_dialogue_moves_whole_to_next_page() {
    let paginated = check_golden("dual_dialogue_moves");
    assert_eq!(paginated.pages.len(), 2);

    let first = body_lines(&paginated, 0);
    assert!(first.len() > 50, "the action should fill the first page");
    assert!(first.iter().all(|(role, _)| *role == LineRole::Element(K::Action)));

    let second = &paginated.pages[1];
    let cues: Vec<_> = second.lines.iter().filter(|line| line.role == LineRole::Element(K::Character)).collect();
    assert_eq!(cues.len(), 2, "both dual-dialogue cues should start on page two");
    assert_eq!(cues[0].text(), "MARA");
    assert_eq!(cues[1].text(), "OWEN");
    assert_eq!(cues[0].row, cues[1].row, "dual-dialogue cues should start on the same row");
}

#[test]
fn scene_numbers_and_title_page() {
    let paginated = check_golden("scene_numbers");
    assert!(paginated.title_page.is_some());
    let numbers: Vec<_> = paginated.pages[0].lines.iter().filter(|l| l.role == LineRole::SceneNumber).map(|l| l.text()).collect();
    assert_eq!(numbers, ["1", "1", "2A", "2A"]);
    assert_eq!(paginated.scenes.iter().map(|s| s.page).collect::<Vec<_>>(), [1, 1]);
}

#[test]
fn fixture_golden() {
    let text = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/lighthouse.fountain")).unwrap();
    let script = fountain::parse(&text).unwrap();
    let paginated = paginate(&script, &Layout::default());
    let path = golden_dir().join("lighthouse.pages.txt");
    let rendered = render_text(&paginated);
    if std::env::var_os("UPDATE_GOLDEN").is_some() {
        std::fs::write(&path, &rendered).unwrap();
    } else {
        assert_eq!(rendered, std::fs::read_to_string(&path).unwrap());
    }
    check_rules(&script, &paginated);
}

/// A long generated script obeys every rule on every page.
#[test]
fn generated_scripts_obey_the_rules() {
    let mut seed: u64 = 0x5eed;
    let mut next = move |n: u64| {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed % n
    };
    let sentences = ["She waits.", "The tide turns again and again.", "Nobody answers the bell?", "It is late -- far too late.", "Gulls."];
    for round in 0..20 {
        let mut fountain_text = String::new();
        for scene in 0..40 {
            fountain_text.push_str(&format!("INT. ROOM {round}-{scene} - DAY\n\n"));
            for _ in 0..next(6) + 1 {
                if next(2) == 0 {
                    let count = next(12) + 1;
                    let text: Vec<&str> = (0..count).map(|_| sentences[next(5) as usize]).collect();
                    fountain_text.push_str(&format!("{}\n\n", text.join(" ")));
                } else {
                    fountain_text.push_str(&format!("{}\n", ["ANNA", "BEN (O.S.)", "CAPTAIN"][next(3) as usize]));
                    for part in 0..next(4) + 1 {
                        if part > 0 && next(3) == 0 {
                            fountain_text.push_str("(beat)\n");
                        }
                        let count = next(10) + 1;
                        let text: Vec<&str> = (0..count).map(|_| sentences[next(5) as usize]).collect();
                        fountain_text.push_str(&format!("{}\n", text.join(" ")));
                    }
                    fountain_text.push('\n');
                }
            }
            if next(4) == 0 {
                fountain_text.push_str("CUT TO:\n\n");
            }
        }
        let script = fountain::parse(&fountain_text).unwrap();
        let paginated = paginate(&script, &Layout::default());
        assert!(paginated.pages.len() > 5);
        check_rules(&script, &paginated);
    }
}
