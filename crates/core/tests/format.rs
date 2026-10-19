#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

//! Element settings and page layout in `.fdx` and Open Screenplay Format, and between them. The
//! documents are invented, shaped like the files the format notes describe.

use storyteller_core::layout::{Align, Layout};
use storyteller_core::model::Script;
use storyteller_core::{Format, fdx, osf};

const DOC: &str = r##"<?xml version="1.0" encoding="UTF-8" standalone="no" ?>
<FinalDraft DocumentType="Script" Template="No" Version="5">
  <Content>
    <Paragraph Type="Scene Heading">
      <Text>INT. BOATHOUSE - NIGHT</Text>
    </Paragraph>
    <Paragraph Type="New Act">
      <Text>ACT TWO</Text>
    </Paragraph>
    <Paragraph Type="Character">
      <Text>MARA</Text>
    </Paragraph>
    <Paragraph Type="Dialogue">
      <Text>Hold the rope.</Text>
    </Paragraph>
  </Content>
  <PageLayout BackgroundColor="#FFFFFFFFFFFF" BottomMargin="54" HeaderMargin="36" TopMargin="72" UsesSmartQuotes="Yes">
    <PageSize Height="11.69" Width="8.27"/>
  </PageLayout>
  <ElementSettings Type="Scene Heading">
    <FontSpec AdornmentStyle="0" Font="Courier Prime" Size="12" Style="AllCaps"/>
    <ParagraphSpec Alignment="Left" FirstIndent="0.00" Leading="Regular" LeftIndent="1.50" RightIndent="7.50" SpaceBefore="24" Spacing="1" StartsNewPage="No"/>
    <Behavior PaginateAs="Scene Heading" ReturnKey="Action" Shortcut="1"/>
  </ElementSettings>
  <ElementSettings Type="Character">
    <FontSpec Font="Courier Prime" Size="12" Style="AllCaps"/>
    <ParagraphSpec Alignment="Left" FirstIndent="0.00" LeftIndent="3.50" RightIndent="7.25" SpaceBefore="12" Spacing="1"/>
    <Behavior PaginateAs="Character" ReturnKey="Dialogue" Shortcut="3"/>
  </ElementSettings>
  <ElementSettings Type="Dialogue">
    <FontSpec Font="Courier Prime" Size="12" Style=""/>
    <ParagraphSpec Alignment="Left" FirstIndent="0.00" LeftIndent="2.50" RightIndent="6.00" SpaceBefore="0" Spacing="1"/>
    <Behavior PaginateAs="Dialogue" ReturnKey="Character" Shortcut="4"/>
  </ElementSettings>
  <ElementSettings Type="New Act">
    <FontSpec Font="Courier Prime" Size="12" Style="Bold+Underline+AllCaps"/>
    <ParagraphSpec Alignment="Center" FirstIndent="0.00" LeftIndent="1.50" RightIndent="7.50" SpaceBefore="12" Spacing="1" StartsNewPage="Yes"/>
    <Behavior PaginateAs="Scene Heading" ReturnKey="Scene Heading"/>
    <Outline CanHide="No" Level="1"/>
  </ElementSettings>
  <MoresAndContinueds>
    <FontSpec Font="Courier Prime" Size="12"/>
    <DialogueBreaks AutomaticCharacterContinueds="Yes" BottomOfPage="Yes" DialogueBottom="(MORE)" DialogueTop="(CONT'D)" TopOfNext="Yes"/>
    <SceneBreaks ContinuedNumber="No" SceneBottom="(CONTINUED)" SceneTop="CONTINUED:"/>
  </MoresAndContinueds>
</FinalDraft>
"##;

/// The format without the XML each format keeps for writing back.
fn model(script: &Script) -> Format {
    let mut format = script.format.clone();
    if let Some(page) = format.page.as_mut() {
        page.fdx_page_layout.clear();
        page.fdx_mores.clear();
        page.osf_settings.clear();
    }
    for element in &mut format.elements {
        element.fdx_xml.clear();
        element.osf_xml.clear();
    }
    format
}

#[test]
fn reads_element_settings_and_page_layout() {
    let script = fdx::parse(DOC).unwrap();
    let format = model(&script);
    let page = format.page.as_ref().unwrap();
    assert_eq!((page.width, page.height, page.top, page.bottom), (Some(8270), Some(11690), Some(1000), Some(750)));
    assert_eq!((page.more.as_deref(), page.contd.as_deref(), page.auto_contd), (Some("(MORE)"), Some("(CONT'D)"), Some(true)));
    let act = format.element("New Act").unwrap();
    assert!(act.bold && act.underline && act.all_caps && act.new_page);
    assert_eq!((act.align, act.left, act.right, act.space_before), (Some(Align::Center), Some(1500), Some(7500), Some(12)));
    assert_eq!(act.based_on.as_deref(), Some("Scene Heading"));
    let character = format.element("Character").unwrap();
    assert_eq!((character.left, character.right, character.enter.as_deref()), (Some(3500), Some(7250), Some("Dialogue")));
}

