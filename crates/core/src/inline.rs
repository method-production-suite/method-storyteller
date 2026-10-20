//! Fountain inline markup: `*italic*`, `**bold**`, `***bold italic***`, `_underline_`,
//! `[[notes]]` and backslash escapes.
//!
//! Element text keeps the markup verbatim; [`parse`] turns it into styled runs for printing
//! and FDX, and [`write`] turns styled runs back into markup.

use serde::{Deserialize, Serialize};

/// Character styling.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Style {
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
}

/// A run of text with one style.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Run {
    pub text: String,
    #[serde(default)]
    pub style: Style,
    /// A `[[note]]`: kept in the document, not printed.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub note: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Marker {
    Italic,
    Bold,
    BoldItalic,
    Underline,
}

impl Marker {
    fn raw(self) -> &'static str {
        match self {
            Marker::Italic => "*",
            Marker::Bold => "**",
            Marker::BoldItalic => "***",
            Marker::Underline => "_",
        }
    }
}

enum Token {
    Text(String),
    Note(String),
    Marker(Marker),
}

fn tokenize(source: &str) -> Vec<Token> {
    let mut tokens = Vec::new();
    let mut text = String::new();
    let mut chars = source.chars().peekable();
    let flush = |text: &mut String, tokens: &mut Vec<Token>| {
        if !text.is_empty() {
            tokens.push(Token::Text(std::mem::take(text)));
        }
    };
    while let Some(c) = chars.next() {
        match c {
            '\\' => match chars.next() {
                Some(next) => text.push(next),
                None => text.push('\\'),
            },
            '[' if chars.peek() == Some(&'[') => {
                chars.next();
                let mut note = String::new();
                let mut closed = false;
                while let Some(n) = chars.next() {
                    if n == ']' && chars.peek() == Some(&']') {
                        chars.next();
                        closed = true;
                        break;
                    }
                    note.push(n);
                }
                if closed {
                    flush(&mut text, &mut tokens);
                    tokens.push(Token::Note(note));
                } else {
                    text.push_str("[[");
                    text.push_str(&note);
                }
            }
            '*' => {
                let mut count = 1;
                while count < 3 && chars.peek() == Some(&'*') {
                    chars.next();
                    count += 1;
                }
                flush(&mut text, &mut tokens);
                tokens.push(Token::Marker(match count {
                    1 => Marker::Italic,
                    2 => Marker::Bold,
                    _ => Marker::BoldItalic,
                }));
            }
            '_' => {
                flush(&mut text, &mut tokens);
                tokens.push(Token::Marker(Marker::Underline));
            }
            other => text.push(other),
        }
    }
    flush(&mut text, &mut tokens);
    tokens
}

/// Parse markup into runs. Unmatched markers stay literal. Adjacent runs with the same style
/// are merged.
pub fn parse(source: &str) -> Vec<Run> {
    let tokens = tokenize(source);
    // Pair markers: a marker closes the most recent open marker of the same kind.
    let mut matched = vec![false; tokens.len()];
    let mut open: Vec<(usize, Marker)> = Vec::new();
    for (index, token) in tokens.iter().enumerate() {
        if let Token::Marker(marker) = token {
            if let Some(position) = open.iter().rposition(|(_, kind)| kind == marker) {
                let (start, _) = open.remove(position);
                if let Some(slot) = matched.get_mut(start) {
                    *slot = true;
                }
                if let Some(slot) = matched.get_mut(index) {
                    *slot = true;
                }
            } else {
                open.push((index, *marker));
            }
        }
    }
    let mut runs: Vec<Run> = Vec::new();
    let mut style = Style::default();
    let mut active = [false; 4];
    let push = |runs: &mut Vec<Run>, text: &str, style: Style, note: bool| {
        if text.is_empty() {
            return;
        }
        if let Some(last) = runs.last_mut()
            && last.style == style
            && last.note == note
        {
            last.text.push_str(text);
            return;
        }
        runs.push(Run { text: text.to_string(), style, note });
    };
    for (index, token) in tokens.iter().enumerate() {
        match token {
            Token::Text(text) => push(&mut runs, text, style, false),
            Token::Note(text) => push(&mut runs, text, style, true),
            Token::Marker(marker) => {
                if !matched.get(index).copied().unwrap_or(false) {
                    push(&mut runs, marker.raw(), style, false);
                    continue;
                }
                let slot = match marker {
                    Marker::Italic => 0,
                    Marker::Bold => 1,
                    Marker::BoldItalic => 2,
                    Marker::Underline => 3,
                };
                if let Some(flag) = active.get_mut(slot) {
                    *flag = !*flag;
                }
                let [italic, bold, bold_italic, underline] = active;
                style = Style { bold: bold || bold_italic, italic: italic || bold_italic, underline };
            }
        }
    }
    runs
}

