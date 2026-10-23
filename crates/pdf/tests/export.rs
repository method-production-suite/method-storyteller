#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

use storyteller_core::fountain;
use storyteller_core::layout::Layout;
use storyteller_core::model::{Element, ElementKind, Revised, Script, ShowRevisions};
use storyteller_core::paginate::PageSelection;
use storyteller_pdf::{PdfOptions, export};

fn fixture() -> Script {
    let text = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../core/tests/fixtures/lighthouse.fountain")).unwrap();
    fountain::parse(&text).unwrap()
}

fn count(haystack: &[u8], needle: &[u8]) -> usize {
    haystack.windows(needle.len()).filter(|window| *window == needle).count()
}

#[test]
fn exports_a_valid_pdf() {
    let bytes = export(&fixture(), &Layout::default(), &PdfOptions { creator: "Method Storyteller test".into(), ..PdfOptions::default() }).unwrap();
    assert!(bytes.starts_with(b"%PDF-"));
    assert!(bytes.trim_ascii_end().ends_with(b"%%EOF"));
    assert!(count(&bytes, b"/Outlines") >= 1, "scene bookmarks");
    assert!(count(&bytes, b"CourierPrime") >= 1, "font embedded");
}

#[test]
fn empty_and_odd_scripts_export() {
    for script in [
        Script::default(),
        Script { elements: vec![Element::new(ElementKind::Action, "日本語 and emoji 🎬 and **bold _under_**")], ..Script::default() },
    ] {
        let bytes = export(&script, &Layout::default(), &PdfOptions::default()).unwrap();
        assert!(bytes.starts_with(b"%PDF-"));
    }
}

/// Optional check with poppler's `pdftotext` when it is installed.
#[test]
fn text_is_extractable_when_pdftotext_is_available() {
    let Ok(output) = std::process::Command::new("pdftotext").arg("-v").output() else { return };
    if !output.status.success() && output.stderr.is_empty() {
        return;
    }
    let dir = std::env::temp_dir().join(format!("storyteller-pdf-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("lighthouse.pdf");
    std::fs::write(&path, export(&fixture(), &Layout::default(), &PdfOptions::default()).unwrap()).unwrap();
    let text = std::process::Command::new("pdftotext").arg("-layout").arg(&path).arg("-").output().unwrap();
    let text = String::from_utf8_lossy(&text.stdout);
    assert!(text.contains("EXT. LIGHTHOUSE - NIGHT"), "{text}");
    assert!(text.contains("OWEN (CONT'D)"), "{text}");
    if let Ok(info) = std::process::Command::new("pdfinfo").arg(&path).output() {
        let info = String::from_utf8_lossy(&info.stdout);
        // Title page plus two script pages.
        assert!(info.lines().any(|l| l.starts_with("Pages:") && l.trim_end().ends_with(" 3")), "{info}");
    }
    let _ = std::fs::remove_dir_all(dir);
}

fn revised_script() -> Script {
    let mut changed = Element::new(ElementKind::Action, "The pump stops dead.");
    changed.revised.push(Revised { from: 9, to: 14, set: 2 });
    let mut script = Script { elements: vec![changed, Element::new(ElementKind::Action, "Nobody moves.")], ..Script::default() };
    script.add_revision_set("");
    script.add_revision_set("10/01/26");
    script
}

#[test]
fn revised_pages_export_in_every_choice() {
    for (revisions, coloured_pages) in [(ShowRevisions::All, true), (ShowRevisions::Active, false), (ShowRevisions::None, true)] {
        let bytes = export(&revised_script(), &Layout::default(), &PdfOptions { revisions, coloured_pages, ..PdfOptions::default() }).unwrap();
        assert!(bytes.starts_with(b"%PDF-"));
    }
}

/// The mark at 7.75" and the set's name under the page number, read back with `pdftotext`
/// when it is installed.
#[test]
fn revision_marks_and_label_print_when_pdftotext_is_available() {
    let Ok(output) = std::process::Command::new("pdftotext").arg("-v").output() else { return };
    if !output.status.success() && output.stderr.is_empty() {
        return;
    }
    let dir = std::env::temp_dir().join(format!("storyteller-pdf-rev-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let text = |options: &PdfOptions| {
        let path = dir.join("revised.pdf");
        std::fs::write(&path, export(&revised_script(), &Layout::default(), options).unwrap()).unwrap();
        let out = std::process::Command::new("pdftotext").arg("-layout").arg(&path).arg("-").output().unwrap();
        String::from_utf8_lossy(&out.stdout).into_owned()
    };
    let shown = text(&PdfOptions::default());
    assert!(shown.contains("Blue Revision 10/01/26"), "{shown}");
    let marked: Vec<&str> = shown.lines().filter(|l| l.trim_end().ends_with('*')).collect();
    assert_eq!(marked.len(), 1, "{shown}");
    assert!(marked[0].contains("The pump stops dead."), "{shown}");
    let hidden = text(&PdfOptions { revisions: ShowRevisions::None, ..PdfOptions::default() });
    assert!(!hidden.contains("Revision") && !hidden.contains('*'), "{hidden}");
    let _ = std::fs::remove_dir_all(dir);
}

/// Revised pages only, a page range and a clean copy, read back with poppler's tools when
/// they are installed.
#[test]
fn issued_pages_and_clean_copies_when_pdftotext_is_available() {
    let Ok(output) = std::process::Command::new("pdftotext").arg("-v").output() else { return };
    if !output.status.success() && output.stderr.is_empty() {
        return;
    }
    let mut script = revised_script();
    script.elements.insert(0, Element::new(ElementKind::PageBreak, ""));
    script.elements.insert(0, Element::new(ElementKind::Action, "The first page, untouched."));
    script.title_page.set("Title", "Pump House");
    let dir = std::env::temp_dir().join(format!("storyteller-pdf-issue-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let text = |options: &PdfOptions| {
        let path = dir.join("issued.pdf");
        std::fs::write(&path, export(&script, &Layout::default(), options).unwrap()).unwrap();
        let out = std::process::Command::new("pdftotext").arg("-layout").arg(&path).arg("-").output().unwrap();
        String::from_utf8_lossy(&out.stdout).into_owned()
    };
    let pages = |text: &str| text.matches('\u{c}').count();
    let all = text(&PdfOptions::default());
    assert_eq!(pages(&all), 3, "title page and two pages: {all}");
    let revised = text(&PdfOptions { selection: PageSelection { revised_only: true, ..PageSelection::default() }, ..PdfOptions::default() });
    assert_eq!(pages(&revised), 1, "{revised}");
    assert!(revised.contains("The pump stops dead.") && revised.contains("Blue Revision 10/01/26"), "{revised}");
    let first = text(&PdfOptions { selection: PageSelection { pages: Some("1".into()), ..PageSelection::default() }, ..PdfOptions::default() });
    assert!(first.contains("The first page") && !first.contains("Pump House") && pages(&first) == 1, "{first}");
    let clean = text(&PdfOptions { clean: true, coloured_pages: true, ..PdfOptions::default() });
    assert_eq!(pages(&clean), 3);
    assert!(clean.contains("The pump stops dead.") && !clean.contains("Revision") && !clean.contains('*'), "{clean}");
    // Nothing matches: an error, not an empty file.
    let none =
        PdfOptions { selection: PageSelection { pages: Some("1".into()), revised_only: true, ..PageSelection::default() }, ..PdfOptions::default() };
    assert!(export(&script, &Layout::default(), &none).is_err());
    let _ = std::fs::remove_dir_all(dir);
}
