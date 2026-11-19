//! One paragraph of the script as the editor shows it: its text as it prints (the markup
//! hidden, capitals where its type prints them, notes faint), wrapped where the page wraps it,
//! and the map between places on screen and places in the stored text.
//!
//! Places on screen are indices into [`Shown::chars`]; places in the text are bytes. Typing and
//! deleting work on the stored text through [`replace`], so the markup stays right however
//! little of it shows.

use std::sync::Arc;

use egui::text::{CCursor, LayoutJob, TextFormat};
use egui::{Galley, Pos2, Rect, Ui, pos2, vec2};
use storyteller_core::inline::{self, SpanKind, Style};
use storyteller_core::layout::Align as PageAlign;
use storyteller_core::wrap;

/// How a character on screen is drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Look {
    /// Printed text in its style.
    Text(Style),
    /// A note, or markup while it shows: on screen, never printed.
    Faint,
    /// Text the input method is still composing: not in the script yet.
    Composing,
}

/// One character on screen and the bytes of the stored text it stands for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ShownChar {
    pub c: char,
    /// The stored bytes, an escaping backslash included. A capital that is two characters on
    /// screen (`ß` is `SS`) has the same bytes twice; composing text has none (`start == end`).
    pub start: usize,
    pub end: usize,
    pub look: Look,
}

/// How a paragraph's text is shown.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct ShowAs {
    /// Bold, italic and underline markers (and escaping backslashes) left out.
    pub hide_markup: bool,
    /// In capitals, however it was typed.
    pub caps: bool,
}

/// A paragraph's text as shown.
#[derive(Clone, Debug, Default)]
pub struct Shown {
    pub chars: Vec<ShownChar>,
    /// The stored text's length in bytes.
    len: usize,
}

impl Shown {
    /// `text` as shown, with `composing` text (from the input method) at its byte.
    pub fn new(text: &str, show: ShowAs, composing: Option<(usize, &str)>) -> Self {
        let mut out = Shown { chars: Vec::with_capacity(text.len()), len: text.len() };
        let mut composing = composing.filter(|(_, c)| !c.is_empty());
        let mut escape: Option<usize> = None;
        for span in inline::spans(text) {
            let Some(part) = text.get(span.start..span.end) else { continue };
            match span.kind {
                SpanKind::Markup if show.hide_markup => {
                    if part == "\\" {
                        escape = Some(span.start);
                    }
                }
                SpanKind::Markup | SpanKind::Note => {
                    for (i, c) in part.char_indices() {
                        out.push(&mut composing, c, span.start + i, span.start + i + c.len_utf8(), Look::Faint, false);
                    }
                }
                SpanKind::Text(style) => {
                    for (i, c) in part.char_indices() {
                        let start = if i == 0 { escape.take().unwrap_or(span.start) } else { span.start + i };
                        out.push(&mut composing, c, start, span.start + i + c.len_utf8(), Look::Text(style), show.caps);
                    }
                }
            }
        }
        if let Some((at, text)) = composing {
            out.chars.extend(text.chars().map(|c| ShownChar { c, start: at, end: at, look: Look::Composing }));
        }
        out
    }

    fn push(&mut self, composing: &mut Option<(usize, &str)>, c: char, start: usize, end: usize, look: Look, caps: bool) {
        if let Some((at, text)) = *composing
            && start >= at
        {
            self.chars.extend(text.chars().map(|c| ShownChar { c, start: at, end: at, look: Look::Composing }));
            *composing = None;
        }
        let c = if c == '\t' { ' ' } else { c };
        if caps && c != '\n' {
            self.chars.extend(c.to_uppercase().map(|c| ShownChar { c, start, end, look }));
        } else {
            self.chars.push(ShownChar { c, start, end, look });
        }
    }

    pub fn len(&self) -> usize {
        self.chars.len()
    }

    /// The stored byte a place on screen stands for: just after the character before it, so
    /// what is typed there takes that character's style. At the very start, just before the
    /// first character.
    pub fn to_stored(&self, at: usize) -> usize {
        if at == 0 {
            return self.chars.first().map_or(self.len, |c| c.start);
        }
        self.chars.get(at - 1).map_or(self.len, |c| c.end)
    }

    /// The place on screen a stored byte is at.
    pub fn place_of(&self, byte: usize) -> usize {
        self.chars.iter().take_while(|c| c.end <= byte && !(c.look == Look::Composing && c.start > byte)).count()
    }

