//! Monospace word wrapping over styled characters.

use crate::inline::{self, Run, Style};
use crate::model::{Deletion, Revised};

/// One printable character and its style.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StyledChar {
    pub c: char,
    pub style: Style,
    /// The revision set the character was changed in.
    pub revised: Option<u32>,
    /// A deletion mark of this set sits beside the character (see [`revised_chars`]).
    pub deleted: Option<u32>,
}

impl StyledChar {
    pub fn new(c: char, style: Style) -> Self {
        StyledChar { c, style, ..StyledChar::default() }
    }
}

/// Printable characters of element markup (notes removed), optionally in capitals.
pub fn styled_chars(markup: &str, caps: bool) -> Vec<StyledChar> {
    revised_chars(markup, caps, &[], &[])
}

/// [`styled_chars`] with each character's revision set, from spans of printed characters. A
/// deletion mark goes on the character before its place (the first, at the start), so it stays
/// on the line the deleted text was on.
pub fn revised_chars(markup: &str, caps: bool, revised: &[Revised], deletions: &[Deletion]) -> Vec<StyledChar> {
    let mut out = Vec::new();
    // Where each printed character starts in `out` (capitals can be longer).
    let mut starts = Vec::new();
    // Spans start and end in order of place; where they overlap, the later in the list wins.
    let mut edges: Vec<(u32, bool, usize)> =
        revised.iter().enumerate().filter(|(_, r)| r.from < r.to).flat_map(|(i, r)| [(r.from, true, i), (r.to, false, i)]).collect();
    edges.sort_unstable();
    let mut edges = edges.into_iter().peekable();
    let mut open = std::collections::BTreeSet::new();
    for run in inline::parse(markup) {
        if run.note {
            continue;
        }
        for c in run.text.chars() {
            let printed = u32::try_from(starts.len()).unwrap_or(u32::MAX);
            starts.push(out.len());
            while let Some((_, start, i)) = edges.next_if(|&(at, _, _)| at <= printed) {
                if start {
                    open.insert(i);
                } else {
                    open.remove(&i);
                }
            }
            let set = open.last().and_then(|&i| revised.get(i)).map(|r| r.set);
            let c = if c == '\t' { ' ' } else { c };
            let styled = |c| StyledChar { c, style: run.style, revised: set, deleted: None };
            if caps {
                out.extend(c.to_uppercase().map(styled));
            } else {
                out.push(styled(c));
            }
        }
    }
    for deletion in deletions {
        let at = usize::try_from(deletion.at).unwrap_or(usize::MAX);
        let index = at.checked_sub(1).and_then(|i| starts.get(i)).copied().unwrap_or(0);
        if let Some(sc) = out.get_mut(index) {
            sc.deleted = Some(deletion.set);
        }
    }
    out
}

fn plain(chars: &str) -> Vec<StyledChar> {
    chars.chars().map(|c| StyledChar::new(c, Style::default())).collect()
}

/// Unstyled characters.
pub fn unstyled(text: &str) -> Vec<StyledChar> {
    plain(text)
}

/// Where a line may end inside `window` (the next `width + 1` characters): the length of the
/// line that ends there. A line ends before a space, or after a hyphen inside a word; a
/// hyphen may hang one column past the margin, as Final Draft allows.
fn last_break(window: &[char], width: usize) -> Option<usize> {
    let c = |i: usize| window.get(i).copied();
    (1..window.len()).rev().find_map(|i| {
        if c(i) == Some(' ') && i <= width {
            return Some(i);
        }
        let in_word = c(i.wrapping_sub(1)).is_some_and(|p| p != ' ') && c(i + 1).is_some_and(|n| n != ' ' && n != '-');
        // A dash before a space may also hang: `TREES -` or `FACE --` at the margin.
        let hangs = i == width && c(i + 1) == Some(' ');
        (c(i) == Some('-') && (in_word || hangs)).then_some(i + 1)
    })
}

/// Wrap at spaces (and after hyphens) to `width` characters; words longer than a line are
/// split. Hard line breaks (`\n`) always start a new line. Always returns at least one
/// (possibly empty) line.
pub fn wrap(chars: &[StyledChar], width: u16) -> Vec<Vec<StyledChar>> {
    wrap_first(chars, width, width)
}

/// [`wrap`] with a different width for the very first line (a hanging first indent).
pub fn wrap_first(chars: &[StyledChar], first_width: u16, width: u16) -> Vec<Vec<StyledChar>> {
    let plain: Vec<char> = chars.iter().map(|sc| sc.c).collect();
    line_ranges(&plain, first_width, width).into_iter().map(|(start, end)| chars.get(start..end).unwrap_or(&[]).to_vec()).collect()
}

