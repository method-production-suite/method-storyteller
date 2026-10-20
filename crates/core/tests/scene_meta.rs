#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

//! A scene's title, summary and colour: read from `.fdx` `SceneProperties`, written back
//! untouched while unchanged, and carried through Open Screenplay Format. The script is invented.

use storyteller_core::model::{ElementKind as K, SceneMeta};
use storyteller_core::{Script, fdx, osf};

fn fdx_with(properties: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="no" ?>
<FinalDraft DocumentType="Script" Template="No" Version="5">
  <Content>
    <Paragraph Number="1" Type="Scene Heading">
      {properties}
      <Text>INT. FIELD STATION - NIGHT</Text>
    </Paragraph>
    <Paragraph Type="Action">
      <Text>The radio crackles.</Text>
    </Paragraph>
  </Content>
</FinalDraft>
"#
    )
}

fn heading(script: &Script) -> &SceneMeta {
    &script.elements.iter().find(|e| e.kind == K::SceneHeading).unwrap().scene
}

#[test]
fn fdx_scene_properties_are_read() {
    let script = fdx::parse(&fdx_with(
        r##"<SceneProperties Color="#FFFF00000000" Length="1/8" Page="1" Title="Static &amp; signal"><Summary>Nora hears the &quot;voice&quot;.</Summary></SceneProperties>"##,
    ))
    .unwrap();
    let meta = heading(&script);
    assert_eq!(meta.title, "Static & signal");
    assert_eq!(meta.summary, "Nora hears the \"voice\".");
    assert_eq!(meta.color.as_deref(), Some("#FFFF00000000"));
    assert_eq!(meta.rgb(), Some((255, 0, 0)));
    let plain = fdx::parse(&fdx_with(r#"<SceneProperties Length="1/8" Page="1" Title=""/>"#)).unwrap();
    assert!(heading(&plain).is_empty());
}

#[test]
fn unchanged_properties_go_back_as_they_came() {
    let properties = r##"<SceneProperties Color="#0000FFFF0000" Length="1/8" Page="1" Title="Static"><SceneArcBeats/><Summary>Nora listens.</Summary></SceneProperties>"##;
    let script = fdx::parse(&fdx_with(properties)).unwrap();
    let written = fdx::write(&script);
    assert!(written.contains(properties), "{written}");
}

#[test]
fn changed_properties_are_written_and_read_back() {
    let mut script = fdx::parse(&fdx_with(r#"<SceneProperties Length="1/8" Page="1" Title=""/>"#)).unwrap();
    let index = script.elements.iter().position(|e| e.kind == K::SceneHeading).unwrap();
    let meta = SceneMeta { title: "The call".into(), summary: "Nora answers.\nNobody speaks.".into(), color: Some("#00FF00".into()) };
    script.elements[index].scene = meta.clone();
    let written = fdx::write(&script);
    assert!(written.contains(r##"Color="#0000FFFF0000""##), "in Final Draft's own form: {written}");
    let back = fdx::parse(&written).unwrap();
    assert_eq!(heading(&back).title, meta.title, "a self-closing tag opens for the summary");
    assert_eq!(heading(&back).summary, meta.summary);
    assert_eq!(heading(&back).rgb(), meta.rgb());
    // Taken away again.
    let mut again = back.clone();
    again.elements[index].scene = SceneMeta::default();
    let written = fdx::write(&again);
    assert!(!written.contains("Color=") && !written.contains("<Summary>"), "{written}");
    assert!(heading(&fdx::parse(&written).unwrap()).is_empty());
    // A heading with no properties at all gets them.
    let mut fresh = storyteller_core::fountain::parse("INT. HUT - DAY\n\nRain.\n").unwrap();
    fresh.elements[0].scene = meta.clone();
    let back = fdx::parse(&fdx::write(&fresh)).unwrap();
    assert_eq!((heading(&back).title.as_str(), heading(&back).rgb()), (meta.title.as_str(), meta.rgb()));
}

#[test]
fn osf_carries_the_properties_both_ways() {
    let mut script = storyteller_core::fountain::parse("INT. HUT - DAY\n\nRain.\n").unwrap();
    script.elements[0].scene = SceneMeta { title: "Hut".into(), summary: "Rain, \"loud\" & long.".into(), color: Some("#336699".into()) };
    let written = osf::write(&script);
    assert!(written.contains("<method_storyteller_scene"), "{written}");
    let back = osf::parse(&written).unwrap();
    assert_eq!(heading(&back), heading(&script));
    assert_eq!(osf::write(&back), written, "and the same document again");
    // Across: OSF to .fdx to OSF.
    let across = osf::parse(&osf::write(&fdx::parse(&fdx::write(&back)).unwrap())).unwrap();
    assert_eq!(
        (heading(&across).title.as_str(), heading(&across).summary.as_str()),
        (heading(&script).title.as_str(), heading(&script).summary.as_str())
    );
    assert_eq!(heading(&across).rgb(), heading(&script).rgb(), "the colour, in .fdx's form now");
}
