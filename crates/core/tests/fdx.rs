#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

use storyteller_core::model::{Element, ElementKind as K, Script};
use storyteller_core::{CoreError, fdx, fountain};

/// The script without the Final Draft data an import keeps for writing back (document
/// sections, and per-paragraph extras such as the scene properties every export writes).
fn model(mut script: Script) -> Script {
    script.fdx = None;
    for element in &mut script.elements {
        element.fdx = None;
    }
    script
}

fn fixture() -> Script {
    let text = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/lighthouse.fountain")).unwrap();
    fountain::parse(&text).unwrap()
}

#[test]
fn fixture_round_trips_through_fdx() {
    let script = fixture();
    let xml = fdx::write(&script);
    let back = fdx::parse(&xml).unwrap();
    assert_eq!(model(back.clone()), script, "xml:\n{xml}");
    assert_eq!(fdx::write(&back), xml, "writing is stable");
}

#[test]
fn fountain_to_fdx_to_fountain_is_lossless() {
    let script = fixture();
    let fountain_text = fountain::write(&script);
    let via_fdx = fountain::write(&fdx::parse(&fdx::write(&script)).unwrap());
    assert_eq!(via_fdx, fountain_text);
}

#[test]
fn shots_and_styles_round_trip() {
    let script = Script {
        elements: vec![
            Element::new(K::Shot, "CLOSE ON THE LENS"),
            Element::new(K::Action, "Plain **bold** *italic* _under_ ***both*** & <tags> \"quotes\" 'apostrophes'"),
            Element::new(K::Action, "A literal \\* star and file\\_name"),
        ],
        ..Script::default()
    };
    assert_eq!(model(fdx::parse(&fdx::write(&script)).unwrap()), script);
}

/// A document shaped like one written by another FDX application: settings blocks, a header
/// with its own paragraphs, entities, character references, combined styles, dual dialogue
/// and a title page without our field markers.
const FOREIGN: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="no" ?>
<FinalDraft DocumentType="Script" Template="No" Version="5">
  <Content>
    <Paragraph Type="Scene Heading" Number="7">
      <SceneProperties Length="1/8" Page="1" Title=""/>
      <Text>INT. PUB &amp; GRILL - NIGHT</Text>
    </Paragraph>
    <Paragraph Type="Action">
      <Text>Glasses </Text><Text Style="Bold+Underline">clink</Text><Text>. It&apos;s loud &#8212; very.</Text>
    </Paragraph>
    <Paragraph>
      <DualDialogue>
        <Paragraph Type="Character"><Text>ALICE</Text></Paragraph>
        <Paragraph Type="Dialogue"><Text>Cheers!</Text></Paragraph>
        <Paragraph Type="Character"><Text>BOB</Text></Paragraph>
        <Paragraph Type="Parenthetical"><Text>(late)</Text></Paragraph>
        <Paragraph Type="Dialogue"><Text>Cheers.</Text></Paragraph>
      </DualDialogue>
    </Paragraph>
    <Paragraph Type="General" StartsNewPage="Yes"><Text>General text.</Text></Paragraph>
    <Paragraph Type="Action" Alignment="Center"><Text>THE END</Text></Paragraph>
    <Paragraph Type="Transition"><Text><![CDATA[FADE OUT.]]></Text></Paragraph>
  </Content>
  <HeaderAndFooter HeaderFirstPage="No">
    <Header><Paragraph Type="Header"><Text>NOT SCRIPT TEXT</Text></Paragraph></Header>
  </HeaderAndFooter>
  <ElementSettings Type="Action"><FontSpec Font="Courier Final Draft" Size="12"/></ElementSettings>
  <TitlePage>
    <Content>
      <Paragraph Alignment="Center"><Text>LAST ORDERS</Text></Paragraph>
      <Paragraph Alignment="Center"><Text></Text></Paragraph>
      <Paragraph Alignment="Center"><Text>Written by</Text></Paragraph>
      <Paragraph Alignment="Center"><Text>A. Writer</Text></Paragraph>
      <Paragraph Alignment="Left"><Text>agent@example.com</Text></Paragraph>
      <Paragraph Alignment="Right"><Text>Draft 2</Text></Paragraph>
    </Content>
  </TitlePage>
</FinalDraft>
"#;

#[test]
fn imports_a_foreign_document() {
    let script = fdx::parse(FOREIGN).unwrap();
    let e = &script.elements;
    let kinds: Vec<K> = e.iter().map(|e| e.kind).collect();
    assert_eq!(
        kinds,
        [
            K::SceneHeading,
            K::Action,
            K::Character,
            K::Dialogue,
            K::Character,
            K::Parenthetical,
            K::Dialogue,
            K::PageBreak,
            K::Action,
            K::Centered,
            K::Transition
        ]
    );
    assert_eq!(e[0].text, "INT. PUB & GRILL - NIGHT");
    assert_eq!(e[0].scene_number.as_deref(), Some("7"));
    assert_eq!(e[1].text, "Glasses _**clink**_. It's loud \u{2014} very.");
    assert!(!e[2].dual);
    assert!(e[4].dual);
    assert_eq!(e[10].text, "FADE OUT.");
    assert_eq!(script.title_page.get("Title"), Some("LAST ORDERS"));
    assert_eq!(script.title_page.get("Credit"), Some("Written by"));
    assert_eq!(script.title_page.get("Author"), Some("A. Writer"));
    assert_eq!(script.title_page.get("Contact"), Some("agent@example.com"));
    assert_eq!(script.title_page.get("Draft date"), Some("Draft 2"));
}

#[test]
fn rejects_hostile_or_foreign_xml() {
    assert!(matches!(fdx::parse("<html><body/></html>"), Err(CoreError::NotFdx(_))));
    assert!(matches!(fdx::parse(""), Err(CoreError::NotFdx(_))));
    assert!(matches!(fdx::parse("<FinalDraft><Content><Paragraph>"), Err(CoreError::Xml(_)) | Ok(_)));
    assert!(fdx::parse("<FinalDraft><Content></Paragraph></Content></FinalDraft>").is_err());
    let deep = format!("<FinalDraft>{}{}</FinalDraft>", "<a>".repeat(1000), "</a>".repeat(1000));
    assert!(matches!(fdx::parse(&deep), Err(CoreError::Xml(_))));
    let entity_bomb =
        "<!DOCTYPE x [<!ENTITY a \"aaaa\">]><FinalDraft><Content><Paragraph Type=\"Action\"><Text>&a;&a;</Text></Paragraph></Content></FinalDraft>";
    let script = fdx::parse(entity_bomb).unwrap();
    assert_eq!(script.elements[0].text, "", "custom entities are not expanded");
}

#[test]
fn a_note_on_the_title_page_does_not_print_and_comes_back() {
    let script = fountain::parse("Title: FIELD STATION\nNotes: Call the producer [[before Friday]]\n\nINT. HUT - DAY\n\nWind.\n").unwrap();
    assert_eq!(script.title_page.get("Notes"), Some("Call the producer [[before Friday]]"));
    let xml = fdx::write(&script);
    assert!(!xml.contains("[["), "no Fountain note printed on the title page:\n{xml}");
    assert!(xml.contains("<ScriptNote"), "the note is a script note:\n{xml}");
    let back = fdx::parse(&xml).unwrap();
    assert_eq!(back.title_page.get("Notes"), Some("Call the producer [[before Friday]]"));
    assert_eq!(fountain::write(&back), fountain::write(&script), "Fountain to .fdx to Fountain keeps it");
}
