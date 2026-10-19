//! Final Draft–style element flow (`docs/spec.md` §4.1), as data.

use storyteller_core::ElementKind as K;

/// One row of the flow table (Final Draft's documented defaults, docs/spec.md §4.1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FlowRule {
    pub kind: K,
    /// Element started by Enter after a non-empty element. Enter on an empty one opens the
    /// element picker instead.
    pub enter: K,
    /// What Tab turns an empty element into.
    pub tab_empty: K,
    /// Element Tab starts after a non-empty element.
    pub tab_next: K,
    pub shift_tab: K,
}

const fn rule(kind: K, enter: K, tab_empty: K, tab_next: K, shift_tab: K) -> FlowRule {
    FlowRule { kind, enter, tab_empty, tab_next, shift_tab }
}

/// The default flow table.
pub const RULES: [FlowRule; 9] = [
    rule(K::SceneHeading, K::Action, K::Action, K::Action, K::Action),
    rule(K::Action, K::Action, K::Character, K::Character, K::SceneHeading),
    rule(K::Character, K::Dialogue, K::Transition, K::Parenthetical, K::Action),
    rule(K::Parenthetical, K::Dialogue, K::Dialogue, K::Dialogue, K::Dialogue),
    rule(K::Dialogue, K::Action, K::Parenthetical, K::Parenthetical, K::Character),
    rule(K::Transition, K::SceneHeading, K::SceneHeading, K::SceneHeading, K::Action),
    // A new act opens on a scene; the end of an act is followed by the next act.
    rule(K::NewAct, K::SceneHeading, K::SceneHeading, K::SceneHeading, K::Action),
    rule(K::EndOfAct, K::NewAct, K::NewAct, K::NewAct, K::Action),
    rule(K::Shot, K::Action, K::Action, K::Action, K::Action),
];

/// The rule for a kind; kinds without one behave like action.
pub fn rule_for(kind: K) -> FlowRule {
    RULES.iter().copied().find(|r| r.kind == kind).unwrap_or(FlowRule {
        kind,
        enter: K::Action,
        tab_empty: K::Character,
        tab_next: K::Character,
        shift_tab: K::SceneHeading,
    })
}

/// Whether an element counts as empty for Enter and Tab (a parenthetical's own brackets do not count).
pub fn is_empty(kind: K, text: &str) -> bool {
    let text = text.trim();
    text.is_empty() || (kind == K::Parenthetical && text == "()")
}

/// The status-bar hint for an element: what Tab and Enter will do.
pub fn hint(kind: K, text: &str) -> String {
    let rule = rule_for(kind);
    if is_empty(kind, text) {
        format!("Tab: {}    Enter: choose an element", rule.tab_empty.label())
    } else {
        format!("Tab: {}    Enter: {}", rule.tab_next.label(), rule.enter.label())
    }
}

/// The element picker: what Enter on an empty element offers, with each choice's key.
pub const PICKER: [(char, K); 11] = [
    ('S', K::SceneHeading),
    ('A', K::Action),
    ('C', K::Character),
    ('P', K::Parenthetical),
    ('D', K::Dialogue),
    ('T', K::Transition),
    ('H', K::Shot),
    ('N', K::Centered),
    ('L', K::Lyrics),
    ('B', K::NewAct),
    ('E', K::EndOfAct),
];

/// Element types reachable by ⌘1–⌘7.
pub const SHORTCUT_KINDS: [K; 7] = [K::SceneHeading, K::Action, K::Character, K::Parenthetical, K::Dialogue, K::Transition, K::Shot];

/// Re-shape text when an element changes kind: parentheticals gain or lose their brackets.
/// Returns the new text and how far the caret moves (in bytes, may be negative).
pub fn convert_text(text: &str, from: K, to: K) -> (String, isize) {
    if from == to {
        return (text.to_string(), 0);
    }
    if to == K::Parenthetical {
        let trimmed = text.trim();
        if trimmed.starts_with('(') {
            return (text.to_string(), 0);
        }
        return (format!("({trimmed})"), 1);
    }
    if from == K::Parenthetical {
        let trimmed = text.trim();
        if let Some(inner) = trimmed.strip_prefix('(') {
            let inner = inner.strip_suffix(')').unwrap_or(inner);
            return (inner.to_string(), -1);
        }
    }
    let text = if to.is_caps() { text.to_uppercase() } else { text.to_string() };
    (text, 0)
}

/// Whether an edit of action text from `old` to `new` turns it into a scene heading: the line
/// held nothing yet, or only the start of `int.`/`ext.`/`i/e` (not the whole of one), and now
/// opens with one of them.
/// So several keys arriving in one frame, or a whole heading pasted into an empty line
/// (`EXT. RIDGE - DAWN`), still make a heading, while existing action that starts that way
/// is left alone, and so is `Est. 1962`.
pub fn becomes_heading(old: &str, new: &str) -> bool {
    const INTROS: [&str; 3] = ["int.", "ext.", "i/e"];
    let old = old.trim_start().to_lowercase();
    let new = new.trim_start().to_lowercase();
    // Only the start of an intro: action that already opens with one (edited, or a heading
    // turned back into action) stays action.
    let unfinished = old.trim_end().is_empty() || INTROS.iter().any(|intro| intro.len() > old.len() && intro.starts_with(old.as_str()));
    let opens = INTROS.iter().any(|intro| new.strip_prefix(intro).is_some_and(|rest| *intro != "i/e" || rest.is_empty() || rest.starts_with(' ')));
    unfinished && opens
}

