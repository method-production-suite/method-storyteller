#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

//! Pagination keeps locked pages: A-pages for what no longer fits, short pages, pages printed
//! together as `7-8`, and pages that start inside a paragraph (docs/production-spec.md §4.1).
//! The script is invented.

use storyteller_core::layout::Layout;
use storyteller_core::model::{Element, ElementKind, PageStart, Script};
use storyteller_core::paginate::{LineRole, Paginated, paginate};

fn action(text: &str) -> Element {
    Element::new(ElementKind::Action, text)
}

/// `n` one-line action paragraphs: two rows each, with the blank line before.
fn lines(n: usize, word: &str) -> Vec<Element> {
    (1..=n).map(|i| action(&format!("{word} {i}."))).collect()
}

fn lock(element: &mut Element, at: u32, label: &str) {
    element.page_starts.push(PageStart { at, label: label.into(), fdx_attributes: Vec::new() });
}

fn script(elements: Vec<Element>) -> Script {
    Script { elements, ..Script::default() }
}

fn labels(paginated: &Paginated) -> Vec<String> {
    paginated.pages.iter().map(|p| p.label()).collect()
}

/// The first line of body text on page `index`.
fn first_text(paginated: &Paginated, index: usize) -> String {
    paginated.pages[index].lines.iter().find(|l| matches!(l.role, LineRole::Element(_) | LineRole::Contd)).map(|l| l.text()).unwrap_or_default()
}

fn printed_number(paginated: &Paginated, index: usize) -> Option<String> {
    paginated.pages[index].lines.iter().find(|l| l.role == LineRole::PageNumber).map(|l| l.text())
}

#[test]
fn a_page_that_grew_flows_onto_a_pages() {
    // Page 1 now holds far more than a page; page 2 starts where it was locked.
    let mut elements = lines(60, "Rain");
    let mut rest = lines(3, "Dry");
    lock(&mut elements[0], 0, "1");
    lock(&mut rest[0], 0, "2");
    elements.extend(rest);
    let paginated = paginate(&script(elements), &Layout::default());
    assert_eq!(labels(&paginated), ["1", "1A", "1B", "2"]);
    assert_eq!(printed_number(&paginated, 0), None, "page 1 prints no number");
    assert_eq!(printed_number(&paginated, 1).as_deref(), Some("1A."));
    assert_eq!(first_text(&paginated, 3), "Dry 1.");
}

#[test]
fn a_page_whose_text_went_stays_short() {
    // Page 2 lost most of its text; page 3 is full, so nothing moves up.
    let mut elements = lines(27, "One");
    let mut two = lines(2, "Two");
    let mut three = lines(27, "Three");
    lock(&mut elements[0], 0, "1");
    lock(&mut two[0], 0, "2");
    lock(&mut three[0], 0, "3");
    elements.extend(two);
    elements.extend(three);
    let paginated = paginate(&script(elements), &Layout::default());
    assert_eq!(labels(&paginated), ["1", "2", "3"]);
    assert_eq!(first_text(&paginated, 2), "Three 1.");
    assert!(paginated.pages[1].lines.iter().filter(|l| matches!(l.role, LineRole::Element(_))).count() == 2);
}

#[test]
fn pages_whose_text_fits_on_one_print_as_one() {
    let mut elements = lines(27, "One");
    let mut two = lines(2, "Two");
    let mut three = lines(2, "Three");
    let mut four = vec![Element::new(ElementKind::PageBreak, ""), action("Four.")];
    lock(&mut elements[0], 0, "1");
    lock(&mut two[0], 0, "2");
    lock(&mut three[0], 0, "3");
    lock(&mut four[1], 0, "4");
    elements.extend(two);
    elements.extend(three);
    elements.extend(four);
    let paginated = paginate(&script(elements), &Layout::default());
    // A page break keeps page 4 apart.
    assert_eq!(labels(&paginated), ["1", "2-3", "4"]);
    assert_eq!(printed_number(&paginated, 1).as_deref(), Some("2-3."));
}

#[test]
fn a_page_with_nothing_left_prints_with_the_one_before() {
    let mut elements = lines(3, "One");
    lock(&mut elements[0], 0, "1");
    // Page 2's text was all deleted; its lock sits on the next page's first paragraph.
    let mut three = lines(2, "Three");
    lock(&mut three[0], 0, "2");
    lock(&mut three[0], 0, "3");
    elements.extend(three);
    let paginated = paginate(&script(elements), &Layout::default());
    assert_eq!(labels(&paginated), ["1-3"]);
}

#[test]
fn a_page_starts_inside_a_paragraph() {
    let sentences: Vec<String> = (1..=90).map(|i| format!("Sentence number {i} runs on a while.")).collect();
    let text = sentences.join(" ");
    let at = sentences[..60].join(" ").chars().count() + 1;
    let mut paragraph = action(&text);
    lock(&mut paragraph, 0, "1");
    lock(&mut paragraph, u32::try_from(at).unwrap(), "2");
    let paginated = paginate(&script(vec![paragraph]), &Layout::default());
    assert_eq!(labels(&paginated), ["1", "2"]);
    assert!(first_text(&paginated, 1).starts_with("Sentence number 61 "), "{}", first_text(&paginated, 1));
}

#[test]
fn a_page_inside_a_speech_continues_it() {
    let mut elements = lines(25, "Wait");
    lock(&mut elements[0], 0, "1");
    elements.push(Element::new(ElementKind::Character, "MARA"));
    elements.push(Element::new(ElementKind::Dialogue, "First we listen. Then we go."));
    let at = u32::try_from("First we listen. ".chars().count()).unwrap();
    lock(elements.last_mut().unwrap(), at, "2");
    elements.extend(lines(26, "Gone"));
    let paginated = paginate(&script(elements), &Layout::default());
    assert_eq!(labels(&paginated), ["1", "2"]);
    let page_one: Vec<String> = paginated.pages[0].lines.iter().map(|l| l.text()).collect();
    assert!(page_one.iter().any(|t| t == "(MORE)"), "{page_one:?}");
    assert_eq!(first_text(&paginated, 1), "MARA (CONT'D)");
    assert!(paginated.pages[1].lines.iter().any(|l| l.text() == "Then we go."));
}

#[test]
fn unlocked_scripts_number_pages_as_before() {
    let paginated = paginate(&script(lines(60, "Rain")), &Layout::default());
    assert_eq!(labels(&paginated), ["1", "2", "3"]);
    assert!(paginated.pages.iter().all(|p| p.locked.is_none()));
}

#[test]
fn locks_read_from_fdx_paginate_unlocked_for_now() {
    // Kept and written back, but where Final Draft's `Position` starts a page inside a paragraph
    // is not read reliably yet (docs/research/locked-pages.md).
    let mut elements = lines(60, "Rain");
    lock(&mut elements[0], 0, "1");
    elements[0].page_starts[0].fdx_attributes = vec![("PageNumber".into(), "1".into())];
    let locked = script(elements);
    assert_eq!(labels(&paginate(&locked, &Layout::default())), ["1", "2", "3"]);
    let kept = storyteller_core::paginate::paginate_with(&locked, &Layout::default(), true);
    assert_eq!(labels(&kept), ["1", "1A", "1B"]);
}
