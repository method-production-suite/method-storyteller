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

/// The endings of possessives and contractions: a word ending in one is right when its stem is.
const CONTRACTION_ENDINGS: [&str; 5] = ["'s", "'ll", "'d", "'re", "'ve"];

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

/// The writer's own dictionary: words added with Learn, spelled right in every script (the app
/// keeps them in its settings). Words ignored for one script live on the script itself
/// ([`Script::spelling_ignored`]).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Personal {
    pub learned: HashSet<String>,
}

impl Personal {
    fn knows(&self, word: &str, ignored: &HashSet<String>) -> bool {
        let lower = word.to_lowercase();
        self.learned.contains(&lower) || ignored.contains(&lower)
    }
}

/// A word with the accents taken off its letters (`café` → `cafe`): the dictionary is built
/// without accents.
fn unaccented(word: &str) -> String {
    word.chars()
        .map(|c| match c {
            'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' => 'a',
            'À' | 'Á' | 'Â' | 'Ã' | 'Ä' | 'Å' => 'A',
            'ç' => 'c',
            'Ç' => 'C',
            'è' | 'é' | 'ê' | 'ë' => 'e',
            'È' | 'É' | 'Ê' | 'Ë' => 'E',
            'ì' | 'í' | 'î' | 'ï' => 'i',
            'Ì' | 'Í' | 'Î' | 'Ï' => 'I',
            'ñ' => 'n',
            'Ñ' => 'N',
            'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'ø' => 'o',
            'Ò' | 'Ó' | 'Ô' | 'Õ' | 'Ö' | 'Ø' => 'O',
            'ù' | 'ú' | 'û' | 'ü' => 'u',
            'Ù' | 'Ú' | 'Û' | 'Ü' => 'U',
            'ý' | 'ÿ' => 'y',
            'Ý' => 'Y',
            other => other,
        })
        .collect()
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
        // JEAN-LUC is known as a whole and as JEAN and LUC, since words split at a hyphen.
        .flat_map(|e| {
            let name = e.character_name().to_uppercase();
            name.split(|c: char| c.is_whitespace() || c == '-').map(str::to_string).chain(std::iter::once(name.clone())).collect::<Vec<_>>()
        })
        .filter(|n| !n.is_empty())
        .collect()
}

/// The script's ignored words in lower case, to look up any case. They keep the case they were
/// written in on the script; build this once per pass, not once per word.
pub fn ignored_set(ignored: &[String]) -> HashSet<String> {
    ignored.iter().map(|w| w.to_lowercase()).collect()
}

/// Whether one word is spelled right. `ignored` is [`ignored_set`] of the script's ignored words.
pub fn is_right(word: &str, names: &HashSet<String>, personal: &Personal, ignored: &HashSet<String>) -> bool {
    let word = word.replace('’', "'");
    if word.chars().any(|c| c.is_ascii_digit()) || word.chars().count() < 2 {
        return true;
    }
    let upper = word.to_uppercase();
    let stem = upper.strip_suffix("'S").unwrap_or(&upper);
    if names.contains(stem) || SCREENPLAY_TERMS.contains(&stem) || personal.knows(&word, ignored) {
        return true;
    }
    let Some(dictionary) = dictionary() else { return true };
    let plain = unaccented(&word);
    if dictionary.check(&word) || (plain != word && dictionary.check(&plain)) {
        return true;
    }
    // A possessive or a contraction of a right word ("else's", "there'll", "whoever's") is right,
    // and so is a known name's ("Ansel's", when Ansel is ignored or a cue).
    let lower = word.to_lowercase();
    for ending in CONTRACTION_ENDINGS {
        if let Some(stem) = lower.strip_suffix(ending).and_then(|_| word.get(..word.len() - ending.len()))
            && stem.chars().count() >= 2
            && is_right(stem, names, personal, ignored)
        {
            return true;
        }
    }
    let word = plain;
    let upper = word.to_uppercase();
    // A word in capitals (a heading, a cue, a first appearance) is right if its usual case is.
    if word == upper {
        let mut chars = word.chars();
        let title: String = chars.next().into_iter().chain(chars.flat_map(char::to_lowercase)).collect();
        return dictionary.check(&title) || dictionary.check(&word.to_lowercase());
    }
    false
}

/// The misspelled words of one text, as byte ranges: what the editor underlines.
pub fn misspelled_in(text: &str, names: &HashSet<String>, personal: &Personal, ignored: &HashSet<String>) -> Vec<(usize, usize)> {
    words(text).into_iter().filter(|&(start, end)| text.get(start..end).is_some_and(|word| !is_right(word, names, personal, ignored))).collect()
}

/// Every misspelled word of the script, in reading order.
pub fn misspelled(script: &Script, personal: &Personal) -> Vec<Miss> {
    let names = names(script);
    let ignored = ignored_set(&script.spelling_ignored);
    let mut out = Vec::new();
    for (index, element) in script.elements.iter().enumerate() {
        // Page breaks and outline lines (sections, synopses) are not script text.
        if !element.kind.is_printed() {
            continue;
        }
        for (start, end) in words(&element.text) {
            let Some(word) = element.text.get(start..end) else { continue };
            if !is_right(word, &names, personal, &ignored) {
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
    fn a_hyphenated_cue_names_both_halves() {
        let script = storyteller_core::fountain::parse("INT. A - DAY\n\nJEAN-LUC\nHello.\n\nLuc waves.\n").unwrap();
        assert!(misspelled(&script, &Personal::default()).is_empty(), "{:?}", misspelled(&script, &Personal::default()));
    }

    #[test]
    fn the_dictionary_loads_and_checks() {
        let none = HashSet::new();
        let personal = Personal::default();
        let is_right = |w: &str, n: &HashSet<String>, p: &Personal| is_right(w, n, p, &HashSet::new());
        assert!(is_right("glass", &none, &personal));
        assert!(is_right("GLASS", &none, &personal), "a word in capitals");
        assert!(is_right("don’t", &none, &personal), "a curly apostrophe");
        assert!(!is_right("glsas", &none, &personal));
        assert!(is_right("INT", &none, &personal), "a screenplay term");
        assert!(is_right("12A", &none, &personal), "numbers are not checked");
        let names: HashSet<String> = ["NORA".to_string()].into_iter().collect();
        assert!(is_right("Nora's", &names, &personal), "a character's name, possessive");
        assert!(is_right("café", &none, &personal), "accents are checked without them");
        assert!(is_right("CAFÉ", &none, &personal));
        assert!(is_right("else's", &none, &personal) && is_right("there'll", &none, &personal), "contractions of right words");
        assert!(is_right("whoever's", &none, &personal) && is_right("THEM'S", &none, &personal));
        assert!(!is_right("glsas's", &none, &personal), "but not of a misspelling");
        assert!(suggestions("glsas", 5).iter().any(|s| s == "glass"));
        assert!(suggestions("GLSAS", 5).iter().any(|s| s == "GLASS"), "suggestions in the word's case");
    }
}
