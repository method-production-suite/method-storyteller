#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

//! Final Draft files keep everything the model does not use, and only real script paragraphs
//! become elements. The document is synthetic, shaped like Final Draft 12 and 13 files.

use storyteller_core::fdx;
use storyteller_core::model::{ElementKind as K, FdxSection};

const DOC: &str = r##"<?xml version="1.0" encoding="UTF-8" standalone="no" ?>
<FinalDraft DocumentType="Script" Template="No" Version="6">
  <Content>
    <Paragraph Type="Outline 1" id="a1"/>
    <Paragraph Type="Outline 1" id="a1">
      <Text>ACT ONE</Text>
    </Paragraph>
    <Paragraph Type="Summary">
      <Text>She finds the key.</Text>
    </Paragraph>
    <Paragraph Number="1" Type="Scene Heading">
      <SceneProperties Length="2/8" Page="1" Title="">
        <SceneArcBeats>
          <CharacterArcBeat Name="ANNA">
            <Paragraph Type="Action">
              <Text>Anna changes her mind.</Text>
            </Paragraph>
          </CharacterArcBeat>
        </SceneArcBeats>
        <Paragraph Type="Action"><Text>A synopsis that never prints.</Text></Paragraph>
      </SceneProperties>
      <Text>INT. HOUSE - DAY</Text>
    </Paragraph>
    <Paragraph Type="Action" Alignment="Left" FirstIndent="0.00" Leading="Regular">
      <ScriptNote ID="7" Color="#FFFFFFFF0000"><Paragraph><Text>check this</Text></Paragraph></ScriptNote>
      <Text AdornmentStyle="0" Font="Courier Final Draft" RevisionID="0" Style="AllCaps">a loud</Text>
      <Text> noise.</Text>
    </Paragraph>
    <Paragraph Number="2" Type="Scene Heading">
      <OmittedScene>
        <Paragraph Number="2" Type="Scene Heading">
          <SceneProperties Length="1/8" Page="1" Title=""/>
          <Text>EXT. GARDEN - NIGHT</Text>
        </Paragraph>
      </OmittedScene>
      <Text>OMITTED</Text>
    </Paragraph>
    <Paragraph Type="New Act">
      <Text>ACT TWO</Text>
    </Paragraph>
    <Paragraph Type="Writer Note">
      <Text>Hidden by the element settings.</Text>
    </Paragraph>
    <Paragraph Type="General">
      <Text>Printed like action.</Text>
    </Paragraph>
  </Content>
  <TitlePage>
    <Content>
      <Paragraph Alignment="Center" Type="Text"><Text>THE KEY</Text></Paragraph>
      <Paragraph Alignment="Center" Type="Text"><Text>Written by</Text></Paragraph>
      <Paragraph Alignment="Center" Type="Text"><Text>A. Writer</Text></Paragraph>
    </Content>
  </TitlePage>
  <ElementSettings Type="Writer Note"><FontSpec Style="Italic+HiddenText"/></ElementSettings>
  <ElementSettings Type="Outline 1"><FontSpec Style="Bold+HiddenText"/></ElementSettings>
  <ElementSettings Type="General"><FontSpec Style=""/></ElementSettings>
  <Revisions ActiveSet="1"><Revision Color="Blue" ID="1" Mark="*" Name="Blue"/></Revisions>
  <LockedPages/>
</FinalDraft>
"##;

#[test]
fn only_script_paragraphs_become_elements() {
    let script = fdx::parse(DOC).unwrap();
    let kinds: Vec<K> = script.elements.iter().map(|e| e.kind).collect();
    assert_eq!(
        kinds,
        // New Act is an act line; General prints like action.
        [K::Section, K::Synopsis, K::SceneHeading, K::Action, K::SceneHeading, K::NewAct, K::Synopsis, K::Action],
        "{:#?}",
        script.elements
    );
    let printed: Vec<&str> = script.elements.iter().filter(|e| e.kind.is_printed()).map(|e| e.text.as_str()).collect();
    assert_eq!(printed, ["INT. HOUSE - DAY", "A LOUD noise.", "OMITTED", "ACT TWO", "Printed like action."]);
    assert_eq!(script.elements[0].level, 1);
    assert_eq!(script.elements[4].scene_number.as_deref(), Some("2"));
    assert_eq!(script.title_page.get("Title"), Some("THE KEY"));
}

