#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

//! The fidelity report, docs/research/fdx-osf-fidelity.md: its table is this file's [`ROWS`], and
//! every row marked exact is checked here on an invented script, `.fdx` to Open Screenplay
//! Format to `.fdx` and OSF to `.fdx` to OSF.
//!
//! To change the report, change [`ROWS`] and run the test with `STORYTELLER_WRITE_CHECK_FILES=1`,
//! which also rewrites the owner's check files in `tests/fixtures/`.

use storyteller_core::layout::Layout;
use storyteller_core::model::Script;
use storyteller_core::{fdx, osf};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Fidelity {
    Exact,
    Approximated,
    NotMapped,
}

struct Row {
    what: &'static str,
    fidelity: Fidelity,
    how: &'static str,
    /// What the script holds of this row, compared before and after a round trip (exact rows).
    check: Option<fn(&Script) -> String>,
}

fn elements(s: &Script) -> String {
    s.elements
        .iter()
        .map(|e| {
            format!("{:?} {:?} {:?} {} {}\n", e.kind, e.fdx.as_ref().map(|f| &f.type_name).filter(|t| *t != e.kind.label()), e.text, e.level, e.dual)
        })
        .collect()
}

fn scene_numbers(s: &Script) -> String {
    format!("{:?}", s.elements.iter().map(|e| e.scene_number.clone()).collect::<Vec<_>>())
}

fn title_page(s: &Script) -> String {
    format!("{:?}", s.title_page)
}

fn notes(s: &Script) -> String {
    let mut out = String::new();
    for (i, e) in s.elements.iter().enumerate() {
        for n in &e.notes {
            out.push_str(&format!("{i} {:?} {:?} {:?} {:?} {}\n", n.text, n.range, n.color, n.author, n.unanchored));
        }
    }
    out
}

fn revision_sets(s: &Script) -> String {
    let sets: Vec<_> = s.revisions.sets.iter().map(|r| (r.id, &r.name, &r.mark, &r.color, &r.page_color)).collect();
    format!("{:?} {sets:?}", s.revisions.active)
}

fn revised(s: &Script) -> String {
    format!("{:?}", s.elements.iter().map(|e| &e.revised).collect::<Vec<_>>())
}

fn deletions(s: &Script) -> String {
    format!("{:?}", s.elements.iter().map(|e| &e.deletions).collect::<Vec<_>>())
}

fn locked_pages(s: &Script) -> String {
    format!("{:?}", s.elements.iter().map(|e| e.page_starts.iter().map(|p| (p.at, &p.label)).collect::<Vec<_>>()).collect::<Vec<_>>())
}

fn element_settings(s: &Script) -> String {
    s.format
        .elements
        .iter()
        .map(|e| {
            format!(
                "{} {:?} {:?} {:?} {} {} {} {} {} {:?} {:?} {:?} {:?} {:?} {:?} {} {:?} {:?}\n",
                e.name,
                e.based_on,
                e.font,
                e.size,
                e.bold,
                e.italic,
                e.underline,
                e.all_caps,
                e.hidden,
                e.align,
                e.left,
                e.right,
                e.first,
                e.space_before,
                e.spacing,
                e.new_page,
                e.enter,
                e.tab
            )
        })
        .collect()
}

fn page_layout(s: &Script) -> String {
    let page = s.format.page.clone().unwrap_or_default();
    format!("{:?} {:?} {:?} {:?} {:?} {:?} {:?}", page.width, page.height, page.top, page.bottom, page.more, page.contd, page.auto_contd)
}

fn spelling_ignored(s: &Script) -> String {
    format!("{:?}", s.spelling_ignored)
}

fn pagination(s: &Script) -> String {
    format!("{:?}", Layout::for_script(s))
}

use Fidelity::{Approximated, Exact, NotMapped};

