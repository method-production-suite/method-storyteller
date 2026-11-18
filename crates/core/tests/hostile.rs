#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

//! Hostile input never panics: random bytes, odd markup, huge lines and deep nesting.

use storyteller_core::layout::Layout;
use storyteller_core::model::{Element, ElementKind, Script};
use storyteller_core::paginate::paginate;
use storyteller_core::{CoreError, autocomplete, decode_text, fdx, fountain, inline};

fn rng(mut seed: u64) -> impl FnMut(u64) -> u64 {
    move |n| {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed % n.max(1)
    }
}

#[test]
fn random_text_never_panics() {
    let alphabet: Vec<char> = "ab AB.!@#=~><()[]*_\\/\n\n\n  ^:-INTEXT".chars().chain(['é', '日', '\u{0}', '\t', '\r']).collect();
    let mut next = rng(42);
    for _ in 0..500 {
        let len = next(400) as usize;
        let text: String = (0..len).map(|_| alphabet[next(alphabet.len() as u64) as usize]).collect();
        let script = fountain::parse(&text).unwrap();
        let _ = fountain::write(&script);
        let _ = paginate(&script, &Layout::default());
        let _ = fdx::parse(&text);
        let _ = fdx::parse(&fdx::write(&script)).unwrap();
        let _ = inline::parse(&text);
        for kind in [ElementKind::Character, ElementKind::SceneHeading, ElementKind::Transition] {
            let _ = autocomplete::suggest(&script, kind, &text);
        }
    }
}

#[test]
fn random_bytes_are_rejected_cleanly() {
    let mut next = rng(7);
    for _ in 0..200 {
        let bytes: Vec<u8> = (0..next(300)).map(|_| next(256) as u8).collect();
        if let Ok(text) = decode_text(&bytes) {
            let _ = fountain::parse(text);
            let _ = fdx::parse(text);
        }
    }
}

#[test]
fn oversize_input_is_refused() {
    let big = "a".repeat(storyteller_core::MAX_INPUT_BYTES + 1);
    assert_eq!(fountain::parse(&big), Err(CoreError::TooLarge));
    assert!(matches!(fdx::parse(&big), Err(CoreError::TooLarge)));
    assert_eq!(decode_text(big.as_bytes()), Err(CoreError::TooLarge));
}

#[test]
fn huge_unbreakable_lines_paginate() {
    let script = Script {
        elements: vec![
            Element::new(ElementKind::Action, "x".repeat(200_000)),
            Element::new(ElementKind::Character, "A".repeat(500)),
            Element::new(ElementKind::Dialogue, "y".repeat(50_000)),
            Element::new(ElementKind::Parenthetical, "(".repeat(10_000)),
        ],
        ..Script::default()
    };
    let paginated = paginate(&script, &Layout::default());
    assert!(paginated.pages.len() > 50);
}

#[test]
fn degenerate_layouts_do_not_hang() {
    let script = fountain::parse("INT. A - DAY\n\nText. More text. Even more.\n\nBOB\nHi. Hello. Hey.\n").unwrap();
    for lines in [0, 1, 2, 3] {
        let layout = Layout { lines_per_page: lines, ..Layout::default() };
        let _ = paginate(&script, &layout);
    }
    let mut narrow = Layout::default();
    narrow.dialogue.width = 0;
    narrow.action.width = 1;
    let _ = paginate(&script, &narrow);
}

#[test]
fn a_huge_autocomplete_list_opens_quickly_and_is_capped() {
    // A file can name far more characters than we keep; we keep the first `MAX_LIST` and read the
    // rest in one pass. The limit is wide enough for a busy machine and still far below what
    // checking each name against all before it would cost.
    let mut script = Script::default();
    script.elements.push(Element::new(ElementKind::Action, "One."));
    script.lists.characters = (0..50_000).map(|n| format!("NAME {n}")).collect();
    script.spelling_ignored = (0..50_000).map(|n| format!("word{n}")).collect();
    for (format, text) in [("fdx", fdx::write(&script)), ("osf", storyteller_core::osf::write(&script))] {
        let start = std::time::Instant::now();
        let back = if format == "fdx" { fdx::parse(&text) } else { storyteller_core::osf::parse(&text) }.unwrap();
        let took = start.elapsed();
        assert_eq!(back.lists.characters.len(), autocomplete::MAX_LIST, "{format}");
        assert_eq!(back.lists.characters.first().map(String::as_str), Some("NAME 0"), "{format}");
        assert_eq!(back.spelling_ignored.len(), 50_000, "{format}: every distinct ignored word is kept");
        assert!(took < std::time::Duration::from_secs(60), "{format} took {took:?}");
    }
}