/// Plain printable text (styles and notes dropped).
pub fn plain(source: &str) -> String {
    parse(source).into_iter().filter(|run| !run.note).map(|run| run.text).collect()
}

fn escape(text: &str, out: &mut String) {
    for c in text.chars() {
        if matches!(c, '*' | '_' | '\\') {
            out.push('\\');
        }
        out.push(c);
    }
}

/// Write runs as markup. `parse(&write(runs))` returns the same runs when no two
/// differently-styled runs abut without a separating character of their own.
pub fn write(runs: &[Run]) -> String {
    let mut out = String::new();
    for run in runs {
        if run.note {
            out.push_str("[[");
            out.push_str(&run.text.replace("]]", "] ]"));
            out.push_str("]]");
            continue;
        }
        let star = match (run.style.bold, run.style.italic) {
            (true, true) => "***",
            (true, false) => "**",
            (false, true) => "*",
            (false, false) => "",
        };
        if run.style.underline {
            out.push('_');
        }
        out.push_str(star);
        let mut escaped = String::new();
        escape(&run.text, &mut escaped);
        out.push_str(&escaped);
        out.push_str(star);
        if run.style.underline {
            out.push('_');
        }
    }
    out
}

/// Which style a span of source text shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpanKind {
    /// Printed text in a style.
    Text(Style),
    /// Markup: a matched marker or an escaping backslash.
    Markup,
    /// A `[[note]]`, brackets included.
    Note,
}

/// A byte range of an element's source text and what it shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Span {
    pub start: usize,
    pub end: usize,
    pub kind: SpanKind,
}

enum PosToken {
    /// One printed character, and its source bytes (an escape's backslash is separate).
    Char(usize, usize),
    Escape(usize),
    Note(usize, usize),
    Marker(Marker, usize, usize),
}

/// [`tokenize`] with each token's place in the source.
fn tokenize_at(source: &str) -> Vec<PosToken> {
    let mut tokens = Vec::new();
    let mut chars = source.char_indices().peekable();
    while let Some((at, c)) = chars.next() {
        let end = at + c.len_utf8();
        match c {
            '\\' => match chars.next() {
                Some((next_at, next)) => {
                    tokens.push(PosToken::Escape(at));
                    tokens.push(PosToken::Char(next_at, next_at + next.len_utf8()));
                }
                None => tokens.push(PosToken::Char(at, end)),
            },
            '[' if chars.peek().map(|(_, c)| *c) == Some('[') => {
                let mut rest = chars.clone();
                rest.next();
                let mut close = None;
                while let Some((n_at, n)) = rest.next() {
                    if n == ']' && rest.peek().map(|(_, c)| *c) == Some(']') {
                        close = Some(n_at + 2);
                        break;
                    }
                }
                match close {
                    Some(close) => {
                        while chars.peek().is_some_and(|(i, _)| *i < close) {
                            chars.next();
                        }
                        tokens.push(PosToken::Note(at, close));
                    }
                    None => tokens.push(PosToken::Char(at, end)),
                }
            }
            '*' => {
                let mut count = 1;
                let mut stop = end;
                while count < 3 && chars.peek().map(|(_, c)| *c) == Some('*') {
                    if let Some((i, _)) = chars.next() {
                        stop = i + 1;
                    }
                    count += 1;
                }
                let marker = match count {
                    1 => Marker::Italic,
                    2 => Marker::Bold,
                    _ => Marker::BoldItalic,
                };
                tokens.push(PosToken::Marker(marker, at, stop));
            }
            '_' => tokens.push(PosToken::Marker(Marker::Underline, at, end)),
            _ => tokens.push(PosToken::Char(at, end)),
        }
    }
    tokens
}

