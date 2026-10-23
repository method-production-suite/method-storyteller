#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

//! Choosing the pages to issue: page ranges, scene ranges and revised pages only
//! (docs/production-spec.md §3.6).

use storyteller_core::layout::Layout;
use storyteller_core::model::{Element, ElementKind, Revised, Script, ShowRevisions};
use storyteller_core::paginate::{PageSelection, Paginated, paginate};

fn scene(number: &str, action: &str) -> Vec<Element> {
    let mut heading = Element::new(ElementKind::SceneHeading, &format!("INT. ROOM {number} - DAY"));
    heading.scene_number = Some(number.into());
    vec![heading, Element::new(ElementKind::Action, action)]
}

/// Five pages: scene 1 on page 1; scene 2 from page 2 to 3 (it runs on past a break); scene 2A
/// starts part way down page 3; scene 3 opens page 4; page 5 continues it. Page 4 is revised.
fn script() -> (Script, Paginated) {
    let mut elements = scene("1", "One.");
    elements.push(Element::new(ElementKind::PageBreak, ""));
    elements.extend(scene("2", "Two."));
    elements.push(Element::new(ElementKind::PageBreak, ""));
    elements.push(Element::new(ElementKind::Action, "Two goes on."));
    elements.extend(scene("2A", "Two A."));
    elements.push(Element::new(ElementKind::PageBreak, ""));
    let mut three = scene("3", "Three.");
    three[1].revised.push(Revised { from: 0, to: 5, set: 1 });
    elements.extend(three);
    elements.push(Element::new(ElementKind::PageBreak, ""));
    elements.push(Element::new(ElementKind::Action, "Three goes on."));
    let mut script = Script { elements, ..Script::default() };
    script.add_revision_set("");
    let pages = paginate(&script, &Layout::default());
    assert_eq!(pages.page_count(), 5);
    (script, pages)
}

fn select(pages: Option<&str>, scenes: Option<&str>, revised_only: bool) -> Result<Vec<u32>, String> {
    let (script, paginated) = script();
    let selection = PageSelection { pages: pages.map(str::to_string), scenes: scenes.map(str::to_string), revised_only };
    let chosen = paginated.select(&selection, &script.revisions, ShowRevisions::All)?;
    Ok(chosen.iter().map(|&i| paginated.pages[i].number).collect())
}

#[test]
fn page_ranges() {
    assert_eq!(select(None, None, false).unwrap(), [1, 2, 3, 4, 5]);
    assert_eq!(select(Some("2-3, 5"), None, false).unwrap(), [2, 3, 5]);
    assert_eq!(select(Some(" 4 "), None, false).unwrap(), [4]);
    assert_eq!(select(Some("1\u{2013}2"), None, false).unwrap(), [1, 2]);
    assert_eq!(select(Some("9"), None, false).unwrap_err(), "there is no page 9");
    assert_eq!(select(Some("4-2"), None, false).unwrap_err(), "the page range 4-2 runs backwards");
    assert_eq!(select(Some(","), None, false).unwrap_err(), "no pages were named");
}

#[test]
fn scene_ranges_take_every_page_the_scenes_print_on() {
    // Scene 2 runs onto page 3, where 2A starts part way down.
    assert_eq!(select(None, Some("2"), false).unwrap(), [2, 3]);
    assert_eq!(select(None, Some("2a"), false).unwrap(), [3]);
    // Scene 3 opens page 4, so 2A ends on page 3.
    assert_eq!(select(None, Some("2A-3"), false).unwrap(), [3, 4, 5]);
    assert_eq!(select(None, Some("1, 3"), false).unwrap(), [1, 4, 5]);
    assert_eq!(select(None, Some("4"), false).unwrap_err(), "there is no scene 4");
}

#[test]
fn revised_only_and_combinations_narrow() {
    assert_eq!(select(None, None, true).unwrap(), [4]);
    assert_eq!(select(Some("1-3"), None, true).unwrap(), Vec::<u32>::new());
    assert_eq!(select(Some("3-5"), Some("3"), false).unwrap(), [4, 5]);
    // Hidden revisions make no page a revised page.
    let (script, paginated) = script();
    let only = PageSelection { revised_only: true, ..PageSelection::default() };
    assert!(paginated.select(&only, &script.revisions, ShowRevisions::None).unwrap().is_empty());
    assert!(PageSelection::default().is_all() && !only.is_all());
    assert!(PageSelection { pages: Some("  ".into()), ..PageSelection::default() }.is_all());
}
