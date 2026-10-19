#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

//! Script notes in all three `.fdx` forms, and through Open Screenplay Format and back. The
//! document is invented, shaped like the files the format notes describe.

use storyteller_core::model::{Element, ElementKind as K, Script, ScriptNote};
use storyteller_core::{fdx, fountain, osf};

/// Offsets in the whole script: `INT. FIELD STATION - NIGHT` is 0..26, the action 27..48,
/// `MARA` 49..53 and the line 54..66 (each paragraph break counts one).
const DOC: &str = r##"<?xml version="1.0" encoding="UTF-8" standalone="no" ?>
<FinalDraft DocumentType="Script" Template="No" Version="5">
  <Content>
    <Paragraph Type="Scene Heading">
      <Text>INT. FIELD STATION - NIGHT</Text>
    </Paragraph>
    <Paragraph Type="Action">
      <ScriptNote Color="#FFFF00000000" DateModified="1700000100" DateTime="1700000000" Name="" Type="General"><Paragraph Alignment="Left" LeftIndent="0.00"><Text Font="Courier Final Draft" Size="12">Too dark?</Text></Paragraph></ScriptNote>
      <Text>The generator </Text>
      <ScriptNote Author="R. Lee" Color="#0000FFFF0000"><Paragraph><Text>Name the model.</Text></Paragraph><Paragraph><Text>Or not.</Text></Paragraph></ScriptNote>
      <Text>coughs.</Text>
    </Paragraph>
    <Paragraph Type="Character">
      <Text>MARA</Text>
    </Paragraph>
    <Paragraph Type="Dialogue">
      <Text>Not tonight.</Text>
    </Paragraph>
  </Content>
  <ScriptNoteDefinitions Active="1"><ScriptNoteDefinition Color="#FFFF00000000" ID="1" Marker="!" Name="General"/></ScriptNoteDefinitions>
  <UnanchoredScriptNotes><ScriptNote Author="R. Lee" Color="#FFFFFFFF0000"><Paragraph><Text>Check the title.</Text></Paragraph></ScriptNote></UnanchoredScriptNotes>
  <ScriptNotes>
    <ScriptNote Color="#0000FFFFFFFF" DateModified="1700000300" DateTime="1700000200" Id="n1" Name="" Range="31,40" Type="General" WriterID="w1" WriterName="A. Writer"><Paragraph><Text>Which one?</Text></Paragraph></ScriptNote>
    <ScriptNote Color="#FFFF0000FFFF" DateModified="1700000500" DateTime="1700000400" Id="n2" Name="" Range="49,66" Type="General" WriterID="w1" WriterName="A. Writer"><Paragraph><Text>Too soon for this.</Text></Paragraph></ScriptNote>
  </ScriptNotes>
</FinalDraft>
"##;

/// What the model holds of a note, without the Final Draft data: its paragraph, text, range,
/// colour, author and whether it is unanchored.
type Note = (usize, String, Option<(u32, u32)>, Option<String>, Option<String>, bool);

fn notes(script: &Script) -> Vec<Note> {
    let mut out = Vec::new();
    for (index, element) in script.elements.iter().enumerate() {
        for n in &element.notes {
            out.push((index, n.text.clone(), n.range, n.color.clone(), n.author.clone(), n.unanchored));
        }
    }
    out
}

/// Every note's Final Draft attributes, in order.
fn fdx_attributes(script: &Script) -> Vec<Vec<(String, String)>> {
    script.elements.iter().flat_map(|e| &e.notes).map(|n| n.fdx.as_ref().map(|f| f.attributes.clone()).unwrap_or_default()).collect()
}

fn s(text: &str) -> Option<String> {
    Some(text.to_string())
}

#[test]
fn reads_all_three_forms_onto_their_paragraphs() {
    let script = fdx::parse(DOC).unwrap();
    assert_eq!(script.elements[1].text, "The generator coughs.", "notes are not text");
    assert_eq!(
        notes(&script),
        [
            (0, "Check the title.".into(), None, s("#FFFF00"), s("R. Lee"), true),
            (1, "Too dark?".into(), None, s("#FF0000"), None, false),
            (1, "Name the model.\nOr not.".into(), Some((14, 14)), s("#00FF00"), s("R. Lee"), false),
            (1, "Which one?".into(), Some((4, 13)), s("#00FFFF"), s("A. Writer"), false),
            (2, "Too soon for this.".into(), Some((0, 17)), s("#FF00FF"), s("A. Writer"), false),
        ]
    );
}