#[test]
fn learning_a_script_of_repeated_cues_and_brackets_is_quick() {
    // Each list one short of its cap, so every repeat would scan all of it if each were compared.
    let mut script = Script::default();
    for n in 0..autocomplete::MAX_LIST - 1 {
        script.elements.push(Element::new(ElementKind::Character, format!("NAME {n}")));
    }
    for _ in script.elements.len()..storyteller_core::MAX_ELEMENTS {
        script.elements.push(Element::new(ElementKind::Character, "NAME 1998"));
    }
    let mut cue = String::from("NAME 1998");
    for n in 0..autocomplete::MAX_LIST - 1 {
        cue.push_str(&format!(" (E{n})"));
    }
    cue.push_str(&" (E1998)".repeat(1_000_000));
    script.elements.push(Element::new(ElementKind::Character, cue));

    let mut lists = autocomplete::Lists::default();
    let start = std::time::Instant::now();
    assert!(lists.learn(&script));
    let took = start.elapsed();
    assert_eq!(lists.characters.len(), autocomplete::MAX_LIST - 1);
    assert_eq!(lists.extensions.len(), autocomplete::MAX_LIST - 1);
    assert!(took < std::time::Duration::from_secs(60), "took {took:?}");
}

/// A Final Draft document with `extra` after the script, where Final Draft keeps its own
/// sections (`ElementSettings`, `Revisions`, and those of later versions we don't model).
fn fdx_with(extra: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="no" ?>
<FinalDraft DocumentType="Script" Template="No" Version="6">
  <Content>
    <Paragraph Type="Scene Heading"><Text>INT. STATION - NIGHT</Text></Paragraph>
    <Paragraph Type="Action"><Text>The radio hums.</Text></Paragraph>
  </Content>
{extra}
</FinalDraft>
"#
    )
}

#[test]
fn an_unknown_section_nested_too_deeply_is_refused() {
    let depth = 300;
    let section = format!("  <Future>{}{}</Future>", "<Level>".repeat(depth), "</Level>".repeat(depth));
    match fdx::parse(&fdx_with(&section)) {
        Err(CoreError::Xml(message)) => assert!(message.contains("nested too deeply"), "{message}"),
        other => panic!("expected the depth limit, got {other:?}"),
    }
}

#[test]
fn a_large_unknown_section_comes_back_byte_for_byte() {
    // About 3 MB of a section we don't model, with attributes, entities, text and nesting.
    let mut section = String::from("  <FutureData Kind=\"beats\">\n");
    let mut n = 0;
    while section.len() < 3 * 1024 * 1024 {
        section.push_str(&format!(
            "    <Beat ID=\"{n}\" Colour=\"#FF{:04X}\"><Note Author=\"A&amp;B\">Beat {n} &lt;keep&gt; as it is</Note><Empty/></Beat>\n",
            n % 0xFFFF
        ));
        n += 1;
    }
    section.push_str("  </FutureData>");
    let doc = fdx_with(&section);
    assert!(doc.len() < storyteller_core::MAX_INPUT_BYTES, "under the import limit");
    let script = fdx::parse(&doc).unwrap();
    assert_eq!(script.elements.len(), 2);
    let written = fdx::write(&script);
    assert!(written.contains(section.trim_start()), "the section is written back unchanged");
    assert_eq!(fdx::write(&fdx::parse(&written).unwrap()), written, "and stays so");
}

#[test]
fn an_unclosed_tag_inside_element_settings_is_refused() {
    let section = r#"  <ElementSettings Type="Action"><FontSpec Style="Bold"><ParagraphSpec Alignment="Left"></ElementSettings>"#;
    match fdx::parse(&fdx_with(section)) {
        Err(CoreError::Xml(_)) => {}
        other => panic!("expected damaged XML, got {other:?}"),
    }
}