/// What each part of `source` shows, in order, covering every byte: printed text with its
/// style, markup, and notes. The editor draws from it; it agrees with [`parse`].
pub fn spans(source: &str) -> Vec<Span> {
    let tokens = tokenize_at(source);
    let mut matched = vec![false; tokens.len()];
    let mut open: Vec<(usize, Marker)> = Vec::new();
    for (index, token) in tokens.iter().enumerate() {
        if let PosToken::Marker(marker, _, _) = token {
            if let Some(position) = open.iter().rposition(|(_, kind)| kind == marker) {
                let (start, _) = open.remove(position);
                for i in [start, index] {
                    if let Some(slot) = matched.get_mut(i) {
                        *slot = true;
                    }
                }
            } else {
                open.push((index, *marker));
            }
        }
    }
    let mut spans: Vec<Span> = Vec::new();
    let mut push = |start: usize, end: usize, kind: SpanKind| {
        if let Some(last) = spans.last_mut()
            && last.kind == kind
            && last.end == start
        {
            last.end = end;
        } else if start < end {
            spans.push(Span { start, end, kind });
        }
    };
    let mut style = Style::default();
    let mut active = [false; 4];
    for (index, token) in tokens.iter().enumerate() {
        match *token {
            PosToken::Char(start, end) => push(start, end, SpanKind::Text(style)),
            PosToken::Escape(at) => push(at, at + 1, SpanKind::Markup),
            PosToken::Note(start, end) => push(start, end, SpanKind::Note),
            PosToken::Marker(marker, start, end) => {
                if !matched.get(index).copied().unwrap_or(false) {
                    push(start, end, SpanKind::Text(style));
                    continue;
                }
                push(start, end, SpanKind::Markup);
                let slot = match marker {
                    Marker::Italic => 0,
                    Marker::Bold => 1,
                    Marker::BoldItalic => 2,
                    Marker::Underline => 3,
                };
                if let Some(flag) = active.get_mut(slot) {
                    *flag = !*flag;
                }
                let [italic, bold, bold_italic, underline] = active;
                style = Style { bold: bold || bold_italic, italic: italic || bold_italic, underline };
            }
        }
    }
    spans
}

/// Each printed character's source bytes and style (notes left out).
fn printed(source: &str) -> Vec<(usize, usize, Style)> {
    let mut out = Vec::new();
    for span in spans(source) {
        if let SpanKind::Text(style) = span.kind
            && let Some(text) = source.get(span.start..span.end)
        {
            for (i, c) in text.char_indices() {
                // An unmatched marker of two or three stars is printed as it stands, one
                // character at a time.
                out.push((span.start + i, span.start + i + c.len_utf8(), style));
            }
        }
    }
    out
}

/// A style the Format menu turns on and off.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StyleKind {
    Bold,
    Italic,
    Underline,
}

impl StyleKind {
    fn marker(self) -> &'static str {
        match self {
            StyleKind::Bold => "**",
            StyleKind::Italic => "*",
            StyleKind::Underline => "_",
        }
    }

    fn get(self, style: Style) -> bool {
        match self {
            StyleKind::Bold => style.bold,
            StyleKind::Italic => style.italic,
            StyleKind::Underline => style.underline,
        }
    }

    fn set(self, mut style: Style, on: bool) -> Style {
        match self {
            StyleKind::Bold => style.bold = on,
            StyleKind::Italic => style.italic = on,
            StyleKind::Underline => style.underline = on,
        }
        style
    }

    pub fn from_id(id: &str) -> Option<Self> {
        match id {
            "bold" => Some(StyleKind::Bold),
            "italic" => Some(StyleKind::Italic),
            "underline" => Some(StyleKind::Underline),
            _ => None,
        }
    }
}