    /// The stored bytes of the characters `from..to` on screen.
    pub fn stored_range(&self, from: usize, to: usize) -> (usize, usize) {
        let (from, to) = (from.min(to), from.max(to));
        if from == to {
            let at = self.to_stored(from);
            return (at, at);
        }
        let start = self.chars.get(from).map_or(self.len, |c| c.start);
        let end = to.checked_sub(1).and_then(|i| self.chars.get(i)).map_or(self.len, |c| c.end);
        (start, end.max(start))
    }

    /// The place before the character before `at`: a capital that is two characters on screen
    /// is one character to step over.
    pub fn prev(&self, at: usize) -> usize {
        let mut at = at.min(self.len()).saturating_sub(1);
        while at > 0 && self.same_char(at - 1, at) {
            at -= 1;
        }
        at
    }

    /// The place after the character at `at`.
    pub fn next(&self, at: usize) -> usize {
        let mut at = (at + 1).min(self.len());
        while at < self.len() && at > 0 && self.same_char(at - 1, at) {
            at += 1;
        }
        at
    }

    fn same_char(&self, a: usize, b: usize) -> bool {
        match (self.chars.get(a), self.chars.get(b)) {
            (Some(a), Some(b)) => a.start == b.start && a.end == b.end && a.start < a.end,
            _ => false,
        }
    }

    fn is_word(&self, at: usize) -> bool {
        self.chars.get(at).is_some_and(|c| c.c.is_alphanumeric() || matches!(c.c, '\'' | '’'))
    }

    /// The start of the word before `at`.
    pub fn word_left(&self, mut at: usize) -> usize {
        at = at.min(self.len());
        while at > 0 && !self.is_word(at - 1) {
            at -= 1;
        }
        while at > 0 && self.is_word(at - 1) {
            at -= 1;
        }
        at
    }

    /// The end of the word after `at`.
    pub fn word_right(&self, mut at: usize) -> usize {
        while at < self.len() && !self.is_word(at) {
            at += 1;
        }
        while at < self.len() && self.is_word(at) {
            at += 1;
        }
        at
    }

    /// The word around `at`, for a double click.
    pub fn word_at(&self, at: usize) -> (usize, usize) {
        let (mut from, mut to) = (at.min(self.len()), at.min(self.len()));
        if !self.is_word(from) && from > 0 && self.is_word(from - 1) {
            from -= 1;
            to = from;
        }
        if !self.is_word(from) {
            return (from, (from + 1).min(self.len()));
        }
        while from > 0 && self.is_word(from - 1) {
            from -= 1;
        }
        while to < self.len() && self.is_word(to) {
            to += 1;
        }
        (from, to)
    }

    /// Where the lines of a paragraph `width` characters wide start and end on screen.
    pub fn lines(&self, width: u16) -> Vec<(usize, usize)> {
        let chars: Vec<char> = self.chars.iter().map(|c| c.c).collect();
        wrap::line_ranges(&chars, width, width)
    }
}

/// The text after an edit, and where the caret goes in it (bytes).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Edit {
    pub text: String,
    pub caret: usize,
}

/// `text` with the characters `from..to` on screen replaced by `typed`. While the markup is
/// hidden, what is typed is taken literally (a typed `*` is escaped, not a marker), and what is
/// taken out takes emptied markers with it.
pub fn replace(text: &str, show: ShowAs, from: usize, to: usize, typed: &str) -> Edit {
    let shown = Shown::new(text, show, None);
    let (from, to) = (from.min(to).min(shown.len()), from.max(to).min(shown.len()));
    let (removed, after) = if from < to {
        let (start, end) = shown.stored_range(from, to);
        let removed = if show.hide_markup {
            inline::remove(text, start, end)
        } else {
            format!("{}{}", text.get(..start).unwrap_or(""), text.get(end..).unwrap_or(""))
        };
        let after = Shown::new(&removed, show, None);
        (removed, after)
    } else {
        (text.to_string(), shown)
    };
    let at = after.to_stored(from).min(removed.len());
    let in_note = inline::spans(&removed).iter().any(|s| s.kind == SpanKind::Note && s.start < at && at < s.end);
    let mut typed_as = String::with_capacity(typed.len());
    for c in typed.chars() {
        if show.hide_markup && !in_note && matches!(c, '*' | '_' | '\\') {
            typed_as.push('\\');
        }
        typed_as.push(c);
    }
    let mut out = String::with_capacity(removed.len() + typed_as.len());
    out.push_str(removed.get(..at).unwrap_or(""));
    out.push_str(&typed_as);
    out.push_str(removed.get(at..).unwrap_or(""));
    Edit { text: out, caret: at + typed_as.len() }
}

