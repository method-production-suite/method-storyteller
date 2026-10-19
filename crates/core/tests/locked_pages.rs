#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

//! Locked pages and A-pages in `.fdx` and Open Screenplay Format, and between them. The documents
//! are invented, shaped like the files the format notes describe.

use storyteller_core::model::{PageStart, Script};
use storyteller_core::{fdx, fountain, osf};

/// `INT. PUMP ROOM - NIGHT` is 0..22, the long action 23..83, `It holds.` 84..93.
const DOC: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="no" ?>
<FinalDraft DocumentType="Script" Template="No" Version="5">
  <Content>
    <Paragraph Type="Scene Heading">
      <Text>INT. PUMP ROOM - NIGHT</Text>
    </Paragraph>
    <Paragraph Type="Action">
      <Text>Water rises past the first step, then the second, then more.</Text>
    </Paragraph>
    <Paragraph Type="Action">
      <Text>It holds.</Text>
    </Paragraph>
  </Content>
  <LockedPages>
    <LockedPage Deleted="0" LevelIndex="0" LockLevel="0" PageNumber="1" Position="0"/>
    <LockedPage Deleted="0" LevelIndex="1" LockLevel="0" PageNumber="2" Position="54"/>
    <LockedPage Deleted="0" LevelIndex="0" LockLevel="0" PageNumber="3" Position="84"/>
  </LockedPages>
</FinalDraft>
"#;

/// Every page start: its paragraph, place and label.
fn pages(script: &Script) -> Vec<(usize, u32, String)> {
    script.elements.iter().enumerate().flat_map(|(i, e)| e.page_starts.iter().map(move |p| (i, p.at, p.label.clone()))).collect()
}

fn fdx_data(script: &Script) -> Vec<Vec<(String, String)>> {
    script.elements.iter().flat_map(|e| &e.page_starts).map(|p| p.fdx_attributes.clone()).collect()
}

#[test]
fn reads_each_locked_page_onto_its_paragraph() {
    let script = fdx::parse(DOC).unwrap();
    assert!(script.pages_locked());
    assert_eq!(pages(&script), [(0, 0, "1".into()), (1, 31, "2".into()), (2, 0, "3".into())]);
}

#[test]
fn fdx_to_fdx_keeps_the_list_as_written() {
    let script = fdx::parse(DOC).unwrap();
    let xml = fdx::write(&script);
    assert!(xml.contains(r#"<LockedPage Deleted="0" LevelIndex="1" LockLevel="0" PageNumber="2" Position="54"/>"#), "{xml}");
    let again = fdx::parse(&xml).unwrap();
    assert_eq!(pages(&again), pages(&script));
    assert_eq!(again.fdx, script.fdx);
    assert_eq!(fdx::write(&again), xml, "writing is stable");
}

#[test]
fn fdx_to_osf_to_fdx_keeps_every_page() {
    let script = fdx::parse(DOC).unwrap();
    let xml = osf::write(&script);
    assert!(xml.contains(r#"pages_locked="1""#), "{xml}");
    assert!(xml.contains(r#"<para page_number="1">"#) && xml.contains(r#"<para page_number="3">"#), "{xml}");
    assert!(xml.contains(r#"<method_storyteller_page at="31" label="2""#), "a page that starts inside a paragraph: {xml}");
    let via_osf = osf::parse(&xml).unwrap();
    assert_eq!(pages(&via_osf), pages(&script));
    assert_eq!(fdx_data(&via_osf), fdx_data(&script));
    assert_eq!(osf::write(&via_osf), xml, "writing OSF is stable");

    let back = fdx::parse(&fdx::write(&via_osf)).unwrap();
    assert_eq!(pages(&back), pages(&script));
    assert_eq!(fdx_data(&back), fdx_data(&script));
    assert!(fdx::write(&back).contains(r#"PageNumber="2" Position="54""#));
}

const OSF: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<document type="Open Screenplay Format document" version="30">
  <settings page_width="2159" page_height="2794" pages_locked="1" pagenumber_mode="1AB"/>
  <paragraphs>
    <para page_number="1">
      <style basestylename="Scene Heading"/>
      <text>EXT. DOCK - DAWN</text>
    </para>
    <para page_number="1">
      <style basestylename="Action"/>
      <text>Fog.</text>
    </para>
    <para page_number="2">
      <style basestylename="Action"/>
      <text>A horn.</text>
    </para>
    <para page_number="2A">
      <style basestylename="Action"/>
      <text>Silence.</text>
    </para>
  </paragraphs>
</document>
"#;

#[test]
fn osf_to_fdx_to_osf_keeps_a_pages() {
    let script = osf::parse(OSF).unwrap();
    assert_eq!(pages(&script), [(0, 0, "1".into()), (2, 0, "2".into()), (3, 0, "2A".into())]);
    assert_eq!(osf::write(&script), osf::write(&osf::parse(&osf::write(&script)).unwrap()), "stable");
    assert!(osf::write(&script).contains(r#"<para page_number="1">"#));

    let xml = fdx::write(&script);
    assert!(xml.contains(r#"<LockedPage PageNumber="2A" Position="30"/>"#), "{xml}");
    let via_fdx = fdx::parse(&xml).unwrap();
    assert_eq!(pages(&via_fdx), pages(&script));
    let back = osf::write(&via_fdx);
    assert!(!back.contains("method_storyteller_page"), "page numbers alone say it all: {back}");
    assert_eq!(pages(&osf::parse(&back).unwrap()), pages(&script));
}

#[test]
fn unlocked_page_numbers_stay_as_they_were() {
    let doc = OSF.replace(r#"pages_locked="1""#, r#"pages_locked="0""#);
    let script = osf::parse(&doc).unwrap();
    assert!(!script.pages_locked());
    assert!(osf::write(&script).contains(r#"<para page_number="2A">"#));
}

#[test]
fn edits_move_positions_and_new_locks_are_written() {
    let mut script = fdx::parse(DOC).unwrap();
    script.elements[0].text = "INT. PUMP ROOM - LATER".into();
    script.elements[0].text.push_str(" STILL");
    let xml = fdx::write(&script);
    assert!(xml.contains(r#"PageNumber="2" Position="60""#) && xml.contains(r#"PageNumber="3" Position="90""#), "{xml}");

    let mut script = fountain::parse("INT. HUT - DAY\n\nRain.\n").unwrap();
    script.elements[0].page_starts.push(PageStart { at: 0, label: "1".into(), ..PageStart::default() });
    script.elements[1].page_starts.push(PageStart { at: 2, label: "1A".into(), ..PageStart::default() });
    for back in [fdx::parse(&fdx::write(&script)).unwrap(), osf::parse(&osf::write(&script)).unwrap()] {
        assert_eq!(pages(&back), pages(&script));
    }
}
