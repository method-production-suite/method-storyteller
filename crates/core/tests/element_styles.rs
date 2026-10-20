#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

//! Element settings the page honours: double spacing, a new page before each element of a type,
//! and capitals, as a multi-camera script uses them. The scripts are invented.

use storyteller_core::layout::Layout;
use storyteller_core::paginate::{LineRole, Paginated, paginate};
use storyteller_core::{ElementKind as K, fdx, fountain};

const SCENES: &str = "\
INT. FIELD STATION - NIGHT

Nora checks the radio and frowns at the static.

NORA
The signal is gone again. It was clear an hour ago, and now there is nothing at all on any band.

EXT. RIDGE - DAWN

Wind.
";

/// (page, row, text) of each line of `kind`.
fn lines(pages: &Paginated, kind: K) -> Vec<(u32, u16, String)> {
    pages.pages.iter().flat_map(|p| p.lines.iter().filter(|l| l.role == LineRole::Element(kind)).map(|l| (p.number, l.row, l.text()))).collect()
}

#[test]
fn double_spaced_dialogue_takes_a_row_between_its_lines() {
    let script = fountain::parse(SCENES).unwrap();
    let single = paginate(&script, &Layout::default());
    let mut layout = Layout::default();
    layout.dialogue.spacing = 2;
    let double = paginate(&script, &layout);
    let rows = |p: &Paginated| lines(p, K::Dialogue).iter().map(|(_, row, _)| *row).collect::<Vec<_>>();
    let (one, two) = (rows(&single), rows(&double));
    assert!(one.len() >= 3, "the speech wraps: {one:?}");
    assert_eq!(one.len(), two.len(), "the same lines");
    assert!(one.windows(2).all(|w| w[1] == w[0] + 1), "single: {one:?}");
    assert!(two.windows(2).all(|w| w[1] == w[0] + 2), "double: {two:?}");
    // What follows moves down by the extra rows.
    let heading_row = |p: &Paginated| lines(p, K::SceneHeading)[1].1;
    assert_eq!(heading_row(&double), heading_row(&single) + u16::try_from(one.len() - 1).unwrap());
}

#[test]
fn a_type_set_to_start_a_new_page_does() {
    let script = fountain::parse(SCENES).unwrap();
    assert_eq!(paginate(&script, &Layout::default()).page_count(), 1);
    let mut layout = Layout::default();
    layout.scene_heading.new_page = true;
    let pages = paginate(&script, &layout);
    let headings = lines(&pages, K::SceneHeading);
    assert_eq!(headings.iter().map(|(page, _, _)| *page).collect::<Vec<_>>(), [1, 2], "the first scene adds no blank page");
    assert_eq!(headings[1].1, layout.body_top, "at the top of its page");
}

#[test]
fn a_type_in_capitals_prints_in_capitals() {
    let script = fountain::parse(SCENES).unwrap();
    let mut layout = Layout::default();
    layout.action.caps = true;
    let action = lines(&paginate(&script, &layout), K::Action);
    assert!(action.iter().any(|(_, _, t)| t.starts_with("NORA CHECKS THE RADIO")), "{action:?}");
    assert!(lines(&paginate(&script, &Layout::default()), K::Action).iter().any(|(_, _, t)| t.starts_with("Nora checks")));
}

#[test]
fn an_fdx_file_brings_its_spacing_new_pages_and_capitals() {
    let settings = |kind: &str, style: &str, paragraph: &str| {
        format!(
            r#"<ElementSettings Type="{kind}"><FontSpec Font="Courier Prime" Size="12" Style="{style}"/><ParagraphSpec {paragraph}/></ElementSettings>"#
        )
    };
    let xml = format!(
        r#"<?xml version="1.0"?><FinalDraft DocumentType="Script" Template="No" Version="5"><Content><Paragraph Type="Action"><Text>A.</Text></Paragraph></Content>{}{}{}</FinalDraft>"#,
        settings(
            "Scene Heading",
            "AllCaps",
            r#"Alignment="Left" LeftIndent="1.50" RightIndent="7.50" SpaceBefore="24" Spacing="1" StartsNewPage="Yes""#
        ),
        settings("Action", "AllCaps", r#"Alignment="Left" LeftIndent="1.50" RightIndent="7.50" SpaceBefore="12" Spacing="1""#),
        settings("Dialogue", "", r#"Alignment="Left" LeftIndent="2.50" RightIndent="6.00" SpaceBefore="0" Spacing="2""#),
    );
    let layout = Layout::for_script(&fdx::parse(&xml).unwrap());
    assert!(layout.scene_heading.new_page && layout.scene_heading.caps);
    assert!(layout.action.caps && !layout.action.new_page);
    assert_eq!(layout.dialogue.spacing, 2);
    assert_eq!(Layout::default().dialogue.spacing, 1);
}