/// Turns `which` on for the printed text in the byte range `start..end` of `source`, or off
/// when all of it has it already, as Format > Bold does. Returns the new source and the byte
/// range of the same text in it, or `None` when the range holds no printed text or the
/// markup cannot say it. Every candidate is checked: the printed text, the notes and every
/// other character's style stay as they were.
pub fn toggle_style(source: &str, start: usize, end: usize, which: StyleKind) -> Option<(String, usize, usize)> {
    let chars = printed(source);
    let first = chars.iter().position(|&(s, _, _)| s >= start)?;
    let last = chars.iter().rposition(|&(_, e, _)| e <= end)?;
    if last < first {
        return None;
    }
    let on = !chars.get(first..=last)?.iter().all(|&(_, _, style)| which.get(style));
    let want: Vec<Style> =
        chars.iter().enumerate().map(|(i, &(_, _, style))| if (first..=last).contains(&i) { which.set(style, on) } else { style }).collect();
    let notes = |s: &str| parse(s).into_iter().filter(|r| r.note).map(|r| r.text).collect::<Vec<_>>();
    let (from, to) = (chars.get(first)?.0, chars.get(last)?.1);
    let marker = which.marker();
    let mut candidates = Vec::new();
    // Taking off markers that wrap the text exactly.
    if !on && from >= marker.len() && source.get(from - marker.len()..from) == Some(marker) && source.get(to..to + marker.len()) == Some(marker) {
        candidates.push(format!("{}{}{}", &source[..from - marker.len()], &source[from..to], &source[to + marker.len()..]));
    }
    // A marker at each end: markers toggle, so this turns the style over for the text between.
    candidates.push(format!("{}{marker}{}{marker}{}", &source[..from], &source[from..to], &source[to..]));
    // Written afresh from styled runs, notes in their places.
    candidates.push(rewrite(source, &want));
    candidates.into_iter().find_map(|candidate| {
        let after = printed(&candidate);
        let same = after.len() == chars.len()
            && plain(&candidate) == plain(source)
            && notes(&candidate) == notes(source)
            && after.iter().zip(&want).all(|(&(_, _, got), &wanted)| got == wanted);
        let range = (after.get(first)?.0, after.get(last)?.1);
        same.then_some((candidate, range.0, range.1))
    })
}