const ROWS: &[Row] = &[
    Row {
        what: "Element types, custom types, outline and summary paragraphs",
        fidelity: Exact,
        how: "paragraph `Type` and OSF style; types OSF lacks carry `method_storyteller kind`",
        check: Some(elements),
    },
    Row { what: "Bold, italic, underline", fidelity: Exact, how: "`Text@Style` and `text@bold/italic/underline`", check: Some(elements) },
    Row {
        what: "Scene numbers, A-numbers included",
        fidelity: Exact,
        how: "`Paragraph@Number` and `para@scene_number` (`number` in 4.0)",
        check: Some(scene_numbers),
    },
    Row { what: "Dual dialogue", fidelity: Exact, how: "`DualDialogue` and `dualdialogue` on the first cue", check: Some(elements) },
    Row { what: "Page breaks", fidelity: Exact, how: "`StartsNewPage` and `pagebreakbefore`", check: Some(elements) },
    Row { what: "Title page fields", fidelity: Exact, how: "`TitlePage` paragraphs and bookmarked `titlepage` paragraphs", check: Some(title_page) },
    Row {
        what: "Script notes: text, range, colour, author, all three `.fdx` forms",
        fidelity: Exact,
        how: "OSF `note` (plain) plus `method_storyteller_note`; ranges count printed characters, one per paragraph break",
        check: Some(notes),
    },
    Row {
        what: "Revision sets: name, mark, mark colour, page colour, active set",
        fidelity: Exact,
        how: "`Revisions` and `revision_colors`, `settings@revision`; colours OSF has no place for ride on `method_storyteller_` attributes",
        check: Some(revision_sets),
    },
    Row { what: "Revised text", fidelity: Exact, how: "`Text@RevisionID` and `text@revision`", check: Some(revised) },
    Row {
        what: "Deleted-text marks",
        fidelity: Exact,
        how: "`DeletedText` (`Offset` counts like note ranges) and our `method_storyteller_deletion`; OSF's own `marks` are kept as read",
        check: Some(deletions),
    },
    Row {
        what: "Locked pages and A-pages",
        fidelity: Exact,
        how: "`LockedPages` (`Position` counts like note ranges) and `pages_locked` with `para@page_number`; a page starting inside a paragraph carries `method_storyteller_page`",
        check: Some(locked_pages),
    },
    Row {
        what: "Element settings: font, size, styles, alignment, indents, space before, line spacing, page break, Enter and Tab",
        fidelity: Exact,
        how: "`ElementSettings` and `styles`, in thousandths of an inch",
        check: Some(element_settings),
    },
    Row {
        what: "Page size, top and bottom margins, (MORE), (CONT'D), automatic continueds",
        fidelity: Exact,
        how: "`PageLayout`, `MoresAndContinueds` and `settings`",
        check: Some(page_layout),
    },
    Row {
        what: "Words spell check ignores",
        fidelity: Exact,
        how: "`SpellCheckIgnoreLists/IgnoredWords` and `<spelling><ignored>`, as written; OSF's user dictionary stays in the file",
        check: Some(spelling_ignored),
    },
    Row { what: "Pagination", fidelity: Exact, how: "both formats paginate from the same model", check: Some(pagination) },
    Row {
        what: "OSF revision colour names",
        fidelity: Approximated,
        how: "OSF has a fixed palette without RGB: the name comes from the set's name, else the nearest page colour; the palette's RGB values are our choice",
        check: None,
    },
    Row {
        what: "`.fdx` lengths written to OSF",
        fidelity: Approximated,
        how: "OSF counts tenths of a millimetre, coarser than `.fdx` hundredths of an inch; our own files bring the `.fdx` value back, and other apps get the nearest tenth",
        check: None,
    },
    Row {
        what: "Lengths from OSF written to `.fdx`",
        fidelity: Approximated,
        how: "can need up to six decimals of an inch (`7.248031`) to come back exact; to confirm in the final check",
        check: None,
    },
    Row {
        what: "Note body formatting",
        fidelity: Approximated,
        how: "fonts and indents on a note's own text survive `.fdx` to `.fdx`; through OSF the text is rewritten plain",
        check: None,
    },
    Row {
        what: "Tab before and after",
        fidelity: Approximated,
        how: "OSF has both, `.fdx` one: `style_tab_after` is `TabKey`, `style_tab_before` rides along",
        check: None,
    },
    Row {
        what: "Omitted scenes",
        fidelity: Approximated,
        how: "the heading, its number and the scene it keeps go both ways (`StorytellerOmitted`, `method_storyteller_omitted`); a file's own `OmittedScene` wrapper stays in `.fdx`",
        check: None,
    },
    Row {
        what: "OSF page margins, keep-with-next, header and footer styles",
        fidelity: NotMapped,
        how: "`.fdx` has no place: they ride along in `StorytellerOsf` and come back to OSF",
        check: None,
    },
    Row {
        what: "`.fdx` shortcuts, outline levels, hidden text, scene CONTINUEDs, header margins",
        fidelity: NotMapped,
        how: "OSF has no place: they ride along in `fdx_xml`, `fdx_page_layout`, `fdx_mores` and come back to `.fdx`",
        check: None,
    },
    Row {
        what: "Ranges spell check ignores",
        fidelity: NotMapped,
        how: "`.fdx` `IgnoredRanges` are character spans OSF has no place for; kept in `.fdx`",
        check: None,
    },
    Row { what: "Run fonts, colours and highlights", fidelity: NotMapped, how: "not kept yet in either format", check: None },
    Row {
        what: "Beat Board, cast, tags, SmartType lists, header and footer, scene number options, spelling lists",
        fidelity: NotMapped,
        how: "kept in the format they came in; not converted (OSF lists and spelling likewise)",
        check: None,
    },
    Row {
        what: "Scene page and length",
        fidelity: NotMapped,
        how: "`.fdx` caches them and OSF does not; written from our pagination on every `.fdx` save",
        check: None,
    },
];