/// The lines [`wrap_first`] makes of `chars`, as `start..end` indices into it. A line's end
/// leaves out the spaces it ends with; the spaces a wrap swallows and each `\n` belong to no
/// line. The editor lays its text out with this, so a paragraph wraps on screen where it prints.
pub fn line_ranges(chars: &[char], first_width: u16, width: u16) -> Vec<(usize, usize)> {
    let rest_width = usize::from(width.max(1));
    let mut lines: Vec<(usize, usize)> = Vec::new();
    // The whole line from `from` to `to`, its spaces at the end left out.
    let trimmed = |from: usize, mut to: usize| {
        while to > from && chars.get(to - 1) == Some(&' ') {
            to -= 1;
        }
        (from, to)
    };
    let mut hard_start = 0;
    for hard in chars.split(|&c| c == '\n') {
        let mut start = 0;
        loop {
            let width = if lines.is_empty() { usize::from(first_width.max(1)) } else { rest_width };
            let rest = hard.get(start..).unwrap_or(&[]);
            let fits = rest.len() <= width || (rest.len() == width + 1 && rest.last() == Some(&'-'));
            if fits {
                lines.push(trimmed(hard_start + start, hard_start + hard.len()));
                break;
            }
            let window = rest.get(..=width.saturating_add(1)).unwrap_or(rest);
            let next = last_break(window, width).unwrap_or(width);
            lines.push(trimmed(hard_start + start, hard_start + start + next));
            let mut next = start + next;
            while hard.get(next) == Some(&' ') {
                next += 1;
            }
            start = next;
        }
        hard_start += hard.len() + 1;
    }
    if lines.is_empty() {
        lines.push((0, 0));
    }
    lines
}

/// Group a line into runs of equal style.
pub fn runs(line: &[StyledChar]) -> Vec<Run> {
    let mut runs: Vec<Run> = Vec::new();
    for sc in line {
        match runs.last_mut() {
            Some(run) if run.style == sc.style && run.revised == sc.revised => run.text.push(sc.c),
            _ => runs.push(Run { text: sc.c.to_string(), style: sc.style, note: false, revised: sc.revised }),
        }
    }
    runs
}

/// Split positions between sentences: `(end of the first part, start of the second)`.
/// A sentence ends at `.`, `?`, `!`, `…` or `--` (optionally followed by closing quotes or
/// brackets) followed by a space, or at a hard line break.
pub fn sentence_breaks(chars: &[StyledChar]) -> Vec<(usize, usize)> {
    let mut breaks = Vec::new();
    let mut index = 0;
    while index < chars.len() {
        let c = chars.get(index).map(|sc| sc.c).unwrap_or(' ');
        if c == '\n' {
            breaks.push((index, index + 1));
            index += 1;
            continue;
        }
        let dash = c == '-' && chars.get(index + 1).is_some_and(|sc| sc.c == '-');
        if matches!(c, '.' | '?' | '!' | '…') || dash {
            let mut end = index + if dash { 2 } else { 1 };
            while chars.get(end).is_some_and(|sc| matches!(sc.c, '"' | '\'' | ')' | '”' | '’' | '.' | '?' | '!')) {
                end += 1;
            }
            if chars.get(end).is_some_and(|sc| sc.c == ' ') {
                let mut next = end;
                while chars.get(next).is_some_and(|sc| sc.c == ' ') {
                    next += 1;
                }
                if next < chars.len() {
                    breaks.push((end, next));
                }
                index = next;
                continue;
            }
            index = end;
            continue;
        }
        index += 1;
    }
    breaks
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(lines: &[Vec<StyledChar>]) -> Vec<String> {
        lines.iter().map(|line| line.iter().map(|sc| sc.c).collect()).collect()
    }

    #[test]
    fn wraps_at_spaces() {
        let chars = unstyled("the quick brown fox jumps over the lazy dog");
        assert_eq!(text(&wrap(&chars, 10)), ["the quick", "brown fox", "jumps over", "the lazy", "dog"]);
    }

    #[test]
    fn breaks_after_hyphens_which_may_hang_one_column() {
        assert_eq!(text(&wrap(&unstyled("a well-known fact"), 8)), ["a well-", "known", "fact"]);
        assert_eq!(text(&wrap(&unstyled("abcd efgh-ijk"), 9)), ["abcd efgh-", "ijk"], "the hyphen hangs");
        assert_eq!(text(&wrap(&unstyled("abcd efghi-jk"), 9)), ["abcd", "efghi-jk"], "nine plus a hanging hyphen");
        assert_eq!(text(&wrap(&unstyled("abcdefghi-jk"), 9)), ["abcdefghi-", "jk"]);
        assert_eq!(text(&wrap(&unstyled("wait -- what"), 7)), ["wait --", "what"]);
        assert_eq!(text(&wrap(&unstyled("- a list"), 4)), ["- a", "list"], "a leading dash is not a break");
    }

    #[test]
    fn splits_long_words_and_keeps_hard_breaks() {
        let chars = unstyled("abcdefghijkl\nxy");
        assert_eq!(text(&wrap(&chars, 5)), ["abcde", "fghij", "kl", "xy"]);
        assert_eq!(text(&wrap(&[], 5)), [""]);
        assert_eq!(text(&wrap(&unstyled("a b"), 0)), ["a", "b"]);
    }

    #[test]
    fn line_ranges_index_the_text() {
        let chars: Vec<char> = "the quick  brown\nfox".chars().collect();
        assert_eq!(line_ranges(&chars, 10, 10), vec![(0, 9), (11, 16), (17, 20)], "spaces between lines and the break belong to none");
        assert_eq!(line_ranges(&[], 5, 5), vec![(0, 0)]);
        let chars: Vec<char> = "abcdefg".chars().collect();
        assert_eq!(line_ranges(&chars, 3, 3), vec![(0, 3), (3, 6), (6, 7)], "a long word splits where it must");
    }

    #[test]
    fn finds_sentence_breaks() {
        let chars = unstyled("One. Two?\" Three -- four");
        let breaks = sentence_breaks(&chars);
        assert_eq!(breaks, vec![(4, 5), (10, 11), (19, 20)]);
    }
}
