#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

//! Revisions on the printed page: which lines carry a margin mark, and which set a page is
//! issued in (docs/production-spec.md §3.5).

use storyteller_core::layout::Layout;
use storyteller_core::model::{Deletion, Element, ElementKind, Revised, Script, ShowRevisions};
use storyteller_core::paginate::{Page, PageLine, paginate};
use storyteller_core::wrap;

/// White (1), Blue (2) and Pink (3), Pink active.
fn script(elements: Vec<Element>) -> Script {
    let mut script = Script { elements, ..Script::default() };
    for date in ["", "10/01/26", "10/08/26"] {
        script.add_revision_set(date);
    }
    script
}

fn revised(kind: ElementKind, text: &str, spans: &[(u32, u32, u32)]) -> Element {
    let mut element = Element::new(kind, text);
    element.revised = spans.iter().map(|&(from, to, set)| Revised { from, to, set }).collect();
    element
}

fn body(page: &Page) -> Vec<&PageLine> {
    page.lines.iter().filter(|line| line.element.is_some()).collect()
}

#[test]
fn characters_carry_their_set_and_deletions_the_one_before() {
    let deletions = [Deletion { at: 4, set: 3 }, Deletion { at: 0, set: 2 }];
    let chars = wrap::revised_chars("The **pump** stops.", true, &[Revised { from: 4, to: 8, set: 2 }], &deletions);
    let text: String = chars.iter().map(|sc| sc.c).collect();
    assert_eq!(text, "THE PUMP STOPS.");
    let sets: Vec<Option<u32>> = chars.iter().map(|sc| sc.revised).collect();
    assert_eq!(&sets[3..9], &[None, Some(2), Some(2), Some(2), Some(2), None]);
    // The deletion at 4 marks the space before it; the one at the start marks the first letter.
    assert_eq!(chars[3].deleted, Some(3));
    assert_eq!(chars[0].deleted, Some(2));
    // An empty span marks nothing; a later span wins where two overlap.
    let spans = [Revised { from: 1, to: 1, set: 1 }, Revised { from: 0, to: 3, set: 1 }, Revised { from: 1, to: 2, set: 2 }];
    let chars = wrap::revised_chars("abcd", false, &spans, &[]);
    assert_eq!(chars.iter().map(|sc| sc.revised).collect::<Vec<_>>(), [Some(1), Some(2), Some(1), None]);
    // Notes are not printed, so they take no place.
    let chars = wrap::revised_chars("A[[note]]B", false, &[Revised { from: 1, to: 2, set: 1 }], &[]);
    assert_eq!(chars.iter().map(|sc| (sc.c, sc.revised)).collect::<Vec<_>>(), [('A', None), ('B', Some(1))]);
}

#[test]
fn revised_lines_are_marked_and_runs_split_by_set() {
    let long = "Water rises through the floor of the pump house while the lamps flicker and the gauges spin. ".repeat(3);
    let from = u32::try_from(long.chars().count() - 20).unwrap();
    let script = script(vec![
        Element::new(ElementKind::SceneHeading, "INT. PUMP HOUSE - NIGHT"),
        revised(ElementKind::Action, &long, &[(from, from + 5, 2)]),
        revised(ElementKind::Action, "The pump stops dead.", &[(9, 14, 2), (14, 19, 3)]),
    ]);
    let pages = paginate(&script, &Layout::default());
    let lines = body(&pages.pages[0]);
    let marks: Vec<&[u32]> = lines.iter().map(|line| line.marks.as_slice()).collect();
    // The heading and every line of the long paragraph but its last are unmarked.
    assert!(marks[..marks.len() - 2].iter().all(|m| m.is_empty()), "{marks:?}");
    assert_eq!(marks[marks.len() - 2], [2]);
    assert_eq!(marks[marks.len() - 1], [2, 3]);
    let last = lines.last().unwrap();
    let runs: Vec<(&str, Option<u32>)> = last.runs.iter().map(|run| (run.text.as_str(), run.revised)).collect();
    assert_eq!(runs, [("The pump ", None), ("stops", Some(2)), (" dead", Some(3)), (".", None)]);
}

#[test]
fn a_deletion_marks_its_line() {
    let mut element = Element::new(ElementKind::Action, "Nobody moves.");
    element.deletions.push(Deletion { at: 6, set: 2 });
    let script = script(vec![element]);
    let pages = paginate(&script, &Layout::default());
    assert_eq!(body(&pages.pages[0])[0].marks, [2]);
}

#[test]
fn a_page_is_issued_in_its_latest_shown_set() {
    let script = script(vec![
        revised(ElementKind::Action, "Blue words.", &[(0, 4, 2)]),
        Element::new(ElementKind::PageBreak, ""),
        revised(ElementKind::Action, "Blue and pink.", &[(0, 4, 2), (9, 13, 3)]),
        Element::new(ElementKind::PageBreak, ""),
        Element::new(ElementKind::Action, "Untouched."),
    ]);
    let pages = paginate(&script, &Layout::default());
    let name = |page: usize, show| pages.pages[page].revision(&script.revisions, show).map(|set| set.name.clone());
    assert_eq!(name(0, ShowRevisions::All).as_deref(), Some("Blue Revision 10/01/26"));
    assert_eq!(name(1, ShowRevisions::All).as_deref(), Some("Pink Revision 10/08/26"));
    assert_eq!(name(2, ShowRevisions::All), None);
    // Showing the active set (Pink) only: the Blue page is not a revised page.
    assert_eq!(name(0, ShowRevisions::Active), None);
    assert_eq!(name(1, ShowRevisions::Active).as_deref(), Some("Pink Revision 10/08/26"));
    assert_eq!(name(1, ShowRevisions::None), None);
    // A mark from a set the script no longer has shows nothing.
    assert_eq!(script.revisions.latest(&[9], ShowRevisions::All), None);
}
