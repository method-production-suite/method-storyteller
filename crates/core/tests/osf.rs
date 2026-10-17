#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

//! Open Screenplay Format import and export. The documents here are invented, shaped like the
//! format's published examples.

use storyteller_core::layout::Layout;
use storyteller_core::model::{ElementKind as K, Script};
use storyteller_core::{fountain, osf};

/// The script without the OSF data an import keeps for writing back.
fn model(mut script: Script) -> Script {
    script.osf = None;
    for element in &mut script.elements {
        element.osf = None;
    }
    script
}

fn fixture() -> Script {
    let text = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/lighthouse.fountain")).unwrap();
    fountain::parse(&text).unwrap()
}

#[test]
fn fountain_to_osf_and_back_is_lossless() {
    let script = fixture();
    let xml = osf::write(&script);
    assert!(xml.contains(r#"<document type="Open Screenplay Format document" version="30">"#), "{xml}");
    let back = osf::parse(&xml).unwrap();
    assert_eq!(model(back.clone()), script, "xml:\n{xml}");
    assert_eq!(fountain::write(&back), fountain::write(&script));
    assert_eq!(osf::write(&back), xml, "writing is stable");
}

const V30: &str = r##"<?xml version="1.0" encoding="UTF-8"?>
<document type="Open Screenplay Format document" version="30">
  <info uuid="0b0c" pagecount="1"/>
  <settings page_width="2100" page_height="2970" margin_top="254" margin_bottom="254" margin_left="381" margin_right="254" more_text="(MORE)" cont_text="(cont'd)"/>
  <styles>
    <style name="Normal Text" builtin="1" builtin_index="0"/>
    <style name="Scene Heading" builtin="1" builtin_index="1" basestylename="Normal Text" spacebefore="1.0"/>
    <style name="Action" builtin="1" builtin_index="2" basestylename="Normal Text"/>
    <style name="Character" builtin="1" builtin_index="3" basestylename="Normal Text" leftindent="508" rightindent="64"/>
    <style name="Dialogue" builtin="1" builtin_index="5" basestylename="Normal Text" leftindent="254" rightindent="381"/>
    <style name="Act Heading" basestylename="Scene Heading"/>
  </styles>
  <fadein_settings saved_with="test"/>
  <paragraphs>
    <para edited_by="AB" scene_number="4A">
      <style basestylename="Scene Heading"/>
      <text>INT. HUT - DAY</text>
    </para>
    <para note="check the date" synopsis="They meet." synopsis_color="3">
      <style basestylename="Action" leftindent="10"/>
      <marks><mark at="3" revision="1"/></marks>
      <text>She </text>
      <text bold="1">waits</text>
      <text>.</text>
    </para>
    <para>
      <style basestylename="Character" dualdialogue="1"/>
      <text>ANNA</text>
    </para>
    <para>
      <style basestylename="Dialogue"/>
      <text>Now.</text>
    </para>
    <para>
      <style basestylename="Character"/>
      <text>BEN</text>
    </para>
    <para>
      <style basestylename="Dialogue"/>
      <text>Now.</text>
    </para>
    <para>
      <style basestylename="Act Heading" pagebreakbefore="1"/>
      <text>ACT TWO</text>
    </para>
  </paragraphs>
  <titlepage>
    <para bookmark="Title">
      <style basestylename="Normal Text" align="center"/>
      <text>THE HUT</text>
    </para>
  </titlepage>
  <lists><revision_colors><revision_color name="Blue" index="1" color_name="Blue" color_index="1" mark="*"/></revision_colors></lists>
</document>
"##;

#[test]
fn reads_elements_notes_synopses_and_dual_dialogue() {
    let script = osf::parse(V30).unwrap();
    let kinds: Vec<K> = script.elements.iter().map(|e| e.kind).collect();
    assert_eq!(kinds, [K::SceneHeading, K::Action, K::Synopsis, K::Character, K::Dialogue, K::Character, K::Dialogue, K::PageBreak, K::SceneHeading]);
    let e = &script.elements;
    assert_eq!(e[0].scene_number.as_deref(), Some("4A"));
    assert_eq!(e[1].text, "She **waits**.[[check the date]]");
    assert_eq!(e[2].text, "They meet.");
    assert!(!e[3].dual && e[5].dual, "the second speaker is the dual one");
    assert_eq!(e[8].text, "ACT TWO", "a custom style imports as the kind it is based on");
    assert_eq!(script.title_page.get("Title"), Some("THE HUT"));
}

#[test]
fn keeps_what_it_does_not_model() {
    let script = osf::parse(V30).unwrap();
    let xml = osf::write(&script);
    for part in [
        r#"<info uuid="0b0c" pagecount="1"/>"#,
        r#"<fadein_settings saved_with="test"/>"#,
        r#"<para scene_number="4A" edited_by="AB">"#,
        r#"note="check the date" synopsis="They meet." synopsis_color="3""#,
        r#"<style basestylename="Action" leftindent="10"/>"#,
        r#"<marks><mark at="3" revision="1"/></marks>"#,
        r#"<style basestylename="Character" dualdialogue="1"/>"#,
        r#"<style basestylename="Act Heading" pagebreakbefore="1"/>"#,
        r#"<revision_color name="Blue" index="1" color_name="Blue" color_index="1" mark="*"/>"#,
    ] {
        assert!(xml.contains(part), "missing {part} in\n{xml}");
    }
    let again = osf::parse(&xml).unwrap();
    assert_eq!(again, script, "a second import is identical");
    assert_eq!(osf::write(&again), xml, "writing is stable");
}

#[test]
fn reads_and_keeps_every_version_spelling() {
    let v40 = V30.replace(r#"version="30""#, r#"version="40""#).replace("basestylename", "basestyle").replace("scene_number=", "number=");
    let script = osf::parse(&v40).unwrap();
    assert_eq!(script.elements[0].scene_number.as_deref(), Some("4A"));
    let xml = osf::write(&script);
    assert!(xml.contains(r#"<para number="4A" edited_by="AB">"#) && xml.contains(r#"<style basestyle="Character" dualdialogue="1"/>"#), "{xml}");

    let v21 = V30.replace(r#"version="30""#, r#"version="21""#).replace("basestylename", "baseStyleName").replace("scene_number=", "sceneNumber=");
    let script = osf::parse(&v21).unwrap();
    assert_eq!(script.elements[0].scene_number.as_deref(), Some("4A"));
    assert_eq!(script.elements[3].kind, K::Character);
}

#[test]
fn page_layout_comes_from_the_document() {
    let script = osf::parse(V30).unwrap();
    let layout = Layout::for_script(&script);
    // A4 (297 mm) with 1" margins: 70 rows, 58 body lines.
    assert_eq!((layout.page_rows, layout.body_top, layout.lines_per_page), (70, 6, 58));
    assert_eq!(layout.character.left, 35);
    assert_eq!(layout.scene_heading.space_before, 1);
    assert_eq!(layout.contd, "(cont'd)");
}

#[test]
fn rejects_foreign_or_hostile_xml() {
    assert!(osf::parse("<html/>").is_err());
    assert!(osf::parse("").is_err());
    assert!(osf::parse("<FinalDraft><Content/></FinalDraft>").is_err());
    let deep = format!("<document>{}{}</document>", "<a>".repeat(1000), "</a>".repeat(1000));
    assert!(osf::parse(&deep).is_err());
}
