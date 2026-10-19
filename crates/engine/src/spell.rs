//! Spell check: the bundled English (US) Hunspell dictionary from SCOWL, read by `spellbook`
//! (assets/dictionaries, attributed in ATTRIBUTION.md).
//!
//! A word counts as spelled right when the dictionary knows it, when it is a character's name
//! (from the script's cues), a screenplay term (INT., CONT'D, V.O. …), or a word the writer
//! added or ignored. Words with digits, and text inside `[[notes]]`, are not checked. Offsets
//! are UTF-8 bytes into an element's text, as everywhere in the engine.

use std::collections::HashSet;
use std::sync::OnceLock;

use storyteller_core::{ElementKind, Script};

const AFF: &str = include_str!("../../../assets/dictionaries/en_US.aff");
const DIC: &str = include_str!("../../../assets/dictionaries/en_US.dic");

/// Words a screenplay uses that a general dictionary does not, in upper case.
const SCREENPLAY_TERMS: [&str; 22] = [
    "INT", "EXT", "I/E", "CONT'D", "CONT", "CONTD", "V.O", "O.S", "O.C", "POV", "INSERT", "INTERCUT", "MONTAGE", "SUPER", "FADE", "DISSOLVE",
    "SMASH", "MATCH", "PRELAP", "MOS", "OMITTED", "B.G",
];

/// The English dictionary, parsed once (about a tenth of a second) and shared.
pub fn dictionary() -> Option<&'static spellbook::Dictionary> {
    static DICTIONARY: OnceLock<Option<spellbook::Dictionary>> = OnceLock::new();
    DICTIONARY.get_or_init(|| spellbook::Dictionary::new(AFF, DIC).ok()).as_ref()
}

/// The writer's own words and the words ignored for this session, kept beside the dictionary.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Personal {
    /// Added with "Learn": spelled right everywhere (the app keeps them in its settings).
    pub learned: HashSet<String>,
    /// Ignored for this session.
    pub ignored: HashSet<String>,
}

impl Personal {
    fn knows(&self, word: &str) -> bool {
        let lower = word.to_lowercase();
        self.learned.contains(&lower) || self.ignored.contains(&lower)
    }
}

/// One misspelled word: element, byte range, the word.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Miss {
    pub element: usize,
    pub start: usize,
    pub end: usize,
    pub word: String,
}

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '\'' || c == '’'
}

/// The words of a text with their byte ranges, skipping `[[notes]]`. An apostrophe inside a
/// word belongs to it; one at either end does not.
pub fn words(text: &str) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut in_note = false;
    let mut start: Option<usize> = None;
    let mut chars = text.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        if !in_note && c == '[' && text.get(i..).is_some_and(|r| r.starts_with("[[")) {
            if let Some(s) = start.take() {
                out.push(trim_apostrophes(text, s, i));
            }
            in_note = true;
            chars.next();
            continue;
        }
        if in_note {
            if c == ']' && text.get(i..).is_some_and(|r| r.starts_with("]]")) {
                in_note = false;
                chars.next();
            }
            continue;
        }
        if is_word_char(c) {
            start.get_or_insert(i);
        } else if let Some(s) = start.take() {
            out.push(trim_apostrophes(text, s, i));
        }
    }
    if let Some(s) = start {
        out.push(trim_apostrophes(text, s, text.len()));
    }
    out.into_iter().filter(|(s, e)| s < e).collect()
}

fn trim_apostrophes(text: &str, mut start: usize, mut end: usize) -> (usize, usize) {
    while let Some(c) = text.get(start..end).and_then(|w| w.chars().next()).filter(|c| *c == '\'' || *c == '’') {
        start += c.len_utf8();
    }
    while let Some(c) = text.get(start..end).and_then(|w| w.chars().last()).filter(|c| *c == '\'' || *c == '’') {
        end -= c.len_utf8();
    }
    (start, end)
}

/// The character names the script's cues use, upper case, so `NORA` and `Nora's` are known.
pub fn names(script: &Script) -> HashSet<String> {
    script
        .elements
        .iter()
        .filter(|e| e.kind == ElementKind::Character)
        .flat_map(|e| e.character_name().split_whitespace().map(str::to_uppercase).collect::<Vec<_>>())
        .collect()
}

/// Whether one word is spelled right.
pub fn is_right(word: &str, names: &HashSet<String>, personal: &Personal) -> bool {
    let word = word.replace('’', "'");
    if word.chars().any(|c| c.is_ascii_digit()) || word.chars().count() < 2 {
        return true;
    }
    let upper = word.to_uppercase();
    let stem = upper.strip_suffix("'S").unwrap_or(&upper);
    if names.contains(stem) || SCREENPLAY_TERMS.contains(&stem) || personal.knows(&word) {
        return true;
    }
    let Some(dictionary) = dictionary() else { return true };
    if dictionary.check(&word) {
        return true;
    }
    // A word in capitals (a heading, a cue, a first appearance) is right if its usual case is.
    if word == upper {
        let mut chars = word.chars();
        let title: String = chars.next().into_iter().chain(chars.flat_map(char::to_lowercase)).collect();
        return dictionary.check(&title) || dictionary.check(&word.to_lowercase());
    }
    false
}

/// Every misspelled word of the script, in reading order.
pub fn misspelled(script: &Script, personal: &Personal) -> Vec<Miss> {
    let names = names(script);
    let mut out = Vec::new();
    for (index, element) in script.elements.iter().enumerate() {
        // Page breaks and outline lines are not script text.
        if matches!(element.kind, ElementKind::PageBreak) {
            continue;
        }
        for (start, end) in words(&element.text) {
            let Some(word) = element.text.get(start..end) else { continue };
            if !is_right(word, &names, personal) {
                out.push(Miss { element: index, start, end, word: word.to_string() });
            }
        }
    }
    out
}

/// Up to `limit` suggestions for a misspelled word, in the word's own case.
pub fn suggestions(word: &str, limit: usize) -> Vec<String> {
    let Some(dictionary) = dictionary() else { return Vec::new() };
    let mut out = Vec::new();
    dictionary.suggest(&word.replace('’', "'"), &mut out);
    let caps = word.chars().any(char::is_alphabetic) && word == word.to_uppercase();
    out.into_iter().map(|s| if caps { s.to_uppercase() } else { s }).take(limit).collect()
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::indexing_slicing)]
    use super::*;

    #[test]
    fn words_skip_notes_and_keep_inner_apostrophes() {
        let text = "Nora's 'quoted' word [[a note]] don’t";
        let found: Vec<&str> = words(text).into_iter().map(|(s, e)| &text[s..e]).collect();
        assert_eq!(found, ["Nora's", "quoted", "word", "don’t"]);
    }

    #[test]
    fn the_dictionary_loads_and_checks() {
        let none = HashSet::new();
        let personal = Personal::default();
        assert!(is_right("glass", &none, &personal));
        assert!(is_right("GLASS", &none, &personal), "a word in capitals");
        assert!(is_right("don’t", &none, &personal), "a curly apostrophe");
        assert!(!is_right("glsas", &none, &personal));
        assert!(is_right("INT", &none, &personal), "a screenplay term");
        assert!(is_right("12A", &none, &personal), "numbers are not checked");
        let names: HashSet<String> = ["NORA".to_string()].into_iter().collect();
        assert!(is_right("Nora's", &names, &personal), "a character's name, possessive");
        assert!(suggestions("glsas", 5).iter().any(|s| s == "glass"));
        assert!(suggestions("GLSAS", 5).iter().any(|s| s == "GLASS"), "suggestions in the word's case");
    }
}