/// The report's table, as the document holds it.
fn table() -> String {
    let mut out = String::from("| What | Fidelity | How |\n| --- | --- | --- |\n");
    for row in ROWS {
        let fidelity = match row.fidelity {
            Exact => "exact",
            Approximated => "approximated",
            NotMapped => "cannot map",
        };
        out.push_str(&format!("| {} | {fidelity} | {} |\n", row.what, row.how));
    }
    out
}

const DOC: &str = r##"<?xml version="1.0" encoding="UTF-8" standalone="no" ?>
<FinalDraft DocumentType="Script" Template="No" Version="5">
  <Content>
    <Paragraph Type="Outline 1"><Text>ACT ONE</Text></Paragraph>
    <Paragraph Number="4A" Type="Scene Heading"><Text>INT. RADIO ROOM - NIGHT</Text></Paragraph>
    <Paragraph Type="Action">
      <ScriptNote Color="#FFFF00000000" Author="R. Lee"><Paragraph><Text>Louder?</Text></Paragraph></ScriptNote>
      <Text>The set </Text><Text RevisionID="1" Style="Bold">crackles</Text><Text>.</Text>
    </Paragraph>
    <Paragraph>
      <DualDialogue>
        <Paragraph Type="Character"><Text>MARA</Text></Paragraph>
        <Paragraph Type="Dialogue"><Text>Now.</Text></Paragraph>
        <Paragraph Type="Character"><Text>BEN</Text></Paragraph>
        <Paragraph Type="Dialogue"><Text>Later.</Text></Paragraph>
      </DualDialogue>
    </Paragraph>
    <Paragraph Type="New Act" StartsNewPage="Yes"><Text>ACT TWO</Text></Paragraph>
  </Content>
  <PageLayout BottomMargin="72" HeaderMargin="36" TopMargin="72"><PageSize Height="11.00" Width="8.50"/></PageLayout>
  <ElementSettings Type="Action"><FontSpec Font="Courier Prime" Size="12" Style=""/><ParagraphSpec Alignment="Left" FirstIndent="0.00" LeftIndent="1.50" RightIndent="7.50" SpaceBefore="12" Spacing="1"/><Behavior PaginateAs="Action" ReturnKey="Action" Shortcut="2"/></ElementSettings>
  <ElementSettings Type="New Act"><FontSpec Font="Courier Prime" Size="12" Style="Bold+Underline"/><ParagraphSpec Alignment="Center" LeftIndent="1.50" RightIndent="7.50" SpaceBefore="12"/><Behavior PaginateAs="Scene Heading" ReturnKey="Scene Heading"/></ElementSettings>
  <ElementSettings Type="Outline 1"><FontSpec Style="Bold+HiddenText"/><Outline Level="1"/></ElementSettings>
  <TitlePage><Content><Paragraph Alignment="Center" Type="Text"><Text>THE STATION</Text></Paragraph></Content></TitlePage>
  <SpellCheckIgnoreLists><IgnoredRanges/><IgnoredWords><Word>Mara</Word></IgnoredWords></SpellCheckIgnoreLists>
  <MoresAndContinueds><DialogueBreaks AutomaticCharacterContinueds="Yes" DialogueBottom="(MORE)" DialogueTop="(CONT'D)"/></MoresAndContinueds>
  <DeletedText><DeletedTextLocation Offset="30" RevisionID="1"/></DeletedText>
  <LockedPages><LockedPage PageNumber="1" Position="0"/><LockedPage PageNumber="1A" Position="40"/></LockedPages>
  <Revisions ActiveSet="1" RevisionMode="Yes"><Revision Color="#00000000FFFF" ID="1" Mark="*" Name="Blue Rev." PageColor="#C6C6EDEDFEFE"/></Revisions>
  <ScriptNotes><ScriptNote Color="#0000FFFF0000" Range="26,31" WriterName="A. Writer"><Paragraph><Text>Which set?</Text></Paragraph></ScriptNote></ScriptNotes>
