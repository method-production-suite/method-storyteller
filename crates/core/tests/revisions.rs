#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing, clippy::type_complexity)]

//! Revision sets and revised text in `.fdx` and Open Screenplay Format, and between them. The
//! documents are invented, shaped like the files the format notes describe.

use storyteller_core::model::{Revised, RevisionSet, Revisions, Script};
use storyteller_core::{fdx, fountain, osf};

const DOC: &str = r##"<?xml version="1.0" encoding="UTF-8" standalone="no" ?>
<FinalDraft DocumentType="Script" Template="No" Version="5">
  <Content>
    <Paragraph Type="Action">
      <Text AdornmentStyle="0" RevisionID="0">The pump </Text>
      <Text RevisionID="1">stops</Text>
      <Text RevisionID="2" Style="Bold"> dead</Text>
      <Text RevisionID="0">.</Text>
    </Paragraph>
    <Paragraph Type="Action">
      <Text RevisionID="2">Nobody moves.</Text>
    </Paragraph>
  </Content>
  <Revisions ActiveSet="2" Location="7.75" RevisionMode="Yes" RevisionsShown="Active" ShowAllMarks="No" ShowAllSets="No" ShowPageColor="No">
    <Revision Color="#00000000FFFF" FullRevision="No" ID="1" Mark="*" Name="Blue Rev. (10/01/26)" PageColor="#C6C6EDEDFEFE" Style=""/>
    <Revision Color="#FFFF00000000" FullRevision="No" ID="2" Mark="*" Name="Pink Rev. (10/08/26)" PageColor="#FDFDCCCCD4D4" Style=""/>
  </Revisions>
</FinalDraft>
"##;

/// The revisions without the Final Draft data, and every paragraph's revised spans.
fn model(script: &Script) -> (Option<u32>, Vec<(u32, String, String, Option<String>, Option<String>)>, Vec<Vec<Revised>>) {
    let sets = script.revisions.sets.iter().map(|s| (s.id, s.name.clone(), s.mark.clone(), s.color.clone(), s.page_color.clone())).collect();
    (script.revisions.active, sets, script.elements.iter().map(|e| e.revised.clone()).collect())
}

fn fdx_data(script: &Script) -> (Vec<(String, String)>, Vec<Vec<(String, String)>>) {
    (script.revisions.fdx_attributes.clone(), script.revisions.sets.iter().map(|s| s.fdx_attributes.clone()).collect())
}

#[test]
fn reads_sets_and_revised_text() {
    let script = fdx::parse(DOC).unwrap();
    assert_eq!(script.elements[0].text, "The pump stops** dead**.");
    let (active, sets, revised) = model(&script);
    assert_eq!(active, Some(2));
    assert_eq!(sets[0], (1, "Blue Rev. (10/01/26)".into(), "*".into(), Some("#0000FF".into()), Some("#C6EDFE".into())));
    assert_eq!(sets[1].4.as_deref(), Some("#FDCCD4"));
    assert_eq!(revised[0], [Revised { from: 9, to: 14, set: 1 }, Revised { from: 14, to: 19, set: 2 }]);
    assert_eq!(revised[1], [Revised { from: 0, to: 13, set: 2 }]);
    assert_eq!(script.elements[0].revision_at(10), Some(1));
    assert_eq!(script.elements[0].revision_at(19), None);
}

