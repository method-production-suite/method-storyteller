#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

//! The words spell check ignores in a script, in `.fdx` and Open Screenplay Format and between
//! them. The documents are invented, shaped like the files the format notes describe.

use storyteller_core::{fdx, fountain, osf};

const DOC: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="no" ?>
<FinalDraft DocumentType="Script" Template="No" Version="5">
  <Content>
    <Paragraph Type="Action">
      <Text>Mara fixes the Kestrel.</Text>
    </Paragraph>
  </Content>
  <SpellCheckIgnoreLists>
    <IgnoredRanges>
      <Range End="4" Start="0"/>
    </IgnoredRanges>
    <IgnoredWords>
      <Word>Kestrel</Word>
      <Word>Mara</Word>
      <Word>kestrel</Word>
    </IgnoredWords>
  </SpellCheckIgnoreLists>
</FinalDraft>
"#;

const OSF: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<document type="Open Screenplay Format document" version="30">
  <paragraphs>
    <para>
      <style basestylename="Action"/>
      <text>Mara fixes the Kestrel.</text>
    </para>
  </paragraphs>
  <spelling language="en_US"><user_dictionary><entry word="Tannoy"/></user_dictionary><ignored><entry word="Kestrel"/><entry word="Mara"/></ignored></spelling>
</document>
"#;

#[test]
fn reads_the_ignored_words_in_lower_case_once_each() {
    assert_eq!(fdx::parse(DOC).unwrap().spelling_ignored, ["kestrel", "mara"]);
    assert_eq!(osf::parse(OSF).unwrap().spelling_ignored, ["kestrel", "mara"], "not the user dictionary");
}

#[test]
fn an_unchanged_list_goes_back_as_written() {
    let xml = fdx::write(&fdx::parse(DOC).unwrap());
    assert!(xml.contains("<Word>Kestrel</Word>") && xml.contains(r#"<Range End="4" Start="0"/>"#), "{xml}");
    let xml = osf::write(&osf::parse(OSF).unwrap());
    assert!(xml.contains(r#"<entry word="Kestrel"/>"#) && xml.contains(r#"<user_dictionary><entry word="Tannoy"/>"#), "{xml}");
}

#[test]
fn a_changed_list_is_rewritten_and_the_rest_stays() {
    let mut script = fdx::parse(DOC).unwrap();
    script.spelling_ignored.push("tannoy".into());
    let xml = fdx::write(&script);
    assert!(xml.contains("<IgnoredWords><Word>kestrel</Word><Word>mara</Word><Word>tannoy</Word></IgnoredWords>"), "{xml}");
    assert!(xml.contains(r#"<Range End="4" Start="0"/>"#), "ignored ranges stay: {xml}");
    assert_eq!(fdx::parse(&xml).unwrap().spelling_ignored, script.spelling_ignored);

    let mut script = osf::parse(OSF).unwrap();
    script.spelling_ignored.retain(|w| w != "mara");
    let xml = osf::write(&script);
    assert!(xml.contains(r#"<user_dictionary><entry word="Tannoy"/></user_dictionary><ignored><entry word="kestrel"/></ignored>"#), "{xml}");
    assert_eq!(osf::parse(&xml).unwrap().spelling_ignored, script.spelling_ignored);
}

#[test]
fn the_list_crosses_between_the_formats() {
    let script = fdx::parse(DOC).unwrap();
    let via_osf = osf::parse(&osf::write(&script)).unwrap();
    assert_eq!(via_osf.spelling_ignored, script.spelling_ignored);
    assert_eq!(fdx::parse(&fdx::write(&via_osf)).unwrap().spelling_ignored, script.spelling_ignored);

    let script = osf::parse(OSF).unwrap();
    let via_fdx = fdx::parse(&fdx::write(&script)).unwrap();
    assert_eq!(via_fdx.spelling_ignored, script.spelling_ignored);
    assert_eq!(osf::parse(&osf::write(&via_fdx)).unwrap().spelling_ignored, script.spelling_ignored);
}

#[test]
fn a_new_script_gets_a_list_only_when_it_has_words() {
    let mut script = fountain::parse("Mara waits.\n").unwrap();
    assert!(!fdx::write(&script).contains("SpellCheckIgnoreLists") && !osf::write(&script).contains("<spelling"));
    script.spelling_ignored = vec!["mara".into()];
    assert_eq!(fdx::parse(&fdx::write(&script)).unwrap().spelling_ignored, ["mara"]);
    assert_eq!(osf::parse(&osf::write(&script)).unwrap().spelling_ignored, ["mara"]);
    // An emptied list leaves an empty one where the file had words.
    let mut script = fdx::parse(DOC).unwrap();
    script.spelling_ignored.clear();
    assert!(fdx::write(&script).contains("<IgnoredWords/>"));
}