/// One line of a paragraph on screen.
pub struct Line {
    /// Its characters on screen, `start..end`.
    pub start: usize,
    pub end: usize,
    pub galley: Arc<Galley>,
    /// Where its text starts.
    pub pos: Pos2,
}

/// Where a paragraph's lines go.
pub struct Geometry {
    /// The paragraph's left edge and the top of its first line.
    pub origin: Pos2,
    /// The width of the paragraph in characters, and in pixels.
    pub columns: u16,
    pub width: f32,
    pub align: PageAlign,
    /// One row, and the distance from one line to the next (two rows when double-spaced).
    pub row: f32,
    pub step: f32,
    /// How far one character advances.
    pub advance: f32,
    /// Extra space before a line, pushing it and those after it down: where a paragraph runs
    /// on to the next sheet. `(line, pixels)`.
    pub gaps: Vec<(usize, f32)>,
}

/// A paragraph laid out on screen.
pub struct Laid {
    pub shown: Shown,
    pub lines: Vec<Line>,
    /// The paragraph's box: every line, at the paragraph's full width.
    pub rect: Rect,
    row: f32,
    advance: f32,
}

impl Laid {
    /// Lays `shown` out in `frame`, each character in the format `format` gives it.
    pub fn new(ui: &Ui, shown: Shown, frame: &Geometry, format: impl Fn(usize, &ShownChar) -> TextFormat) -> Self {
        let ranges = shown.lines(frame.columns);
        let mut lines = Vec::with_capacity(ranges.len());
        let mut y = frame.origin.y;
        for (n, (start, end)) in ranges.into_iter().enumerate() {
            if n > 0 {
                y += frame.step;
            }
            y += frame.gaps.iter().filter(|(line, _)| *line == n).map(|(_, gap)| gap).sum::<f32>();
            let mut job = LayoutJob::default();
            job.wrap.max_width = f32::INFINITY;
            let mut part = String::new();
            let mut current: Option<TextFormat> = None;
            for (i, c) in shown.chars.iter().enumerate().take(end).skip(start) {
                let mut f = format(i, c);
                f.line_height = Some(frame.row);
                if current.as_ref() != Some(&f) {
                    if let Some(previous) = current.take() {
                        job.append(&std::mem::take(&mut part), 0.0, previous);
                    }
                    current = Some(f);
                }
                part.push(c.c);
            }
            if let Some(previous) = current {
                job.append(&part, 0.0, previous);
            }
            let galley = ui.fonts_mut(|f| f.layout_job(job));
            let spare = frame.width - galley.size().x;
            let x = match frame.align {
                PageAlign::Left => frame.origin.x,
                PageAlign::Right => frame.origin.x + spare,
                PageAlign::Center => frame.origin.x + spare / 2.0,
            };
            lines.push(Line { start, end, galley, pos: pos2(x, y) });
        }
        let bottom = y + frame.row;
        let rect = Rect::from_min_max(frame.origin, pos2(frame.origin.x + frame.width, bottom));
        Laid { shown, lines, rect, row: frame.row, advance: frame.advance }
    }

    /// The line a place on screen is on: a place between two lines is at the start of the
    /// later one.
    pub fn line_of(&self, at: usize) -> usize {
        self.lines.iter().rposition(|l| l.start <= at).unwrap_or(0)
    }

    /// The caret's rectangle at a place on screen.
    pub fn caret_rect(&self, at: usize) -> Rect {
        let Some(line) = self.lines.get(self.line_of(at)) else { return Rect::from_min_size(self.rect.min, vec2(0.0, self.row)) };
        let column = at.saturating_sub(line.start);
        let length = line.end - line.start;
        let x = if column <= length {
            line.galley.pos_from_cursor(CCursor::new(column)).min.x
        } else {
            // Spaces typed at the end of a line, before the next word wraps.
            line.galley.size().x + (column - length) as f32 * self.advance
        };
        Rect::from_min_size(pos2(line.pos.x + x, line.pos.y), vec2(0.0, self.row))
    }

