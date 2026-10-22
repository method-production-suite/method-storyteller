#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

//! Revision sets: the standard order of colours, adding, changing and deleting sets, and the
//! files keeping them. The script is invented.

use storyteller_core::model::{Revised, Script, revision_color};
use storyteller_core::{fdx, fountain, osf};

#[test]
fn sets_follow_the_standard_colours() {
    assert_eq!(revision_color(0), ("White".to_string(), None));
    assert_eq!(revision_color(1).0, "Blue");
    assert_eq!(revision_color(9).0, "Tan");
    assert_eq!(revision_color(10), ("Double White".to_string(), None));
    assert_eq!(revision_color(11).0, "Double Blue");
    let mut script = fountain::parse("INT. HUT - DAY\n\nRain.\n").unwrap();
    let white = script.add_revision_set("10/21/26");
    let blue = script.add_revision_set("10/22/26");
    let sets = &script.revisions.sets;
    assert_eq!((sets[0].name.as_str(), sets[1].name.as_str()), ("White Revision 10/21/26", "Blue Revision 10/22/26"));
    assert_eq!(sets[1].page_color.as_deref(), Some("#C9DAF8"));
    assert_eq!((sets[0].mark.as_str(), white, blue), ("*", 1, 2));
    assert_eq!(script.revisions.active, Some(blue), "the new set is the active one");
}

#[test]
fn a_set_changes_and_its_marks_move_when_it_goes() {
    let mut script = fountain::parse("INT. HUT - DAY\n\nRain falls.\n").unwrap();
    let white = script.add_revision_set("");
    let blue = script.add_revision_set("");
    script.elements[1].revised = vec![Revised { from: 0, to: 4, set: blue }, Revised { from: 5, to: 10, set: white }];
    script.update_revision_set(blue, Some("Blue Pages"), Some("+"), Some(Some("#aabbcc"))).unwrap();
    let set = script.revisions.set(blue).unwrap();
    assert_eq!((set.name.as_str(), set.mark.as_str(), set.page_color.as_deref()), ("Blue Pages", "+", Some("#AABBCC")));
    assert!(script.update_revision_set(blue, None, Some("***"), None).is_err(), "a mark is one or two characters");
    assert!(script.update_revision_set(blue, Some(" "), None, None).is_err(), "a set needs a name");
    assert_eq!(script.revision_marks(blue), 1);
    assert_eq!(script.delete_revision_set(blue).unwrap(), 1);
    assert!(script.elements[1].revised.iter().all(|r| r.set == white), "Blue's mark moves to White");
    assert_eq!(script.revisions.active, Some(white));
    assert_eq!(script.delete_revision_set(white).unwrap(), 2);
    assert!(script.elements[1].revised.is_empty(), "the first set's marks go");
    assert_eq!(script.revisions.active, None);
}

#[test]
fn new_sets_are_kept_by_fdx_and_osf() {
    let mut script: Script = fountain::parse("INT. HUT - DAY\n\nRain.\n").unwrap();
    script.add_revision_set("10/21/26");
    script.add_revision_set("10/22/26");
    for (name, back) in [("fdx", fdx::parse(&fdx::write(&script)).unwrap()), ("osf", osf::parse(&osf::write(&script)).unwrap())] {
        let names: Vec<_> = back.revisions.sets.iter().map(|s| (s.name.clone(), s.mark.clone(), s.page_color.clone())).collect();
        let expected: Vec<_> = script.revisions.sets.iter().map(|s| (s.name.clone(), s.mark.clone(), s.page_color.clone())).collect();
        assert_eq!(names, expected, "{name}");
        assert_eq!(back.revisions.active, script.revisions.active, "{name}");
    }
}

#[test]
fn edits_in_revision_mode_are_marked_and_deletions_leave_a_mark() {
    use storyteller_core::model::{Deletion, Element, ElementKind};
    let mut element = Element::new(ElementKind::Action, "Rain falls.");
    element.set_text_revised("Rain falls hard.", Some(2));
    assert_eq!(element.revised, [Revised { from: 10, to: 15, set: 2 }], "the inserted \" hard\"");
    element.set_text_revised("Rain hard.", Some(2));
    assert_eq!(element.deletions, [Deletion { at: 5, set: 2 }], "the taken-out \"falls \" leaves a mark");
    assert_eq!(element.revised, [Revised { from: 5, to: 9, set: 2 }], "the mark moves with its text (\"hard\")");
    // A later set's change inside an earlier one's span takes those characters over.
    element.set_text_revised("Rain very hard.", Some(3));
    let sets: Vec<(u32, u32, u32)> = element.revised.iter().map(|r| (r.from, r.to, r.set)).collect();
    assert_eq!(sets, [(5, 10, 3), (10, 14, 2)]);
    // Without a set (revision mode off) marks only move.
    element.set_text_revised("Cold rain very hard.", None);
    assert_eq!(element.revised.first().map(|r| r.from), Some(10));
    assert_eq!(element.revised.len(), 2, "nothing new is marked");
    // Clear Revised over a range takes its marks off, deletion marks included.
    element.mark_range(0, 30, None);
    assert!(element.revised.is_empty() && element.deletions.is_empty());
    element.mark_range(0, 4, Some(5));
    assert_eq!(element.revised, [Revised { from: 0, to: 4, set: 5 }]);
}

#[test]
fn deletion_marks_are_kept_by_fdx_and_osf() {
    use storyteller_core::model::Deletion;
    let mut script: Script = fountain::parse("INT. HUT - DAY\n\nRain falls.\n\nNORA\nIt held.\n").unwrap();
    script.add_revision_set("10/21/26");
    script.elements[1].deletions = vec![Deletion { at: 4, set: 1 }];
    script.elements[3].deletions = vec![Deletion { at: 0, set: 1 }, Deletion { at: 2, set: 1 }];
    let fdx_text = fdx::write(&script);
    assert!(fdx_text.contains("<DeletedText>") && fdx_text.contains("<DeletedTextLocation Offset="), "{fdx_text}");
    for (name, back) in [("fdx", fdx::parse(&fdx_text).unwrap()), ("osf", osf::parse(&osf::write(&script)).unwrap())] {
        let marks: Vec<_> = back.elements.iter().map(|e| e.deletions.clone()).collect();
        let expected: Vec<_> = script.elements.iter().map(|e| e.deletions.clone()).collect();
        assert_eq!(marks, expected, "{name}");
    }
    assert_eq!(fdx::write(&fdx::parse(&fdx_text).unwrap()), fdx_text, "unchanged, the list goes back as read");
}