#[test]
fn unmodelled_parts_are_written_back() {
    let script = fdx::parse(DOC).unwrap();
    let xml = fdx::write(&script);
    assert!(xml.contains(r#"<FinalDraft DocumentType="Script" Template="No" Version="6">"#), "{xml}");
    for part in [
        r#"<ElementSettings Type="Writer Note"><FontSpec Style="Italic+HiddenText"/></ElementSettings>"#,
        r#"<Revisions ActiveSet="1"><Revision Color="Blue" ID="1" Mark="*" Name="Blue"/></Revisions>"#,
        "<LockedPages/>",
        "<OmittedScene>",
        "<CharacterArcBeat Name=\"ANNA\">",
        r##"<ScriptNote ID="7" Color="#FFFFFFFF0000"><Paragraph><Text>check this</Text></Paragraph></ScriptNote>"##,
        r#"<Paragraph Type="Writer Note">"#,
        r#"<Paragraph Type="New Act">"#,
        r#"<Paragraph Type="Action" Alignment="Left" FirstIndent="0.00" Leading="Regular">"#,
        r#"<Paragraph Alignment="Center" Type="Text"><Text>THE KEY</Text></Paragraph>"#,
    ] {
        assert!(xml.contains(part), "missing {part} in\n{xml}");
    }
    let sections: Vec<&str> = match &script.fdx {
        Some(doc) => doc
            .sections
            .iter()
            .map(|s| match s {
                FdxSection::TitlePage { .. } => "TitlePage",
                FdxSection::Raw(raw) => raw.split([' ', '>', '/']).next().unwrap_or_default(),
            })
            .collect(),
        None => panic!("an import keeps its document data"),
    };
    assert_eq!(sections, ["TitlePage", "<ElementSettings", "<ElementSettings", "<ElementSettings", "<Revisions", "<LockedPages"]);

    let again = fdx::parse(&xml).unwrap();
    let text = |s: &storyteller_core::Script| s.elements.iter().map(|e| (e.kind, e.text.clone())).collect::<Vec<_>>();
    assert_eq!(text(&again), text(&script), "a second import has the same script");
    assert_eq!(again.fdx, script.fdx, "and the same document sections");
    assert_eq!(fdx::write(&again), xml, "writing is stable");
}

#[test]
fn a_changed_kind_drops_the_imported_type_name() {
    let mut script = fdx::parse(DOC).unwrap();
    let index = script.elements.iter().position(|e| e.text == "ACT TWO").unwrap();
    script.elements[index].kind = K::Transition;
    let xml = fdx::write(&script);
    assert!(!xml.contains(r#"Type="New Act""#), "{xml}");
    assert!(xml.contains("<Paragraph Type=\"Transition\">\n      <Text>ACT TWO</Text>"), "{xml}");
}

#[test]
fn a_changed_title_page_is_written_from_the_model() {
    let mut script = fdx::parse(DOC).unwrap();
    script.title_page.set("Title", "THE DOOR");
    let xml = fdx::write(&script);
    assert!(xml.contains("THE DOOR") && !xml.contains("THE KEY"), "{xml}");
    assert_eq!(xml.matches("<TitlePage>").count(), 1);
}

#[test]
fn outline_types_print_when_the_file_says_so() {
    let doc = DOC.replace(r#"<FontSpec Style="Bold+HiddenText"/>"#, r#"<FontSpec Style="Bold"/>"#);
    let script = fdx::parse(&doc).unwrap();
    assert_eq!(script.elements[0].kind, K::Action);
    assert!(fdx::write(&script).contains(r#"<Paragraph Type="Outline 1" id="a1">"#));
    let doc = DOC.replace(
        r#"<ElementSettings Type="General">"#,
        r#"<ElementSettings Type="Summary"><FontSpec Style=""/></ElementSettings><ElementSettings Type="General">"#,
    );
    let script = fdx::parse(&doc).unwrap();
    assert_eq!(script.elements[1].kind, K::Action, "a printing summary type prints");
    assert!(fdx::write(&script).contains(r#"<Paragraph Type="Summary">"#));
}

#[test]
fn sections_and_synopses_export_as_outline_and_summary() {
    let script = storyteller_core::fountain::parse("# Act One\n\n## Part\n\n= Things happen.\n\nINT. HOUSE - DAY\n").unwrap();
    let xml = fdx::write(&script);
    assert!(xml.contains(r#"<Paragraph Type="Outline 1">"#) && xml.contains(r#"<Paragraph Type="Outline 2">"#), "{xml}");
    assert!(xml.contains(r#"<Paragraph Type="Summary">"#) && !xml.contains("Synopsis") && !xml.contains("Level="), "{xml}");
    let back = fdx::parse(&xml).unwrap();
    let kinds: Vec<K> = back.elements.iter().map(|e| e.kind).collect();
    assert_eq!(kinds, [K::Section, K::Section, K::Synopsis, K::SceneHeading]);
    assert_eq!(back.elements[1].level, 2);
}

#[test]
fn every_export_writes_scene_pages_and_lengths() {
    let script = storyteller_core::fountain::parse("INT. ONE - DAY\n\nA line.\n\nEXT. TWO - NIGHT\n\nAnother line.\n").unwrap();
    let xml = fdx::write(&script);
    // Two blank lines before the heading, the heading, a blank, the action: 5 lines, one eighth.
    assert_eq!(xml.matches(r#"<SceneProperties Length="1/8" Page="1" Title=""/>"#).count(), 2, "{xml}");

    // An imported scene keeps its other properties; the stale page and length are replaced.
    let script = fdx::parse(DOC).unwrap();
    let xml = fdx::write(&script);
    assert!(xml.contains(r#"<SceneProperties Length="1/8" Page="1" Title="">"#), "{xml}");
    assert!(xml.contains("<CharacterArcBeat Name=\"ANNA\">"), "{xml}");
    // The omitted scene's own heading reports no length.
    assert!(xml.contains("<SceneProperties Length=\"0/8\" Page=\"1\" Title=\"\"/><OmittedScene>"), "{xml}");
}

#[test]
fn eighths_are_written_like_final_draft() {
    use storyteller_core::paginate::format_eighths;
    assert_eq!(format_eighths(0), "0/8");
    assert_eq!(format_eighths(3), "3/8");
    assert_eq!(format_eighths(8), "1");
    assert_eq!(format_eighths(10), "1 2/8");
}