/// Smart quotes: when `new` is `old` with one straight quote typed before byte `caret`, the
/// text with that quote curled (opening after a space, bracket, dash or the start; closing or
/// an apostrophe otherwise) and the caret after it.
pub fn smart_quote(old: &str, new: &str, caret: usize) -> Option<(String, usize)> {
    if new.len() != old.len() + 1 || caret == 0 || !new.is_char_boundary(caret) {
        return None;
    }
    let typed = new.get(caret - 1..caret)?;
    if typed != "\"" && typed != "'" {
        return None;
    }
    let before = new.get(..caret - 1)?;
    if format!("{before}{}", new.get(caret..)?) != old {
        return None;
    }
    let opening = before.chars().next_back().is_none_or(|c| c.is_whitespace() || matches!(c, '(' | '[' | '{' | '\u{2014}' | '\u{2013}' | '-' | '/'));
    let curly = match (typed, opening) {
        ("\"", true) => '\u{201C}',
        ("\"", false) => '\u{201D}',
        (_, true) => '\u{2018}',
        _ => '\u{2019}',
    };
    let text = format!("{before}{curly}{}", new.get(caret..)?);
    Some((text, caret - 1 + curly.len_utf8()))
}

/// Words that start with an apostrophe standing for missing letters ('til, 'em, 'cause …).
const ELISIONS: [&str; 9] = ["til", "em", "cause", "round", "tis", "twas", "n", "bout", "nother"];

/// A leading apostrophe typed as an opening quote, put right: `‘90s` → `’90s` as soon as the digit
/// is typed, and `‘til` → `’til` once the word ends. Returns the corrected text (same length).
pub fn fix_leading_apostrophe(text: &str, caret: usize) -> Option<String> {
    let before = text.get(..caret)?;
    let last = before.chars().next_back()?;
    let swap = |at: usize| Some(format!("{}\u{2019}{}", text.get(..at)?, text.get(at + '\u{2018}'.len_utf8()..)?));
    let opening_at = |end: usize| {
        let head = before.get(..end)?;
        let quote = head.strip_suffix('\u{2018}')?;
        quote.chars().next_back().is_none_or(char::is_whitespace).then_some(quote.len())
    };
    if last.is_ascii_digit() {
        return opening_at(before.len() - last.len_utf8()).and_then(swap);
    }
    if last.is_alphanumeric() {
        return None;
    }
    let word_end = before.len() - last.len_utf8();
    let head = before.get(..word_end)?;
    let word: String = head.chars().rev().take_while(|c| c.is_alphabetic()).collect::<Vec<_>>().into_iter().rev().collect();
    if word.is_empty() || !ELISIONS.contains(&word.to_lowercase().as_str()) {
        return None;
    }
    opening_at(word_end - word.len()).and_then(swap)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn curls_quotes_as_they_are_typed() {
        assert_eq!(smart_quote("", "\"", 1), Some(("\u{201C}".into(), 3)));
        assert_eq!(smart_quote("\u{201C}Hi", "\u{201C}Hi\"", 6), Some(("\u{201C}Hi\u{201D}".into(), 8)));
        assert_eq!(smart_quote("It", "It'", 3), Some(("It\u{2019}".into(), 5)));
        assert_eq!(smart_quote("say (", "say ('", 6), Some(("say (\u{2018}".into(), 8)));
        assert_eq!(smart_quote("a", "ab", 2), None, "only quotes");
        assert_eq!(smart_quote("", "''", 2), None, "one character at a time");
    }

    #[test]
    fn leading_apostrophes_point_the_right_way() {
        assert_eq!(fix_leading_apostrophe("the \u{2018}9", 8).as_deref(), Some("the \u{2019}9"));
        assert_eq!(fix_leading_apostrophe("wait \u{2018}til ", 12).as_deref(), Some("wait \u{2019}til "));
        assert_eq!(fix_leading_apostrophe("\u{2018}tilde ", 9), None, "not an elision");
        assert_eq!(fix_leading_apostrophe("say \u{2018}hi ", 10), None, "an opening quote stays one");
    }

    #[test]
    fn a_heading_typed_fast_or_pasted_into_an_empty_line_becomes_one() {
        assert!(becomes_heading("int", "int."), "typed a key at a time");
        assert!(becomes_heading("", "EXT. RIDGE - DAWN"), "pasted, or keys arriving together");
        assert!(becomes_heading("E", "EXT. RIDGE"));
        assert!(becomes_heading("", "INT./EXT. CAR - DAY"));
        assert!(becomes_heading("", "I/E CAR - DAY"));
        assert!(!becomes_heading("", "Est. 1962, says the sign."), "EST stays action");
        assert!(!becomes_heading("", "Interior design."));
        assert!(!becomes_heading("", "I/Eat"));
        assert!(!becomes_heading("She drives.", "Ext. of the car: she drives."), "existing action is left alone");
        assert!(!becomes_heading("Ext. of the car", "Ext. of the car:"), "action already opening with an intro, typed on");
        assert!(!becomes_heading("INT. HUT - DAY", "INT. HUT - DAY."), "a heading turned back into action stays action");
        assert!(!becomes_heading("int.", "int. "), "once action, the next key does not promote it");
    }

    #[test]
    fn hints_say_what_tab_and_enter_do() {
        assert_eq!(hint(K::Dialogue, "Hello."), "Tab: Parenthetical    Enter: Action");
        assert_eq!(hint(K::Action, ""), "Tab: Character    Enter: choose an element");
        assert_eq!(hint(K::Parenthetical, "()"), "Tab: Dialogue    Enter: choose an element");
    }
}