/// `source` written again with each printed character in its `want`ed style.
fn rewrite(source: &str, want: &[Style]) -> String {
    let mut runs: Vec<Run> = Vec::new();
    let mut styles = want.iter();
    for run in parse(source) {
        if run.note {
            runs.push(run);
            continue;
        }
        for c in run.text.chars() {
            let style = styles.next().copied().unwrap_or(run.style);
            match runs.last_mut() {
                Some(last) if !last.note && last.style == style => last.text.push(c),
                _ => runs.push(Run { text: c.to_string(), style, note: false }),
            }
        }
    }
    write(&runs)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn styled(text: &str, bold: bool, italic: bool, underline: bool) -> Run {
        Run { text: text.into(), style: Style { bold, italic, underline }, note: false }
    }

    #[test]
    fn parses_emphasis() {
        let runs = parse("a *b* **c** ***d*** _e_ f");
        assert_eq!(
            runs,
            vec![
                styled("a ", false, false, false),
                styled("b", false, true, false),
                styled(" ", false, false, false),
                styled("c", true, false, false),
                styled(" ", false, false, false),
                styled("d", true, true, false),
                styled(" ", false, false, false),
                styled("e", false, false, true),
                styled(" f", false, false, false),
            ]
        );
    }

    #[test]
    fn unmatched_markers_are_literal() {
        assert_eq!(plain("file_name and 5 * 3"), "file_name and 5 * 3");
        assert_eq!(plain(r"\*not italic\*"), "*not italic*");
    }

    #[test]
    fn notes_are_not_printed() {
        assert_eq!(plain("Hello [[fix this]]world"), "Hello world");
        assert_eq!(plain("Open [[ never closed"), "Open [[ never closed");
    }

    #[test]
    fn write_round_trips() {
        let runs = vec![styled("Plain * star ", false, false, false), styled("bold", true, false, true), styled(" end_", false, false, false)];
        assert_eq!(parse(&write(&runs)), runs);
    }

    #[test]
    fn spans_agree_with_parse() {
        for source in [
            "a *b* **c** ***d*** _e_ f",
            "file_name and 5 * 3",
            r"\*not italic\*",
            "Hello [[fix this]]world",
            "Open [[ never closed",
            "_**both**_ and *é*",
            "**unclosed",
        ] {
            let from_spans: Vec<(String, Style)> = spans(source)
                .iter()
                .filter_map(|s| if let SpanKind::Text(style) = s.kind { Some((source[s.start..s.end].to_string(), style)) } else { None })
                .collect();
            let text: String = from_spans.iter().map(|(t, _)| t.as_str()).collect();
            assert_eq!(text, plain(source), "{source}");
            let mut covered = 0;
            for span in spans(source) {
                assert_eq!(span.start, covered, "{source}: every byte, in order");
                covered = span.end;
            }
            assert_eq!(covered, source.len(), "{source}");
            let styles: Vec<Style> = printed(source).iter().map(|&(_, _, s)| s).collect();
            let from_parse: Vec<Style> = parse(source).iter().filter(|r| !r.note).flat_map(|r| r.text.chars().map(move |_| r.style)).collect();
            assert_eq!(styles, from_parse, "{source}");
        }
    }

    fn toggled(source: &str, word: &str, which: StyleKind) -> String {
        let start = source.find(word).unwrap();
        let (out, s, e) = toggle_style(source, start, start + word.len(), which).unwrap_or_else(|| panic!("{source:?} {word:?} {which:?}"));
        assert_eq!(plain(&out[s..e]), plain(word), "the range still holds {word:?} in {out:?}");
        out
    }

    #[test]
    fn toggling_a_style_wraps_and_unwraps() {
        assert_eq!(toggled("She runs.", "runs", StyleKind::Bold), "She **runs**.");
        assert_eq!(toggled("She **runs**.", "runs", StyleKind::Bold), "She runs.");
        assert_eq!(toggled("She runs.", "runs", StyleKind::Italic), "She *runs*.");
        assert_eq!(toggled("She runs.", "She runs.", StyleKind::Underline), "_She runs._");
        assert_eq!(toggled("_She runs._", "She runs.", StyleKind::Underline), "She runs.");
        // Part of a bold word comes out of it.
        let out = toggled("**abc**", "b", StyleKind::Bold);
        let styles: Vec<bool> = printed(&out).iter().map(|&(_, _, s)| s.bold).collect();
        assert_eq!(styles, [true, false, true], "{out}");
        // Bold over italic gives bold italic.
        let out = toggled("*abc*", "abc", StyleKind::Bold);
        assert!(printed(&out).iter().all(|&(_, _, s)| s.bold && s.italic), "{out}");
        // Mixed: some bold, so all of it becomes bold.
        let out = toggled("a **b** c", "a **b** c", StyleKind::Bold);
        assert!(printed(&out).iter().all(|&(_, _, s)| s.bold), "{out}");
        // Literal stars and notes stay as they were.
        let out = toggled("5 * 3 [[check]] ok", "ok", StyleKind::Bold);
        assert_eq!(plain(&out), "5 * 3  ok");
        assert!(out.contains("[[check]]"), "{out}");
        // Bold straight after italic, with nothing between, has no markup that says it
        // (`*ab***c**` reads as bold italic): refused rather than written wrong.
        assert!(toggle_style("*ab*c", 4, 5, StyleKind::Bold).is_none());
        assert_eq!(toggled("*ab* c", "c", StyleKind::Bold), "*ab* **c**");
        assert!(toggle_style("[[only a note]]", 0, 15, StyleKind::Bold).is_none(), "nothing printed to style");
    }
}