#[test]
fn fdx_to_fdx_keeps_the_sections_as_written() {
    let script = fdx::parse(DOC).unwrap();
    let xml = fdx::write(&script);
    for part in [
        r#"<Behavior PaginateAs="Scene Heading" ReturnKey="Action" Shortcut="1"/>"#,
        r#"<Outline CanHide="No" Level="1"/>"#,
        r#"SceneTop="CONTINUED:""#,
    ] {
        assert!(xml.contains(part), "missing {part} in\n{xml}");
    }
    let again = fdx::parse(&xml).unwrap();
    assert_eq!(again.format, script.format);
    assert_eq!(fdx::write(&again), xml, "writing is stable");
}

#[test]
fn fdx_to_osf_to_fdx_keeps_the_format_and_the_pages() {
    let script = fdx::parse(DOC).unwrap();
    let layout = Layout::for_script(&script);
    let xml = osf::write(&script);
    assert!(xml.contains(r#"page_width="2101" page_height="2969""#), "A4 in tenths of a millimetre: {xml}");
    assert!(
        xml.contains(r#"<style name="New Act" basestylename="Scene Heading" font="Courier Prime" size="12" bold="1" underline="1" allcaps="1""#),
        "{xml}"
    );
    let via_osf = osf::parse(&xml).unwrap();
    assert_eq!(model(&via_osf), model(&script));
    assert_eq!(Layout::for_script(&via_osf), layout, "the OSF file paginates the same");
    assert_eq!(osf::write(&via_osf), xml, "writing OSF is stable");

    let back = fdx::parse(&fdx::write(&via_osf)).unwrap();
    assert_eq!(model(&back), model(&script));
    assert_eq!(Layout::for_script(&back), layout);
    let written = fdx::write(&back);
    for part in [r#"Shortcut="1""#, r#"<Outline CanHide="No" Level="1"/>"#, r#"SceneTop="CONTINUED:""#, r#"HeaderMargin="36""#] {
        assert!(written.contains(part), "what OSF cannot hold comes back: {part} in\n{written}");
    }
}

const OSF: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<document type="Open Screenplay Format document" version="30">
  <settings page_width="2159" page_height="2794" margin_top="254" margin_bottom="254" margin_left="381" margin_right="254" cont_text="cont'd" more_text="(more)" dialogue_continues="false"/>
  <styles>
    <style name="Normal Text" builtin="1" builtin_index="0" font="Courier Prime" size="12"/>
    <style name="Action" builtin="1" builtin_index="2" basestylename="Normal Text" spacebefore="1.0"/>
    <style name="Character" builtin="1" builtin_index="3" basestylename="Normal Text" style_enter="Dialogue" spacebefore="1.0" keepwithnext="1" leftindent="508" rightindent="64" allcaps="1"/>
    <style name="Chorus" basestylename="Action" italic="1" align="center"/>
    <header_style basestylename="Normal Text" font="Courier Prime"/>
  </styles>
  <paragraphs>
    <para>
      <style basestylename="Action"/>
      <text>The tide turns.</text>
    </para>
  </paragraphs>
</document>
"#;

#[test]
fn osf_to_fdx_to_osf_keeps_the_format_and_the_pages() {
    let script = osf::parse(OSF).unwrap();
    let format = model(&script);
    assert_eq!(format.page.as_ref().unwrap().contd.as_deref(), Some("(cont'd)"));
    let character = format.element("Character").unwrap();
    assert_eq!((character.left, character.right), (Some(3500), Some(7248)));
    let layout = Layout::for_script(&script);

    let xml = fdx::write(&script);
    assert!(
        xml.contains(r#"<ElementSettings Type="Chorus"><FontSpec Font="Courier Prime" Size="12" Style="Italic"/>"#)
            || xml.contains(r#"Type="Chorus""#),
        "{xml}"
    );
    let via_fdx = fdx::parse(&xml).unwrap();
    assert_eq!(model(&via_fdx), format);
    assert_eq!(Layout::for_script(&via_fdx), layout, "the .fdx file paginates the same");

    let back = osf::write(&via_fdx);
    assert!(!back.contains("fdx_"), "nothing but what the styles say: {back}");
    for part in [r#"keepwithnext="1""#, r#"cont_text="cont&apos;d""#] {
        assert!(back.contains(part), "what .fdx cannot hold comes back: {part} in\n{back}");
    }
    let again = osf::parse(&back).unwrap();
    assert_eq!(model(&again), format);
    assert_eq!(Layout::for_script(&again), layout);
}

#[test]
fn a_changed_style_is_written_from_the_model() {
    let mut script = fdx::parse(DOC).unwrap();
    let dialogue = script.format.elements.iter_mut().find(|e| e.name == "Dialogue").unwrap();
    dialogue.left = Some(2600);
    dialogue.italic = true;
    let xml = fdx::write(&script);
    assert!(xml.contains(r#"LeftIndent="2.60""#) && xml.contains(r#"Style="Italic""#), "{xml}");
    assert!(xml.contains(r#"<Behavior PaginateAs="Scene Heading" ReturnKey="Action" Shortcut="1"/>"#), "other settings stay: {xml}");
    let back = fdx::parse(&xml).unwrap();
    assert_eq!(model(&back), model(&script));
    assert_eq!(Layout::for_script(&back).dialogue.left, 26);
}