#[test]
fn fdx_to_fdx_keeps_every_note_as_written() {
    let script = fdx::parse(DOC).unwrap();
    let xml = fdx::write(&script);
    for part in [
        r##"<ScriptNote Color="#FFFF00000000" DateModified="1700000100" DateTime="1700000000" Name="" Type="General"><Paragraph Alignment="Left" LeftIndent="0.00"><Text Font="Courier Final Draft" Size="12">Too dark?</Text></Paragraph></ScriptNote>"##,
        "      <Text>The generator </Text>\n      <ScriptNote Author=\"R. Lee\"",
        "</ScriptNote>\n      <Text>coughs.</Text>",
        r##"<ScriptNoteDefinitions Active="1">"##,
        r##"<UnanchoredScriptNotes><ScriptNote Author="R. Lee" Color="#FFFFFFFF0000">"##,
        r#"Id="n1" Name="" Range="31,40""#,
        r#"Id="n2" Name="" Range="49,66""#,
    ] {
        assert!(xml.contains(part), "missing {part} in\n{xml}");
    }
    assert_eq!(xml.matches("<ScriptNotes>").count(), 1);
    let again = fdx::parse(&xml).unwrap();
    assert_eq!(notes(&again), notes(&script));
    assert_eq!(again.fdx, script.fdx, "the same document data");
    assert_eq!(fdx::write(&again), xml, "writing is stable");
}