</FinalDraft>
"##;

#[test]
fn every_exact_row_round_trips() {
    let script = fdx::parse(DOC).unwrap();
    let via_osf = osf::parse(&osf::write(&script)).unwrap();
    let back = fdx::parse(&fdx::write(&via_osf)).unwrap();
    let osf_back = osf::parse(&osf::write(&fdx::parse(&fdx::write(&via_osf)).unwrap())).unwrap();
    for row in ROWS.iter().filter(|r| r.fidelity == Exact) {
        let check = row.check.unwrap_or_else(|| panic!("an exact row needs a check: {}", row.what));
        let expected = check(&script);
        assert!(!expected.trim().is_empty() && expected != "[]", "the fixture shows nothing for: {}", row.what);
        assert_eq!(check(&via_osf), expected, ".fdx to OSF: {}", row.what);
        assert_eq!(check(&back), expected, ".fdx to OSF to .fdx: {}", row.what);
        assert_eq!(check(&osf_back), check(&via_osf), "OSF to .fdx to OSF: {}", row.what);
    }
}

#[test]
fn the_report_lists_every_row() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../docs/research/fdx-osf-fidelity.md");
    let mut doc = std::fs::read_to_string(path).unwrap();
    let start = doc.find("<!-- table:start -->\n").map(|at| at + "<!-- table:start -->\n".len()).expect("the report marks its table");
    let end = doc.find("<!-- table:end -->").expect("the report marks its table");
    if std::env::var_os("STORYTELLER_WRITE_CHECK_FILES").is_some() {
        doc.replace_range(start..end, &table());
        std::fs::write(path, &doc).unwrap();
        return;
    }
    assert_eq!(doc.get(start..end).unwrap_or_default(), table(), "the report's table is out of date: run with STORYTELLER_WRITE_CHECK_FILES=1");
}

/// Writes the owner's check files (run with `STORYTELLER_WRITE_CHECK_FILES=1`).
#[test]
fn check_files_are_current() {
    let script = fdx::parse(DOC).unwrap();
    let osf_text = osf::write(&script);
    let fdx_text = fdx::write(&osf::parse(&osf_text).unwrap());
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures");
    let (osf_path, fdx_path) = (format!("{dir}/lossless-check.osf"), format!("{dir}/lossless-check.fdx"));
    if std::env::var_os("STORYTELLER_WRITE_CHECK_FILES").is_some() {
        std::fs::write(&osf_path, &osf_text).unwrap();
        std::fs::write(&fdx_path, &fdx_text).unwrap();
    }
    assert_eq!(std::fs::read_to_string(&osf_path).unwrap_or_default(), osf_text, "rerun with STORYTELLER_WRITE_CHECK_FILES=1");
    assert_eq!(std::fs::read_to_string(&fdx_path).unwrap_or_default(), fdx_text, "rerun with STORYTELLER_WRITE_CHECK_FILES=1");
}