    /// The place on screen nearest `pos`.
    pub fn place_at(&self, pos: Pos2) -> usize {
        let index = self.lines.iter().rposition(|l| l.pos.y <= pos.y).unwrap_or(0);
        let Some(line) = self.lines.get(index) else { return 0 };
        let column: usize = line.galley.cursor_from_pos(pos - line.pos).index.into();
        line.start + column.min(line.end - line.start)
    }

    /// The place one line up or down from `at`, in column `x`, if there is a line there.
    pub fn line_step(&self, at: usize, x: f32, up: bool) -> Option<usize> {
        let line = self.line_of(at);
        let target = if up { line.checked_sub(1)? } else { line + 1 };
        let target = self.lines.get(target)?;
        Some(self.place_at(pos2(x, target.pos.y + self.row * 0.5)))
    }

    /// The start and end of the line `at` is on.
    pub fn line_ends(&self, at: usize) -> (usize, usize) {
        self.lines.get(self.line_of(at)).map_or((0, 0), |l| (l.start, l.end))
    }

    /// Whether `at` is on the first line, or on the last.
    pub fn on_first_line(&self, at: usize) -> bool {
        self.line_of(at) == 0
    }

    pub fn on_last_line(&self, at: usize) -> bool {
        self.line_of(at) + 1 >= self.lines.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HIDDEN: ShowAs = ShowAs { hide_markup: true, caps: false };
    const SHOWN: ShowAs = ShowAs { hide_markup: false, caps: false };

    fn text(shown: &Shown) -> String {
        shown.chars.iter().map(|c| c.c).collect()
    }

    #[test]
    fn markup_hides_and_places_map_both_ways() {
        let shown = Shown::new("She **runs** home.", HIDDEN, None);
        assert_eq!(text(&shown), "She runs home.");
        assert_eq!(shown.to_stored(4), 4, "before the word: before its markers");
        assert_eq!(shown.to_stored(8), 10, "after the word: inside its markers, so typing stays bold");
        assert_eq!(shown.place_of(6), 4, "the place just inside the markers is the word's start");
        assert_eq!(shown.place_of(12), 8);
        assert_eq!(shown.stored_range(4, 8), (6, 10));
        assert_eq!(text(&Shown::new("She **runs** home.", SHOWN, None)), "She **runs** home.");
        assert_eq!(text(&Shown::new("a \\*b [[note]]", HIDDEN, None)), "a *b [[note]]", "notes stay, faint");
    }

    #[test]
    fn capitals_by_display_keep_the_stored_casing() {
        let caps = ShowAs { hide_markup: true, caps: true };
        let shown = Shown::new("Straße", caps, None);
        assert_eq!(text(&shown), "STRASSE");
        assert_eq!(shown.prev(6), 4, "the two capitals of one letter step as one");
        assert_eq!(shown.to_stored(7), "Straße".len());
        assert_eq!(replace("Straße", caps, 7, 7, "n"), Edit { text: "Straßen".into(), caret: 8 });
    }

    #[test]
    fn typing_and_deleting_keep_the_markup_right() {
        assert_eq!(replace("She **runs** home.", HIDDEN, 8, 8, "s"), Edit { text: "She **runss** home.".into(), caret: 11 });
        assert_eq!(replace("She **runs** home.", HIDDEN, 4, 9, ""), Edit { text: "She home.".into(), caret: 4 }, "the word and its markers go");
        assert_eq!(replace("a b", HIDDEN, 1, 1, "*x_"), Edit { text: "a\\*x\\_ b".into(), caret: 6 }, "typed markers are literal");
        assert_eq!(replace("a b", SHOWN, 1, 1, "*x"), Edit { text: "a*x b".into(), caret: 3 }, "with the markup showing, they are markup");
        assert_eq!(replace("a [[no]] b", HIDDEN, 4, 4, "*"), Edit { text: "a [[*no]] b".into(), caret: 5 }, "inside a note, nothing to escape");
        let composing = Shown::new("ab", HIDDEN, Some((1, "xy")));
        assert_eq!(text(&composing), "axyb");
        assert_eq!(composing.place_of(1), 3, "the caret after the composing text");
    }

    #[test]
    fn words_and_lines() {
        let shown = Shown::new("one two  three", HIDDEN, None);
        assert_eq!(shown.word_left(9), 4);
        assert_eq!(shown.word_right(3), 7);
        assert_eq!(shown.word_at(5), (4, 7));
        assert_eq!(shown.lines(8), vec![(0, 7), (9, 14)]);
    }
}