#[test]
fn fdx_to_osf_to_fdx_keeps_every_note() {
    let script = fdx::parse(DOC).unwrap();
    let xml = osf::write(&script);
    // Other apps read the plain note.
    assert!(xml.contains(r#"<para note="Too dark? / Name the model.&#10;Or not. / Which one?">"#), "{xml}");
    assert!(xml.contains(r##"<method_storyteller_note text="Check the title." color="#FFFF00" author="R. Lee" unanchored="1""##), "{xml}");
    let via_osf = osf::parse(&xml).unwrap();
    assert_eq!(notes(&via_osf), notes(&script), "OSF keeps range, colour and author");
    assert_eq!(osf::write(&via_osf), xml, "writing OSF is stable");

    let back = fdx::parse(&fdx::write(&via_osf)).unwrap();
    assert_eq!(notes(&back), notes(&script));
    assert_eq!(fdx_attributes(&back), fdx_attributes(&script), "and Final Draft's own attributes");
    let written = fdx::write(&back);
    assert!(written.contains(r#"Id="n1" Name="" Range="31,40""#) && written.contains(r#"Range="49,66""#), "{written}");
}

#[test]
fn osf_notes_reach_fdx_and_come_back() {
    let source = r##"<?xml version="1.0" encoding="UTF-8"?>
<document type="Open Screenplay Format document" version="30">
  <paragraphs>
    <para note="Louder.">
      <style basestylename="Action"/>
      <text>The radio hisses.</text>
    </para>
    <para>
      <style basestylename="Action"/>
      <method_storyteller_note text="Cut this?" range="4,9" color="#336699" author="R. Lee"/>
      <text>Mara waits.</text>
    </para>
  </paragraphs>
</document>
"##;
    let script = osf::parse(source).unwrap();
    assert_eq!(script.elements[0].text, "The radio hisses.[[Louder.]]", "a plain note stays a plain note");
    assert_eq!(notes(&script), [(1, "Cut this?".into(), Some((4, 9)), s("#336699"), s("R. Lee"), false)]);

    let xml = fdx::write(&script);
    assert!(xml.contains(r#"<ScriptNote StorytellerInline="Yes"><Paragraph><Text>Louder.</Text></Paragraph></ScriptNote>"#), "{xml}");
    assert!(xml.contains(r##"<ScriptNote Color="#333366669999" Range="22,27" WriterName="R. Lee">"##), "{xml}");
    let via_fdx = fdx::parse(&xml).unwrap();
    let back = osf::parse(&osf::write(&via_fdx)).unwrap();
    let texts = |s: &Script| s.elements.iter().map(|e| e.text.clone()).collect::<Vec<_>>();
    assert_eq!(texts(&back), texts(&script));
    assert_eq!(notes(&back), notes(&script));
    assert!(!osf::write(&back).contains("fdx_"), "nothing but what the note says");
}

#[test]
fn plain_notes_stay_where_they_sit_through_fdx() {
    let script = fountain::parse("The lamp [[a softer one?]]flickers.\n").unwrap();
    let xml = fdx::write(&script);
    assert!(xml.contains("<Text>The lamp </Text>\n      <ScriptNote StorytellerInline=\"Yes\">"), "{xml}");
    let back = fdx::parse(&xml).unwrap();
    assert_eq!(back.elements[0].text, "The lamp [[a softer one?]]flickers.");
    assert!(back.elements[0].notes.is_empty());
}

#[test]
fn edited_notes_are_written_from_the_model() {
    let mut script = fdx::parse(DOC).unwrap();
    // Text added before the notes moves their place in the script.
    script.elements[0].text = "INT. FIELD STATION - LATER NIGHT".into();
    script.elements[1].notes[1].text = "Name it.".into();
    script.elements[1].notes[1].color = None;
    let xml = fdx::write(&script);
    assert!(xml.contains(r#"Id="n1" Name="" Range="37,46""#) && xml.contains(r#"Range="55,72""#), "{xml}");
    assert!(xml.contains(r#"<ScriptNote Author="R. Lee"><Paragraph><Text>Name it.</Text></Paragraph></ScriptNote>"#), "{xml}");
    let back = fdx::parse(&xml).unwrap();
    assert_eq!(notes(&back), notes(&script));
}

#[test]
fn new_notes_pick_their_form() {
    let mut script = fountain::parse("INT. HUT - DAY\n\nThe door sticks.\n").unwrap();
    script.elements[1].notes.push(ScriptNote { text: "Whole line.".into(), ..ScriptNote::default() });
    script.elements[1].notes.push(ScriptNote { text: "Just the door.".into(), range: Some((4, 8)), color: s("#FF0000"), ..ScriptNote::default() });
    script.elements[0].notes.push(ScriptNote { text: "About the draft.".into(), unanchored: true, ..ScriptNote::default() });
    let xml = fdx::write(&script);
    assert!(xml.contains("<ScriptNote><Paragraph><Text>Whole line.</Text></Paragraph></ScriptNote>\n      <Text>The door sticks.</Text>"), "{xml}");
    assert!(
        xml.contains(
            r##"<ScriptNotes>
    <ScriptNote Color="#FFFF00000000" Range="19,23"><Paragraph><Text>Just the door.</Text></Paragraph></ScriptNote>
  </ScriptNotes>"##
        ),
        "{xml}"
    );
    assert!(xml.contains("<UnanchoredScriptNotes>\n    <ScriptNote><Paragraph><Text>About the draft.</Text></Paragraph></ScriptNote>"), "{xml}");
    let back = fdx::parse(&xml).unwrap();
    assert_eq!(notes(&back), notes(&script));
    assert_eq!(fdx::write(&back), xml, "writing is stable");
}

#[test]
fn notes_in_dual_dialogue_keep_their_place() {
    let mut script = Script {
        elements: vec![
            Element::new(K::Character, "ANNA"),
            Element::new(K::Dialogue, "Now."),
            Element::new(K::Character, "BEN").with_dual(true),
            Element::new(K::Dialogue, "Later."),
            Element::new(K::Action, "They part."),
        ],
        ..Script::default()
    };
    script.elements[3].notes.push(ScriptNote { text: "Softer.".into(), range: Some((0, 5)), ..ScriptNote::default() });
    script.elements[4].notes.push(ScriptNote { text: "Slower.".into(), range: Some((5, 9)), ..ScriptNote::default() });
    let back = fdx::parse(&fdx::write(&script)).unwrap();
    assert_eq!(notes(&back), notes(&script));
}

#[test]
fn hostile_note_lists_do_not_break_the_import() {
    let doc = DOC.replace(r#"Range="31,40""#, r#"Range="-4,x""#).replace(r#"Range="49,66""#, r#"Range="999999,3""#);
    let script = fdx::parse(&doc).unwrap();
    assert_eq!(script.elements.iter().map(|e| e.notes.len()).sum::<usize>(), 5);
    assert!(fdx::parse(&fdx::write(&script)).is_ok());
}
