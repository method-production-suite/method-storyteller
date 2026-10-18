//! Find and replace over the script's elements.
//!
//! Matching is on each element's text as stored (with its Fountain inline markup), one element
//! at a time, so a match never spans two paragraphs. Offsets are UTF-8 byte offsets.

use storyteller_core::{ElementKind, Script};

/// What to look for.
#[derive(Clone, Debug, Default)]
pub struct Query {
    pub text: String,
    /// Match case exactly; otherwise letters match in either case.
    pub case: bool,
    /// Only whole words: no letter, digit or apostrophe on either side, except that a
    /// possessive `'s` may follow, so `NORA` is found in `Nora's` but `DON` is not in `don't`.
    pub whole_word: bool,
    /// Only elements of this type.
    pub kind: Option<ElementKind>,
}

/// One match: element index and byte range.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Hit {
    pub element: usize,
    pub start: usize,
    pub end: usize,
}

fn is_apostrophe(c: char) -> bool {
    c == '\'' || c == '’'
}

fn is_word(c: char) -> bool {
    c.is_alphanumeric() || is_apostrophe(c)
}

/// Whether the text after a match ends the word: no word character, or a possessive `'s`.
fn ends_word(rest: &[(usize, char)]) -> bool {
    match rest {
        [] => true,
        [(_, c), ..] if !is_word(*c) => true,
        [(_, a), (_, s), tail @ ..] if is_apostrophe(*a) && (*s == 's' || *s == 'S') => tail.first().is_none_or(|(_, c)| !is_word(*c)),
        _ => false,
    }
}

fn same(a: char, b: char, case: bool) -> bool {
    a == b || (!case && a.to_lowercase().eq(b.to_lowercase()))
}

/// Every match of `query` in one text, left to right, not overlapping.
pub fn find_in(text: &str, query: &Query) -> Vec<(usize, usize)> {
    let wanted: Vec<char> = query.text.chars().collect();
    if wanted.is_empty() {
        return Vec::new();
    }
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let mut hits = Vec::new();
    let mut i = 0;
    while i + wanted.len() <= chars.len() {
        let matched = chars.get(i..i + wanted.len()).is_some_and(|run| run.iter().zip(&wanted).all(|((_, a), b)| same(*a, *b, query.case)));
        let start = chars.get(i).map_or(text.len(), |c| c.0);
        let end = chars.get(i + wanted.len()).map_or(text.len(), |c| c.0);
        let bounded = !query.whole_word
            || (i.checked_sub(1).and_then(|j| chars.get(j)).is_none_or(|c| !is_word(c.1)) && ends_word(chars.get(i + wanted.len()..).unwrap_or(&[])));
        if matched && bounded {
            hits.push((start, end));
            i += wanted.len();
        } else {
            i += 1;
        }
    }
    hits
}

/// Every match in the script, in reading order.
pub fn find_all(script: &Script, query: &Query) -> Vec<Hit> {
    script
        .elements
        .iter()
        .enumerate()
        .filter(|(_, e)| query.kind.is_none_or(|k| e.kind == k))
        .flat_map(|(element, e)| find_in(&e.text, query).into_iter().map(move |(start, end)| Hit { element, start, end }))
        .collect()
}

/// The next match after (or, backwards, before) a position, wrapping around the script.
/// Returns the hit and its 0-based position among all hits.
pub fn find_next(hits: &[Hit], element: usize, offset: usize, backwards: bool) -> Option<(usize, Hit)> {
    if backwards {
        let before = hits.iter().rposition(|h| (h.element, h.start) < (element, offset));
        before.or_else(|| hits.len().checked_sub(1)).and_then(|i| hits.get(i).map(|h| (i, *h)))
    } else {
        let after = hits.iter().position(|h| (h.element, h.start) >= (element, offset));
        after.or(if hits.is_empty() { None } else { Some(0) }).and_then(|i| hits.get(i).map(|h| (i, *h)))
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::indexing_slicing)]
    use super::*;

    fn q(text: &str) -> Query {
        Query { text: text.into(), ..Query::default() }
    }

    #[test]
    fn case_and_whole_words() {
        assert_eq!(find_in("Nora, NORA and nora's", &q("nora")), vec![(0, 4), (6, 10), (15, 19)]);
        assert_eq!(find_in("Nora, NORA", &Query { case: true, ..q("NORA") }), vec![(6, 10)]);
        let whole = Query { whole_word: true, ..q("cut") };
        assert_eq!(find_in("cut, cutting, the cut's end, cut", &whole), vec![(0, 3), (18, 21), (29, 32)]);
        // Only a possessive 's ends a word at an apostrophe, straight or curly.
        let nora = Query { whole_word: true, ..q("nora") };
        assert_eq!(find_in("Nora's hat, Nora’s coat, Nora'd go", &nora), vec![(0, 4), (12, 16)]);
        assert_eq!(find_in("O'Brien and O’Brien", &Query { whole_word: true, ..q("o") }), vec![]);
        assert_eq!(find_in("don't, can’t", &Query { whole_word: true, ..q("don") }), vec![]);
        assert_eq!(find_in("Nora's", &Query { whole_word: true, ..q("nora's") }), vec![(0, 6)]);
    }

    #[test]
    fn unicode_offsets_are_bytes() {
        assert_eq!(find_in("café Café", &q("CAFÉ")), vec![(0, 5), (6, 11)]);
        assert_eq!(find_in("", &q("a")), vec![]);
        assert_eq!(find_in("abc", &q("")), vec![]);
    }

    #[test]
    fn next_wraps_both_ways() {
        let hits = [Hit { element: 1, start: 0, end: 1 }, Hit { element: 3, start: 2, end: 3 }];
        assert_eq!(find_next(&hits, 0, 0, false).map(|h| h.0), Some(0));
        assert_eq!(find_next(&hits, 1, 1, false).map(|h| h.0), Some(1));
        assert_eq!(find_next(&hits, 4, 0, false).map(|h| h.0), Some(0));
        assert_eq!(find_next(&hits, 3, 2, true).map(|h| h.0), Some(0));
        assert_eq!(find_next(&hits, 0, 0, true).map(|h| h.0), Some(1));
        assert_eq!(find_next(&[], 0, 0, false), None);
    }
}