#[test]
fn fdx_to_fdx_keeps_sets_and_marks() {
    let script = fdx::parse(DOC).unwrap();
    let xml = fdx::write(&script);
    assert!(xml.contains(r#"<Revisions ActiveSet="2" Location="7.75" RevisionMode="Yes""#), "{xml}");
    assert!(xml.contains("<Text>The pump </Text>\n      <Text RevisionID=\"1\">stops</Text>\n      <Text RevisionID=\"2\" Style=\"Bold\"> dead</Text>\n      <Text>.</Text>"), "{xml}");
    let again = fdx::parse(&xml).unwrap();
    assert_eq!(model(&again), model(&script));
    assert_eq!(again.fdx, script.fdx);
    assert_eq!(fdx::write(&again), xml, "writing is stable");
}

#[test]
fn fdx_to_osf_to_fdx_keeps_everything() {
    let script = fdx::parse(DOC).unwrap();
    let xml = osf::write(&script);
    assert!(xml.contains(r#"<text revision="1">stops</text>"#) && xml.contains(r#"<text bold="1" revision="2"> dead</text>"#), "{xml}");
    assert!(xml.contains(r##"<revision_color name="Pink Rev. (10/08/26)" index="2" color_name="Pink" color_index="2" mark="*""##), "{xml}");
    assert!(xml.contains(r#" revision="2""#), "the active set: {xml}");
    let via_osf = osf::parse(&xml).unwrap();
    assert_eq!(model(&via_osf), model(&script));
    assert_eq!(fdx_data(&via_osf), fdx_data(&script));
    assert_eq!(osf::write(&via_osf), xml, "writing OSF is stable");

    let back = fdx::parse(&fdx::write(&via_osf)).unwrap();
    assert_eq!(model(&back), model(&script));
    assert_eq!(fdx_data(&back), fdx_data(&script));
}

const OSF: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<document type="Open Screenplay Format document" version="30">
  <settings page_width="2159" page_height="2794" revision="1"/>
  <paragraphs>
    <para>
      <style basestylename="Action"/>
      <text>The tide </text>
      <text revision="1">turns</text>
      <text>.</text>
    </para>
  </paragraphs>
  <lists><characters/><revision_colors><revision_color name="White" index="0" color_name="White" color_index="0" mark=""/><revision_color name="Blue" index="1" color_name="Blue" color_index="1" mark="*"/></revision_colors></lists>
</document>
"#;

#[test]
fn osf_to_fdx_to_osf_keeps_everything() {
    let script = osf::parse(OSF).unwrap();
    let (active, sets, revised) = model(&script);
    assert_eq!(active, Some(1));
    assert_eq!(sets.len(), 2);
    assert_eq!(revised[0], [Revised { from: 9, to: 14, set: 1 }]);
    assert_eq!(osf::write(&script), osf::write(&osf::parse(&osf::write(&script)).unwrap()), "stable");
    assert!(osf::write(&script).contains("<lists><characters/><revision_colors>"), "an unchanged list goes back as it was");

    let xml = fdx::write(&script);
    assert!(xml.contains(r#"<Text RevisionID="1">turns</Text>"#) && xml.contains(r#"<Revisions ActiveSet="1">"#), "{xml}");
    let via_fdx = fdx::parse(&xml).unwrap();
    assert_eq!(model(&via_fdx), model(&script));
    let back = osf::write(&via_fdx);
    assert!(
        back.contains(r#"<revision_color name="Blue" index="1" color_name="Blue" color_index="1" mark="*"/>"#) && !back.contains("fdx_"),
        "nothing but what the set says: {back}"
    );
    assert_eq!(model(&osf::parse(&back).unwrap()), model(&script));
}

#[test]
fn changed_sets_are_written_from_the_model() {
    let mut script = fdx::parse(DOC).unwrap();
    script.revisions.sets[1].name = "Pink Rev. (10/09/26)".into();
    script.revisions.active = Some(1);
    let xml = fdx::write(&script);
    assert!(xml.contains(r#"<Revisions ActiveSet="1" Location="7.75" RevisionMode="Yes" RevisionsShown="Active""#), "{xml}");
    assert!(
        xml.contains(
            r##"<Revision Color="#FFFF00000000" FullRevision="No" ID="2" Mark="*" Name="Pink Rev. (10/09/26)" PageColor="#FDFDCCCCD4D4" Style=""/>"##
        ),
        "{xml}"
    );
    assert_eq!(model(&fdx::parse(&xml).unwrap()), model(&script));

    let mut script = osf::parse(OSF).unwrap();
    script.revisions.active = Some(0);
    script.revisions.sets.push(RevisionSet { id: 2, name: "Pink".into(), mark: "*".into(), ..RevisionSet::default() });
    let xml = osf::write(&script);
    assert!(xml.contains(r#"revision="0""#) && xml.contains("<lists><characters/><revision_colors>"), "{xml}");
    assert_eq!(model(&osf::parse(&xml).unwrap()).1.len(), 3);
}

#[test]
fn a_new_script_writes_its_sets() {
    let mut script = fountain::parse("The bell rings.\n").unwrap();
    script.revisions = Revisions {
        active: Some(1),
        sets: vec![RevisionSet { id: 1, name: "Blue".into(), mark: "*".into(), color: Some("#0000FF".into()), ..RevisionSet::default() }],
        ..Revisions::default()
    };
    script.elements[0].revised.push(Revised { from: 4, to: 8, set: 1 });
    for back in [fdx::parse(&fdx::write(&script)).unwrap(), osf::parse(&osf::write(&script)).unwrap()] {
        assert_eq!(model(&back), model(&script));
    }
}
