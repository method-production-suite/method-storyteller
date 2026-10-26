#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

//! Element settings and page layout in `.fdx` and Open Screenplay Format, and between them. The
//! documents are invented, shaped like the files the format notes describe.

use storyteller_core::format::INCH;
use storyteller_core::layout::{Align, Layout};
use storyteller_core::model::Script;
use storyteller_core::{Format, fdx, osf};

/// Thousandths of an inch in EMU.
fn m(mils: i64) -> i64 {
    mils * INCH / 1000
}

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
    assert_eq!((page.width, page.height, page.top, page.bottom), (Some(m(8270)), Some(m(11690)), Some(m(1000)), Some(m(750))));
    assert_eq!((page.more.as_deref(), page.contd.as_deref(), page.auto_contd), (Some("(MORE)"), Some("(CONT'D)"), Some(true)));
    let act = format.element("New Act").unwrap();
    assert!(act.bold && act.underline && act.all_caps && act.new_page);
    assert_eq!((act.align, act.left, act.right, act.space_before), (Some(Align::Center), Some(m(1500)), Some(m(7500)), Some(12)));
    assert_eq!(act.based_on.as_deref(), Some("Scene Heading"));
    let character = format.element("Character").unwrap();
    assert_eq!((character.left, character.right, character.enter.as_deref()), (Some(m(3500)), Some(m(7250)), Some("Dialogue")));
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
    assert_eq!((character.left, character.right), (Some(m(3500)), Some(m(7500) - 64 * 3600)));
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
    dialogue.left = Some(m(2600));
    dialogue.italic = true;
    let xml = fdx::write(&script);
    assert!(xml.contains(r#"LeftIndent="2.60""#) && xml.contains(r#"Style="Italic""#), "{xml}");
    assert!(xml.contains(r#"<Behavior PaginateAs="Scene Heading" ReturnKey="Action" Shortcut="1"/>"#), "other settings stay: {xml}");
    let back = fdx::parse(&xml).unwrap();
    assert_eq!(model(&back), model(&script));
    assert_eq!(Layout::for_script(&back).dialogue.left, 26);
}

/// A document with only the given settings sections.
fn with_sections(sections: &str) -> Script {
    fdx::parse(&format!(
        "<?xml version=\"1.0\"?><FinalDraft DocumentType=\"Script\" Template=\"No\" Version=\"5\"><Content><Paragraph Type=\"Action\"><Text>A.</Text></Paragraph></Content>{sections}</FinalDraft>"
    ))
    .unwrap()
}

#[test]
fn a_new_osf_document_paginates_like_the_defaults() {
    let text = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../examples/field-station.fountain")).unwrap();
    let script = storyteller_core::fountain::parse(&text).unwrap();
    let via_osf = osf::parse(&osf::write(&script)).unwrap();
    assert_eq!(Layout::for_script(&via_osf), Layout::default(), "our own styles are the default layout, Lyrics included");
    let pages = |s: &Script| storyteller_core::paginate::paginate(s, &Layout::for_script(s)).page_count();
    assert_eq!(pages(&via_osf), pages(&script));
}

#[test]
fn layouts_follow_the_file_exactly() {
    let default = Layout::default();
    // One edge only: the type keeps its default width.
    let layout = Layout::for_script(&with_sections(r#"<ElementSettings Type="Dialogue"><ParagraphSpec LeftIndent="2.60"/></ElementSettings>"#));
    assert_eq!((layout.dialogue.left, layout.dialogue.width), (default.dialogue.left, default.dialogue.width));
    // 78 points is six and a half lines, which rounds up.
    let layout = Layout::for_script(&with_sections(r#"<PageLayout TopMargin="78"/>"#));
    assert_eq!((layout.body_top, layout.lines_per_page), (7, 53));
    // A bottom margin alone still sets the lines on a page.
    let layout = Layout::for_script(&with_sections(r#"<PageLayout BottomMargin="144"/>"#));
    assert_eq!((layout.body_top, layout.lines_per_page), (6, 48));
    // A bad height does not stop a good width.
    let layout = Layout::for_script(&with_sections(r#"<PageLayout><PageSize Height="99" Width="8.27"/></PageLayout>"#));
    assert_eq!((layout.page_rows, layout.page_columns), (66, 83));
    // Absurd indents are refused, not applied.
    let script = with_sections(r#"<ElementSettings Type="Action"><ParagraphSpec LeftIndent="999" RightIndent="7.50"/></ElementSettings>"#);
    assert_eq!(script.format.elements[0].left, None);
    assert_eq!(Layout::for_script(&script).action, default.action);
}

#[test]
fn an_unnamed_style_stays_where_it_was() {
    let doc = OSF.replace(r#"<header_style"#, r#"<style basestylename="Action" italic="1"/><header_style"#);
    let mut script = osf::parse(&doc).unwrap();
    script.format.elements[1].italic = true;
    let xml = osf::write(&script);
    assert!(xml.contains(r#"<style basestylename="Action" italic="1"/>"#), "{xml}");
}

/// Scene numbers in one margin only: read from `SceneNumberOptions`, kept through OSF, printed on
/// that side alone (docs/production-spec.md §3.1).
#[test]
fn scene_number_margins_travel_and_print() {
    use storyteller_core::model::{Element, ElementKind};
    use storyteller_core::paginate::{LineRole, paginate};
    let options = r#"<SceneNumberOptions LeftLocation="0.75" NumberScheme="1A" RightLocation="7.38" ShowNumbersOnLeft="No" ShowNumbersOnRight="Yes"><FontSpec Font="Courier Final Draft" Size="12"/></SceneNumberOptions>"#;
    let script = with_sections(options);
    assert_eq!(script.scene_number_margins(), (false, true));
    // Unchanged: the section is written back as it was.
    assert!(fdx::write(&script).contains(options));
    // Through OSF and back, the same.
    let back = fdx::parse(&fdx::write(&osf::parse(&osf::write(&script)).unwrap())).unwrap();
    assert_eq!(back.scene_number_margins(), (false, true));
    assert!(fdx::write(&back).contains("NumberScheme=\"1A\""));
    // Changed: written from the model, the rest of the section kept.
    let mut changed = script.clone();
    assert!(changed.set_scene_number_margins(true, false));
    assert!(!changed.set_scene_number_margins(true, false));
    let written = fdx::write(&changed);
    assert!(
        written.contains("ShowNumbersOnLeft=\"Yes\"") && written.contains("ShowNumbersOnRight=\"No\"") && written.contains("NumberScheme=\"1A\""),
        "{written}"
    );
    let osf_text = osf::write(&changed);
    assert!(osf_text.contains("method_storyteller_scene_numbers=\"left\""), "{osf_text}");
    assert_eq!(osf::parse(&osf_text).unwrap().scene_number_margins(), (true, false));
    // A script that never said keeps both, and writes no section.
    let plain = with_sections("");
    assert_eq!(plain.scene_number_margins(), (true, true));
    assert!(!fdx::write(&plain).contains("SceneNumberOptions"));
    assert!(!osf::write(&plain).contains("method_storyteller_scene_numbers"));
    // On the page: only the right-hand number.
    let mut heading = Element::new(ElementKind::SceneHeading, "INT. ROOM - DAY");
    heading.scene_number = Some("7".into());
    let mut page_script = Script { elements: vec![heading], ..Script::default() };
    let sides = |s: &Script| {
        let pages = paginate(s, &Layout::default());
        pages.pages[0].lines.iter().filter(|l| l.role == LineRole::SceneNumber).map(|l| l.col).collect::<Vec<_>>()
    };
    assert_eq!(sides(&page_script).len(), 2);
    page_script.set_scene_number_margins(false, true);
    assert_eq!(sides(&page_script), [Layout::default().scene_number_right]);
}
