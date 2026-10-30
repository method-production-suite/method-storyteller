//! Industry-standard pagination: elements → wrapped blocks → pages, with the break rules of
//! `docs/spec.md` §5.1 (scene headings keep with what follows, dialogue splits between
//! sentences with `(MORE)` / `(CONT'D)`, transitions never open a page …).

use std::collections::VecDeque;

use serde::{Deserialize, Serialize};

use crate::inline::Run;
use crate::layout::{Align, ElementStyle, Layout};
use crate::model::{ElementKind, RevisionSet, Revisions, Script, ShowRevisions, TitlePage};
use crate::wrap::{self, StyledChar};

/// Hard cap on output pages, a guard against pathological layouts.
pub const MAX_PAGES: usize = 50_000;

/// What a printed line is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "role", content = "kind")]
pub enum LineRole {
    Element(ElementKind),
    More,
    Contd,
    PageNumber,
    SceneNumber,
    Title,
    /// The characters of a lettered scene, listed under its heading: `(NORA, OWEN)`.
    Cast,
}

/// One printed line at an absolute cell position.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PageLine {
    pub row: u16,
    pub col: u16,
    pub runs: Vec<Run>,
    pub role: LineRole,
    /// Index of the element this line belongs to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub element: Option<usize>,
    /// Revision sets with revised text or a deletion mark on this line: the margin marks it.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub marks: Vec<u32>,
}

impl PageLine {
    fn new(row: u16, col: u16, chars: &[StyledChar], role: LineRole, element: Option<usize>) -> Self {
        let mut marks: Vec<u32> = chars.iter().flat_map(|sc| sc.revised.into_iter().chain(sc.deleted)).collect();
        marks.sort_unstable();
        marks.dedup();
        PageLine { row, col, runs: wrap::runs(chars), role, element, marks }
    }

    pub fn text(&self) -> String {
        self.runs.iter().map(|run| run.text.as_str()).collect()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Page {
    /// 1-based position in the script; 0 for the title page.
    pub number: u32,
    pub lines: Vec<PageLine>,
    /// The number a locked script prints, such as `12A` or `7-8` (docs/production-spec.md §4.1).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub locked: Option<String>,
    /// Where the page's text starts: the element, and its printed characters before the page.
    #[serde(skip)]
    pub start: Option<(usize, u32)>,
}

impl Page {
    /// The page's number as printed, such as `12`, or `12A` when the pages are locked.
    pub fn label(&self) -> String {
        self.locked.clone().unwrap_or_else(|| self.number.to_string())
    }

    /// The set a revised page is issued in: the latest shown set with a mark on it.
    pub fn revision<'a>(&self, revisions: &'a Revisions, show: ShowRevisions) -> Option<&'a RevisionSet> {
        revisions.latest(self.lines.iter().flat_map(|line| &line.marks), show)
    }
}

/// A scene heading's position in the paginated script.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SceneInfo {
    pub element: usize,
    pub number: Option<String>,
    pub heading: String,
    pub page: u32,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Paginated {
    pub title_page: Option<Page>,
    pub pages: Vec<Page>,
    pub scenes: Vec<SceneInfo>,
    /// Page on which each element starts (0 for elements that are not printed before any page).
    pub element_pages: Vec<u32>,
}

impl Paginated {
    pub fn page_count(&self) -> usize {
        self.pages.len()
    }

    /// The pages a selection issues, as indexes into [`Paginated::pages`], in order. Each part
    /// given narrows the pages: a page range, a scene range, revised pages only.
    pub fn select(&self, selection: &PageSelection, revisions: &Revisions, show: ShowRevisions) -> Result<Vec<usize>, String> {
        let mut chosen: Vec<bool> = vec![true; self.pages.len()];
        if let Some(spec) = selection.pages.as_deref().filter(|s| !s.trim().is_empty()) {
            let labels: Vec<String> = self.pages.iter().map(Page::label).collect();
            let mut wanted = vec![false; self.pages.len()];
            for (from, to) in ranges(spec, &labels, "page")? {
                wanted.iter_mut().take(to + 1).skip(from).for_each(|w| *w = true);
            }
            chosen.iter_mut().zip(wanted).for_each(|(c, w)| *c &= w);
        }
        if let Some(spec) = selection.scenes.as_deref().filter(|s| !s.trim().is_empty()) {
            let labels: Vec<String> =
                self.scenes.iter().enumerate().map(|(i, scene)| scene.number.clone().unwrap_or_else(|| (i + 1).to_string())).collect();
            let mut wanted = vec![false; self.pages.len()];
            for (from, to) in ranges(spec, &labels, "scene")? {
                for scene in from..=to {
                    let (first, last) = self.scene_pages(scene);
                    wanted.iter_mut().take(last + 1).skip(first).for_each(|w| *w = true);
                }
            }
            chosen.iter_mut().zip(wanted).for_each(|(c, w)| *c &= w);
        }
        if selection.revised_only {
            for (c, page) in chosen.iter_mut().zip(&self.pages) {
                *c &= page.revision(revisions, show).is_some();
            }
        }
        Ok(chosen.iter().enumerate().filter(|(_, c)| **c).map(|(i, _)| i).collect())
    }

    /// The first and last page index scene `index` prints on: up to the page where the next
    /// scene starts, unless that page opens with the next scene.
    fn scene_pages(&self, index: usize) -> (usize, usize) {
        let page_index = |page: u32| usize::try_from(page).ok().and_then(|p| p.checked_sub(1)).unwrap_or(0);
        let Some(scene) = self.scenes.get(index) else { return (0, 0) };
        let first = page_index(scene.page);
        let last = match self.scenes.get(index + 1) {
            None => self.pages.len().saturating_sub(1),
            Some(next) => {
                let at = page_index(next.page);
                let opens = self.pages.get(at).and_then(|p| p.lines.iter().filter_map(|l| l.element).min()).is_some_and(|e| e >= next.element);
                if opens { at.saturating_sub(1) } else { at }
            }
        };
        (first, last.max(first))
    }
}

/// Which pages File > Export PDF… issues (docs/production-spec.md §3.6).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PageSelection {
    /// Page labels and ranges, such as `12-18, 22A`.
    pub pages: Option<String>,
    /// Scene numbers and ranges, such as `12-15` (scenes count from 1 when unnumbered).
    pub scenes: Option<String>,
    /// Only pages with a shown revision.
    pub revised_only: bool,
}

impl PageSelection {
    /// Every page, with the title page.
    pub fn is_all(&self) -> bool {
        !self.revised_only && [&self.pages, &self.scenes].iter().all(|s| s.as_deref().is_none_or(|s| s.trim().is_empty()))
    }
}

/// `12-18, 22A` as inclusive index ranges into `labels`, matched without regard to case. A range
/// runs by position, so `12-13` takes in `12A`.
fn ranges(spec: &str, labels: &[String], what: &str) -> Result<Vec<(usize, usize)>, String> {
    let find = |label: &str| {
        let label = label.trim();
        labels.iter().position(|l| l.eq_ignore_ascii_case(label)).ok_or_else(|| format!("there is no {what} {label}"))
    };
    let mut out = Vec::new();
    for part in spec.split(',').map(str::trim).filter(|p| !p.is_empty()) {
        let range = match part.split_once(['-', '\u{2013}']) {
            Some((from, to)) => (find(from)?, find(to)?),
            None => (find(part)?, find(part)?),
        };
        if range.0 > range.1 {
            return Err(format!("the {what} range {part} runs backwards"));
        }
        out.push(range);
    }
    if out.is_empty() {
        return Err(format!("no {what}s were named"));
    }
    Ok(out)
}

#[derive(Clone, Debug)]
struct Line {
    col: u16,
    chars: Vec<StyledChar>,
}

#[derive(Clone, Debug)]
struct Block {
    element: usize,
    kind: ElementKind,
    style: ElementStyle,
    space_before: u16,
    source: Vec<StyledChar>,
    lines: Vec<Line>,
    /// Text may be re-split at sentence boundaries.
    splittable: bool,
    /// The last lines are the scene's cast list, not the element's own text.
    cast_lines: usize,
    /// Printed characters of the element before this block's text (a block split off a page).
    from: u32,
}

impl Block {
    fn new(element: usize, kind: ElementKind, style: ElementStyle, source: Vec<StyledChar>, splittable: bool) -> Self {
        let lines = layout_lines(&source, style, kind == ElementKind::Parenthetical);
        Block { element, kind, style, space_before: style.space_before, source, lines, splittable, cast_lines: 0, from: 0 }
    }

    fn with_source(&self, source: Vec<StyledChar>) -> Self {
        Block { from: self.from, ..Block::new(self.element, self.kind, self.style, source, self.splittable) }
    }

    /// The text from printed character `start` of this block on, as its own block.
    fn rest_from(&self, start: usize) -> Self {
        let from = self.from.saturating_add(u32::try_from(start).unwrap_or(u32::MAX));
        Block { from, ..self.with_source(self.source.get(start..).unwrap_or(&[]).to_vec()) }
    }

    /// Rows the block takes: one a line, with the gaps of a double-spaced element between.
    fn height(&self) -> u16 {
        let lines = u16::try_from(self.lines.len()).unwrap_or(u16::MAX);
        let gaps = lines.saturating_sub(1).saturating_mul(self.style.spacing.max(1).saturating_sub(1));
        lines.saturating_add(gaps)
    }

    /// Split the laid-out lines without re-wrapping (a last resort).
    fn split_lines(&self, at: usize) -> (Block, Block) {
        let at = at.min(self.lines.len());
        let mut top = self.clone();
        let mut rest = self.clone();
        top.lines.truncate(at);
        rest.lines = self.lines.get(at..).unwrap_or(&[]).to_vec();
        // The rest starts after the top's characters and the spaces the wrap dropped.
        let taken: usize = top.lines.iter().map(|line| line.chars.len()).sum();
        let skipped = self.source.get(taken..).unwrap_or(&[]).iter().take_while(|sc| sc.c == ' ').count();
        rest.from = self.from.saturating_add(u32::try_from(taken + skipped).unwrap_or(u32::MAX));
        top.splittable = false;
        rest.splittable = false;
        (top, rest)
    }
}

fn layout_lines(source: &[StyledChar], style: ElementStyle, hanging_indent: bool) -> Vec<Line> {
    // A hanging first line starts one column further left, so it holds one more character.
    let first = if hanging_indent { style.width.saturating_add(1) } else { style.width };
    let wrapped = wrap::wrap_first(source, first, style.width);
    wrapped
        .into_iter()
        .enumerate()
        .map(|(index, chars)| {
            let len = u16::try_from(chars.len()).unwrap_or(u16::MAX);
            let slack = style.width.saturating_sub(len);
            let col = match style.align {
                Align::Left => style.left.saturating_add(if hanging_indent && index > 0 { 1 } else { 0 }),
                Align::Right => style.left.saturating_add(slack),
                Align::Center => style.left.saturating_add(slack / 2),
            };
            Line { col, chars }
        })
        .collect()
}

#[derive(Clone, Debug)]
struct Speech {
    cue: Block,
    parts: Vec<Block>,
    /// The cue is a continuation from the previous page: print `NAME (CONT'D)`.
    contd: bool,
    /// The speech continues on the next page: print `(MORE)` after it.
    more: bool,
}

impl Speech {
    fn content(&self) -> u16 {
        self.parts.iter().map(Block::height).fold(0u16, u16::saturating_add)
    }

    fn height(&self) -> u16 {
        1u16.saturating_add(self.content()).saturating_add(u16::from(self.more))
    }

    /// Body lines the speech needs. `(MORE)` may hang one row into the bottom margin, and a
    /// continued cue at the top of a page sits one row above the body, as in Final Draft.
    fn body_height(&self, at_top: bool) -> u16 {
        self.height().saturating_sub(u16::from(self.more)).saturating_sub(u16::from(self.contd && at_top))
    }
}

#[derive(Clone, Debug)]
enum Group {
    Block(Block),
    Speech(Speech),
    Dual(Speech, Speech),
    Break,
}

impl Group {
    fn space_before(&self) -> u16 {
        match self {
            Group::Block(block) => block.space_before,
            Group::Speech(speech) | Group::Dual(speech, _) => speech.cue.space_before,
            Group::Break => 0,
        }
    }

    fn height(&self) -> u16 {
        match self {
            Group::Block(block) => block.height(),
            Group::Speech(speech) => speech.height(),
            Group::Dual(left, right) => left.height().max(right.height()),
            Group::Break => 0,
        }
    }

    /// Body lines the group needs, `at_top` of a page or not (see [`Speech::body_height`]).
    fn body_height(&self, at_top: bool) -> u16 {
        match self {
            Group::Speech(speech) => speech.body_height(at_top),
            other => other.height(),
        }
    }

    /// Scene headings, shots and cues without speech must not end a page.
    fn keeps_with_next(&self) -> bool {
        match self {
            Group::Block(block) => matches!(block.kind, ElementKind::SceneHeading | ElementKind::Shot | ElementKind::Character),
            _ => false,
        }
    }

    fn is_transition(&self) -> bool {
        matches!(self, Group::Block(block) if block.kind == ElementKind::Transition)
    }

    /// Fewest lines of this group that may start at the bottom of a page.
    fn min_start(&self) -> u16 {
        let space = self.space_before();
        let min = match self {
            Group::Block(block) if block.splittable && block.height() >= 4 => 2,
            Group::Speech(speech) if speech.content() >= 4 => 4,
            Group::Break => 0,
            other => other.height(),
        };
        space.saturating_add(min)
    }
}

/// Whether the cue at `index` gets an automatic ` (CONT'D)`: the same character spoke the
/// previous speech in this scene (with action or another element in between), and the cue does
/// not already say so. A scene heading starts afresh; dual-dialogue cues are left alone.
pub fn auto_contd_at(script: &Script, index: usize) -> bool {
    let Some(element) = script.elements.get(index).filter(|e| e.kind == ElementKind::Character) else { return false };
    let name = crate::model::character_name(&element.text).to_uppercase();
    // A cue that already carries a continued extension, such as `(CONT'D)` or `(cont’d)`.
    let marked = element.text.to_uppercase().replace(' ', "").contains("(CONT");
    if name.is_empty() || element.dual || marked {
        return false;
    }
    let next_cue =
        script.elements.get(index + 1..).unwrap_or(&[]).iter().find(|e| e.kind == ElementKind::Character || e.kind == ElementKind::SceneHeading);
    if next_cue.is_some_and(|e| e.kind == ElementKind::Character && e.dual) {
        return false;
    }
    for previous in script.elements.get(..index).unwrap_or(&[]).iter().rev() {
        match previous.kind {
            ElementKind::SceneHeading => return false,
            ElementKind::Character => return crate::model::character_name(&previous.text).eq_ignore_ascii_case(&name),
            _ => {}
        }
    }
    false
}

/// [`auto_contd_at`] for every element.
pub fn auto_contd(script: &Script) -> Vec<bool> {
    (0..script.elements.len()).map(|index| auto_contd_at(script, index)).collect()
}

fn block_for(script: &Script, index: usize, layout: &Layout, style: ElementStyle) -> Option<Block> {
    let element = script.elements.get(index)?;
    // An imported paragraph may override its element's indents and spacing.
    let style = match element.fdx.as_deref().filter(|f| f.kind == element.kind) {
        Some(fdx) => crate::fdx::paragraph_style(fdx, style),
        None => style,
    };
    let mut source = wrap::revised_chars(&element.text, element.kind.is_caps() || style.caps, &element.revised, &element.deletions);
    if layout.auto_contd && element.kind == ElementKind::Character && auto_contd_at(script, index) {
        source.extend(wrap::unstyled(&format!(" {}", layout.contd)));
    }
    if source.is_empty() && !element.text.trim().is_empty() {
        // Only notes: nothing to print.
        return None;
    }
    let splittable = matches!(element.kind, ElementKind::Action | ElementKind::Dialogue);
    let mut block = Block::new(index, element.kind, style, source, splittable);
    if element.kind == ElementKind::Lyrics && style == layout.dialogue {
        block.space_before = 0;
    }
    Some(block)
}

fn speech_at(script: &Script, start: usize, layout: &Layout, column: Option<&crate::layout::DualColumn>) -> (Speech, usize) {
    let cue_style = column.map_or(layout.character, |c| c.character);
    let cue = block_for(script, start, layout, cue_style).unwrap_or_else(|| Block::new(start, ElementKind::Character, cue_style, Vec::new(), false));
    let mut parts = Vec::new();
    let mut index = start + 1;
    while let Some(element) = script.elements.get(index) {
        let style = match element.kind {
            ElementKind::Parenthetical => column.map_or(layout.parenthetical, |c| c.parenthetical),
            ElementKind::Dialogue | ElementKind::Lyrics => column.map_or(layout.dialogue, |c| c.dialogue),
            _ => break,
        };
        if let Some(mut block) = block_for(script, index, layout, style) {
            block.space_before = 0;
            parts.push(block);
        }
        index += 1;
    }
    (Speech { cue, parts, contd: false, more: false }, index)
}

fn groups(script: &Script, layout: &Layout) -> Vec<Group> {
    let mut groups = Vec::new();
    // A lettered (multi-camera) script lists each scene's characters under its heading.
    let lettered = script.is_lettered();
    let mut index = 0;
    while let Some(element) = script.elements.get(index) {
        match element.kind {
            ElementKind::Section | ElementKind::Synopsis => index += 1,
            ElementKind::PageBreak => {
                groups.push(Group::Break);
                index += 1;
            }
            ElementKind::Character => {
                let followed_by_dual = |next: usize| script.elements.get(next).is_some_and(|e| e.kind == ElementKind::Character && e.dual);
                let (speech, next) = speech_at(script, index, layout, None);
                if !speech.parts.is_empty() && followed_by_dual(next) {
                    let (left, _) = speech_at(script, index, layout, Some(&layout.dual_left));
                    let (right, after) = speech_at(script, next, layout, Some(&layout.dual_right));
                    groups.push(Group::Dual(left, right));
                    index = after;
                } else if speech.parts.is_empty() {
                    groups.push(Group::Block(speech.cue));
                    index = next;
                } else {
                    groups.push(Group::Speech(speech));
                    index = next;
                }
            }
            kind => {
                // Imported types such as `New Act` or `General` use their own settings.
                let custom = element.fdx.as_ref().filter(|f| f.kind == kind).and_then(|f| layout.custom_style(&f.type_name));
                let style = custom.unwrap_or_else(|| layout.style(kind));
                // A new act starts a new page, as does any type set to (a break on an empty page
                // adds nothing).
                if kind == ElementKind::NewAct || style.new_page {
                    groups.push(Group::Break);
                }
                if let Some(mut block) = block_for(script, index, layout, style) {
                    let cast = if lettered && kind == ElementKind::SceneHeading { script.scene_cast(index) } else { Vec::new() };
                    if !cast.is_empty() {
                        let list = layout_lines(&wrap::unstyled(&format!("({})", cast.join(", "))), layout.action, false);
                        block.cast_lines = list.len();
                        block.lines.extend(list);
                    }
                    groups.push(Group::Block(block));
                }
                index += 1;
            }
        }
    }
    groups
}

/// Split an action block between sentences, at least two lines on each side, the first part
/// fitting in `avail` lines.
fn split_block(block: &Block, avail: u16) -> Option<(Block, Block)> {
    if !block.splittable {
        return None;
    }
    for (end, start) in wrap::sentence_breaks(&block.source).into_iter().rev() {
        let top = block.with_source(block.source.get(..end).unwrap_or(&[]).to_vec());
        if top.height() > avail || top.height() < 2 {
            continue;
        }
        let mut rest = block.rest_from(start);
        // Final Draft counts the lines left over in the paragraph as laid out, not re-wrapped.
        if rest.height() < 2 || block.height().saturating_sub(top.height()) < 2 {
            continue;
        }
        rest.space_before = 0;
        return Some((top, rest));
    }
    None
}

/// The two halves of a speech split at `part`, optionally inside that part's text.
fn speech_halves(speech: &Speech, part: usize, inner: Option<(usize, usize)>) -> (Speech, Speech) {
    let mut top = Speech { cue: speech.cue.clone(), parts: Vec::new(), contd: speech.contd, more: true };
    let mut rest = Speech { cue: speech.cue.clone(), parts: Vec::new(), contd: true, more: speech.more };
    for (index, block) in speech.parts.iter().enumerate() {
        match (index.cmp(&part), inner) {
            (std::cmp::Ordering::Less, _) => top.parts.push(block.clone()),
            (std::cmp::Ordering::Equal, Some((end, start))) => {
                top.parts.push(block.with_source(block.source.get(..end).unwrap_or(&[]).to_vec()));
                rest.parts.push(block.rest_from(start));
            }
            _ => rest.parts.push(block.clone()),
        }
    }
    (top, rest)
}

/// Split a speech between sentences or between its parts. Each side keeps at least
/// `min_lines` content lines, the top never ends with a parenthetical, and the top (with its
/// cue and `(MORE)`) fits in `avail` lines. The split keeping the most lines on top wins.
fn split_speech(speech: &Speech, avail: u16, min_lines: u16, at_top: bool) -> Option<(Speech, Speech)> {
    let mut best: Option<(u16, Speech, Speech)> = None;
    let mut consider = |top: Speech, rest: Speech| {
        let top_lines = top.content();
        // Final Draft lets a parenthetical end the top and counts it as a line.
        let valid = top_lines >= min_lines && rest.content() >= min_lines && top.body_height(at_top) <= avail;
        // On a tie the later split wins: it keeps more text on top (Final Draft does the same).
        if valid && best.as_ref().is_none_or(|(lines, _, _)| top_lines >= *lines) {
            best = Some((top_lines, top, rest));
        }
    };
    for (index, part) in speech.parts.iter().enumerate() {
        if index > 0 {
            let (top, rest) = speech_halves(speech, index, None);
            consider(top, rest);
        }
        if part.splittable {
            for split in wrap::sentence_breaks(&part.source) {
                let (top, rest) = speech_halves(speech, index, Some(split));
                // Inside a paragraph, at least two of its lines stay on top, and at least two
                // lines are left over counting the paragraph as laid out, not re-wrapped.
                let top_piece = top.parts.get(index).map_or(0, Block::height);
                let later: u16 = speech.parts.get(index + 1..).unwrap_or(&[]).iter().map(Block::height).sum();
                let leftover = part.height().saturating_sub(top_piece).saturating_add(later);
                if top_piece >= min_lines && leftover >= min_lines {
                    consider(top, rest);
                }
            }
        }
    }
    best.map(|(_, top, rest)| (top, rest))
}

/// Split a speech on line boundaries regardless of sentences (a last resort).
fn force_split_speech(speech: &Speech, avail: u16, at_top: bool) -> Option<(Speech, Speech)> {
    // The cue takes a body line unless it is a continued cue at the top; `(MORE)` hangs below.
    let budget = usize::from(avail.saturating_sub(u16::from(!(speech.contd && at_top))));
    if budget == 0 {
        return None;
    }
    let mut used = 0usize;
    for (index, part) in speech.parts.iter().enumerate() {
        let height = part.lines.len();
        if used + height > budget {
            let take = budget - used;
            let mut top =
                Speech { cue: speech.cue.clone(), parts: speech.parts.get(..index).unwrap_or(&[]).to_vec(), contd: speech.contd, more: true };
            let mut rest = Speech { cue: speech.cue.clone(), parts: Vec::new(), contd: true, more: speech.more };
            let (part_top, part_rest) = part.split_lines(take);
            if take > 0 {
                top.parts.push(part_top);
            }
            rest.parts.push(part_rest);
            rest.parts.extend(speech.parts.get(index + 1..).unwrap_or(&[]).iter().cloned());
            if top.parts.is_empty() {
                return None;
            }
            return Some((top, rest));
        }
        used += height;
    }
    None
}

#[derive(Clone)]
struct PageBuilder<'a> {
    layout: &'a Layout,
    lines: Vec<PageLine>,
    used: u16,
    /// Placed groups: (lines before, used before, group).
    items: Vec<(usize, u16, Group)>,
    /// The locked page's number, such as `12` or `12A`.
    label: Option<String>,
    /// Where the page's text starts (see [`Page::start`]).
    start: Option<(usize, u32)>,
    /// The locked numbers, which an A-page never takes.
    taken: std::rc::Rc<std::collections::HashSet<String>>,
}

impl<'a> PageBuilder<'a> {
    fn new(layout: &'a Layout, label: Option<String>) -> Self {
        PageBuilder { layout, lines: Vec::new(), used: 0, items: Vec::new(), label, start: None, taken: Default::default() }
    }

    fn avail(&self) -> u16 {
        self.layout.lines_per_page.saturating_sub(self.used)
    }

    fn push_line(&mut self, col: u16, chars: &[StyledChar], role: LineRole, element: Option<usize>, row_offset: u16) {
        let row = self.layout.body_top.saturating_add(self.used).saturating_add(row_offset);
        self.lines.push(PageLine::new(row, col, chars, role, element));
    }

    fn place_block(&mut self, block: &Block, script: &Script, element_pages: &mut [u32], page: u32) {
        let cast_from = block.lines.len().saturating_sub(block.cast_lines);
        for (index, line) in block.lines.iter().enumerate() {
            if index > 0 {
                self.used = self.used.saturating_add(block.style.spacing.max(1).saturating_sub(1));
            }
            let role = if index >= cast_from { LineRole::Cast } else { LineRole::Element(block.kind) };
            self.push_line(line.col, &line.chars, role, Some(block.element), 0);
            if index == 0 && block.kind == ElementKind::SceneHeading && self.layout.show_scene_numbers {
                self.scene_numbers(block.element, script);
            }
            self.used = self.used.saturating_add(1);
        }
        mark(element_pages, block.element, page);
    }

    fn scene_numbers(&mut self, element: usize, script: &Script) {
        let Some(number) = script.elements.get(element).and_then(|e| e.scene_number.as_deref()) else { return };
        let chars = wrap::unstyled(number);
        let len = u16::try_from(chars.len()).unwrap_or(u16::MAX);
        let left = self.layout.scene_number_left_end.saturating_sub(len);
        let (on_left, on_right) = script.scene_number_margins();
        if on_left {
            self.push_line(left, &chars, LineRole::SceneNumber, Some(element), 0);
        }
        if on_right {
            self.push_line(self.layout.scene_number_right, &chars, LineRole::SceneNumber, Some(element), 0);
        }
    }

    /// Lay out a speech starting at the current row; returns its height.
    fn speech_lines(&mut self, speech: &Speech, element_pages: &mut [u32], page: u32) -> u16 {
        let mut row: u16 = 0;
        let cue_above = speech.contd && self.used == 0;
        let mut cue = speech.cue.lines.first().map(|line| line.chars.clone()).unwrap_or_default();
        let cue_col = speech.cue.lines.first().map_or(speech.cue.style.left, |line| line.col);
        let role = if speech.contd { LineRole::Contd } else { LineRole::Element(ElementKind::Character) };
        if speech.contd && !cue.iter().map(|sc| sc.c).collect::<String>().contains(self.layout.contd.trim_matches(['(', ')'])) {
            cue.push(StyledChar::new(' ', Default::default()));
            cue.extend(wrap::unstyled(&self.layout.contd));
        }
        if cue_above {
            // One row above the body, like Final Draft's continued cue.
            let line_row = self.layout.body_top.saturating_sub(1);
            self.lines.push(PageLine::new(line_row, cue_col, &cue, role, Some(speech.cue.element)));
        } else {
            self.push_line(cue_col, &cue, role, Some(speech.cue.element), row);
            row += 1;
        }
        mark(element_pages, speech.cue.element, page);
        for part in &speech.parts {
            for (index, line) in part.lines.iter().enumerate() {
                if index > 0 {
                    row = row.saturating_add(part.style.spacing.max(1).saturating_sub(1));
                }
                self.push_line(line.col, &line.chars, LineRole::Element(part.kind), Some(part.element), row);
                row = row.saturating_add(1);
            }
            mark(element_pages, part.element, page);
        }
        if speech.more {
            // `(MORE)` may hang one row into the bottom margin, so it takes no body line.
            let more = wrap::unstyled(&self.layout.more);
            self.push_line(speech.cue.style.left, &more, LineRole::More, Some(speech.cue.element), row);
        }
        row
    }

    fn place(&mut self, group: Group, script: &Script, element_pages: &mut [u32], page: u32) {
        if self.start.is_none() {
            self.start = group_start(&group);
        }
        let lines_before = self.lines.len();
        let used_before = self.used;
        if self.used > 0 {
            self.used = self.used.saturating_add(group.space_before());
        }
        match &group {
            Group::Block(block) => self.place_block(block, script, element_pages, page),
            Group::Speech(speech) => {
                let height = self.speech_lines(speech, element_pages, page);
                self.used = self.used.saturating_add(height);
            }
            Group::Dual(left, right) => {
                let left_height = self.speech_lines(left, element_pages, page);
                let right_height = self.speech_lines(right, element_pages, page);
                self.used = self.used.saturating_add(left_height.max(right_height));
            }
            Group::Break => {}
        }
        self.items.push((lines_before, used_before, group));
    }

    /// Remove the last placed group so it can move to the next page.
    fn pop(&mut self) -> Option<Group> {
        let (lines_before, used_before, group) = self.items.pop()?;
        self.lines.truncate(lines_before);
        self.used = used_before;
        Some(group)
    }

    fn finish(self, number: u32) -> Page {
        let mut lines = self.lines;
        let printed = self.label.clone().unwrap_or_else(|| number.to_string());
        if printed != "1" {
            let label = wrap::unstyled(&format!("{printed}."));
            let len = u16::try_from(label.len()).unwrap_or(u16::MAX);
            lines.insert(
                0,
                PageLine::new(self.layout.page_number_row, self.layout.page_number_right.saturating_sub(len), &label, LineRole::PageNumber, None),
            );
        }
        lines.sort_by_key(|line| (line.row, line.col));
        Page { number, lines, locked: self.label, start: self.start }
    }
}

fn mark(element_pages: &mut [u32], element: usize, page: u32) {
    if let Some(slot) = element_pages.get_mut(element)
        && *slot == 0
    {
        *slot = page;
    }
}

/// The last page `page` holds, numbered after `pages`; the next page continues its locked
/// number with a letter (`12A` after `12`).
fn flush(page: &mut PageBuilder<'_>, pages: &mut Vec<Page>) {
    let number = u32::try_from(pages.len() + 1).unwrap_or(u32::MAX);
    let next = page.label.as_deref().map(|label| a_page_label(label, &page.taken));
    let taken = page.taken.clone();
    let done = std::mem::replace(page, PageBuilder { taken, ..PageBuilder::new(page.layout, next) });
    pages.push(done.finish(number));
}

/// Lays `queue` out from `page` on, flushing full pages into `pages`. With `one_page`, it stops
/// and returns false instead of starting a page.
fn fill(
    page: &mut PageBuilder<'_>,
    mut queue: VecDeque<Group>,
    script: &Script,
    pages: &mut Vec<Page>,
    element_pages: &mut [u32],
    one_page: bool,
) -> bool {
    let lpp = page.layout.lines_per_page.max(1);
    while let Some(group) = queue.pop_front() {
        if pages.len() >= MAX_PAGES {
            break;
        }
        let number = u32::try_from(pages.len() + 1).unwrap_or(u32::MAX);
        if matches!(group, Group::Break) {
            if !page.items.is_empty() {
                if one_page {
                    return false;
                }
                flush(page, pages);
            }
            continue;
        }
        let empty = page.items.is_empty();
        let space = if empty { 0 } else { group.space_before() };
        let height = group.body_height(empty);
        let avail = page.avail();
        let keep = if group.keeps_with_next() { queue.front().map_or(0, Group::min_start) } else { 0 };

        if space.saturating_add(height).saturating_add(keep) <= avail || (empty && height <= lpp) {
            page.place(group, script, element_pages, number);
            continue;
        }
        if space.saturating_add(height) <= avail && !empty {
            // Fits alone but would strand a heading or cue at the bottom.
            if one_page {
                return false;
            }
            flush(page, pages);
            queue.push_front(group);
            continue;
        }
        let room = avail.saturating_sub(space);
        let split = match &group {
            Group::Block(block) => split_block(block, room).map(|(top, rest)| (Group::Block(top), Group::Block(rest))),
            Group::Speech(speech) => split_speech(speech, room, 2, empty).map(|(top, rest)| (Group::Speech(top), Group::Speech(rest))),
            _ => None,
        };
        if let Some((top, rest)) = split {
            page.place(top, script, element_pages, number);
            if one_page {
                return false;
            }
            flush(page, pages);
            queue.push_front(rest);
            continue;
        }
        if empty {
            // Nothing else fits on an empty page: split anywhere, or overflow.
            let forced = match &group {
                Group::Block(block) => {
                    let (top, rest) = block.split_lines(usize::from(lpp));
                    Some((Group::Block(top), Group::Block(rest)))
                }
                Group::Speech(speech) => split_speech(speech, lpp, 1, true)
                    .or_else(|| force_split_speech(speech, lpp, true))
                    .map(|(top, rest)| (Group::Speech(top), Group::Speech(rest))),
                _ => None,
            };
            match forced {
                Some((top, rest)) => {
                    page.place(top, script, element_pages, number);
                    if one_page {
                        return false;
                    }
                    flush(page, pages);
                    queue.push_front(rest);
                }
                None => page.place(group, script, element_pages, number),
            }
            continue;
        }
        if group.is_transition() && page.items.len() >= 2 {
            let before_last_keeps = page.items.len().checked_sub(2).and_then(|at| page.items.get(at)).is_some_and(|(_, _, g)| g.keeps_with_next());
            if !before_last_keeps && let Some(previous) = page.pop() {
                if one_page {
                    return false;
                }
                flush(page, pages);
                queue.push_front(group);
                queue.push_front(previous);
                continue;
            }
        }
        if one_page {
            return false;
        }
        flush(page, pages);
        queue.push_front(group);
    }
    true
}

/// The page after a locked page that overflows: `12A` after `12`, `12B` after `12A`, `8A`
/// after the combined `7-8`.
fn next_label(label: &str) -> String {
    let label = label.rsplit('-').next().unwrap_or(label);
    let last = label.chars().last().filter(|c| ('A'..='Y').contains(c));
    match (last, last.and_then(|c| label.strip_suffix(c))) {
        (Some(c), Some(stem)) => format!("{stem}{}", char::from_u32(u32::from(c) + 1).unwrap_or('Z')),
        _ => format!("{label}A"),
    }
}

/// The A-page after `label`, never a number already locked: `12A` after `12`; but with `12A`
/// locked (Lock A-Pages), a page after `12` is `12AA`, then `12AB`.
fn a_page_label(label: &str, taken: &std::collections::HashSet<String>) -> String {
    let mut candidate = next_label(label);
    let mut letters = String::from("A");
    while taken.contains(&candidate) && letters.len() < 8 {
        candidate = format!("{}{letters}", label.rsplit('-').next().unwrap_or(label));
        letters.push('A');
    }
    candidate
}

/// `7` and `8` printed as one page: `7-8`; `7-8` and `9`: `7-9`.
fn combined_label(first: &str, last: &str) -> String {
    format!("{}-{last}", first.split('-').next().unwrap_or(first))
}

/// The first and last element a group prints, or `None` for a page break.
fn group_span(group: &Group) -> Option<(usize, usize)> {
    let speech_span = |speech: &Speech| (speech.cue.element, speech.parts.last().map_or(speech.cue.element, |p| p.element));
    match group {
        Group::Block(block) => Some((block.element, block.element)),
        Group::Speech(speech) => Some(speech_span(speech)),
        Group::Dual(left, right) => Some((speech_span(left).0, speech_span(right).1)),
        Group::Break => None,
    }
}

/// Where a page that opens with `group` starts: a speech continued from the page before starts
/// in its first part.
fn group_start(group: &Group) -> Option<(usize, u32)> {
    match group {
        Group::Block(block) => Some((block.element, block.from)),
        Group::Speech(speech) if speech.contd => speech.parts.first().map(|part| (part.element, part.from)),
        Group::Speech(speech) | Group::Dual(speech, _) => Some((speech.cue.element, 0)),
        Group::Break => None,
    }
}

/// Where a locked page start falls in a group.
enum Cut {
    /// The page starts with the group.
    Before,
    /// The page starts inside it: the part before, the part after, and how many printed
    /// characters of the element the part after leaves out.
    Inside(Box<(Group, Group)>, u32),
}

/// Cuts `group` where a page starts `at` printed characters into element `element`, which the
/// group prints.
fn cut(group: &Group, element: usize, at: u32) -> Cut {
    let at_usize = usize::try_from(at).unwrap_or(usize::MAX);
    // The part after starts past the spaces at the cut, and the part before ends before them.
    let after = |source: &[StyledChar]| at_usize + source.get(at_usize..).unwrap_or(&[]).iter().take_while(|sc| sc.c == ' ').count();
    let before = |source: &[StyledChar]| at_usize - source.get(..at_usize).unwrap_or(&[]).iter().rev().take_while(|sc| sc.c == ' ').count();
    match group {
        Group::Block(block) if at > 0 && at_usize < block.source.len() => {
            let start = after(&block.source);
            let top = block.with_source(block.source.get(..before(&block.source)).unwrap_or(&[]).to_vec());
            let mut rest = block.rest_from(start);
            rest.space_before = 0;
            Cut::Inside(Box::new((Group::Block(top), Group::Block(rest))), u32::try_from(start).unwrap_or(u32::MAX))
        }
        Group::Speech(speech) => {
            let Some(part) = speech.parts.iter().position(|p| p.element == element) else { return Cut::Before };
            let source = speech.parts.get(part).map_or(&[][..], |p| p.source.as_slice());
            if at == 0 && part > 0 {
                let (top, rest) = speech_halves(speech, part, None);
                return Cut::Inside(Box::new((Group::Speech(top), Group::Speech(rest))), 0);
            }
            if at == 0 || at_usize >= source.len() {
                return Cut::Before;
            }
            let start = after(source);
            let (top, rest) = speech_halves(speech, part, Some((before(source), start)));
            Cut::Inside(Box::new((Group::Speech(top), Group::Speech(rest))), u32::try_from(start).unwrap_or(u32::MAX))
        }
        _ => Cut::Before,
    }
}

/// The script's groups cut where each locked page starts: the groups before the first locked
/// page (normally none), then each locked page's groups with its number.
fn locked_segments(script: &Script, groups: Vec<Group>) -> Vec<(Option<String>, Vec<Group>)> {
    let mut starts: Vec<(usize, u32, &str)> = script
        .elements
        .iter()
        .enumerate()
        .flat_map(|(index, element)| element.page_starts.iter().map(move |p| (index, p.at, p.label.as_str())))
        .collect();
    starts.sort_by_key(|&(element, at, _)| (element, at));
    let mut starts = starts.into_iter().peekable();
    let mut segments: Vec<(Option<String>, Vec<Group>)> = vec![(None, Vec::new())];
    let open = |segments: &mut Vec<(Option<String>, Vec<Group>)>, label: &str| {
        // A page break that ended the last page goes with the new one.
        let moved = segments.last_mut().and_then(|(_, groups)| if matches!(groups.last(), Some(Group::Break)) { groups.pop() } else { None });
        segments.push((Some(label.to_string()), moved.into_iter().collect()));
    };
    // Printed characters of an element already left on earlier pages, when a page started inside it.
    let mut taken: (usize, u32) = (usize::MAX, 0);
    let mut queue: VecDeque<Group> = groups.into();
    while let Some(group) = queue.pop_front() {
        let Some((first, last)) = group_span(&group) else {
            if let Some((_, groups)) = segments.last_mut() {
                groups.push(group);
            }
            continue;
        };
        match starts.peek().copied() {
            Some((element, at, label)) if element <= last => {
                starts.next();
                let at = if taken.0 == element { at.saturating_sub(taken.1) } else { at };
                let before = element < first || (element == first && at == 0);
                match if before { Cut::Before } else { cut(&group, element, at) } {
                    Cut::Before => {
                        open(&mut segments, label);
                        queue.push_front(group);
                    }
                    Cut::Inside(parts, consumed) => {
                        let (top, rest) = *parts;
                        if let Some((_, groups)) = segments.last_mut() {
                            groups.push(top);
                        }
                        open(&mut segments, label);
                        taken = if taken.0 == element { (element, taken.1.saturating_add(consumed)) } else { (element, consumed) };
                        queue.push_front(rest);
                    }
                }
            }
            _ => {
                if let Some((_, groups)) = segments.last_mut() {
                    groups.push(group);
                }
            }
        }
    }
    // Pages locked after the last printed text stay, empty.
    for (_, _, label) in starts {
        open(&mut segments, label);
    }
    segments
}

/// Paginates a script whose pages are locked: each locked page starts where it did; what no
/// longer fits flows onto A-pages (`12A`), a page whose text has gone stays short, and a page
/// whose text all fits on the page before prints with it as one page (`7-8`).
fn paginate_locked(script: &Script, layout: &Layout, groups: Vec<Group>, pages: &mut Vec<Page>, element_pages: &mut [u32]) {
    let taken: std::rc::Rc<std::collections::HashSet<String>> =
        std::rc::Rc::new(script.elements.iter().flat_map(|e| &e.page_starts).map(|p| p.label.clone()).collect());
    let mut page = PageBuilder::new(layout, None);
    // Whether `page` is a locked page itself, not one of its A-pages.
    let mut on_locked = false;
    for (label, segment) in locked_segments(script, groups) {
        let Some(label) = label else {
            fill(&mut page, segment.into(), script, pages, element_pages, false);
            continue;
        };
        if on_locked && pages.len() < MAX_PAGES {
            let mut trial = page.clone();
            let mut trial_pages = element_pages.to_vec();
            if fill(&mut trial, segment.clone().into(), script, pages, &mut trial_pages, true) {
                trial.label = Some(combined_label(page.label.as_deref().unwrap_or_default(), &label));
                page = trial;
                element_pages.copy_from_slice(&trial_pages);
                continue;
            }
        }
        if !page.items.is_empty() || page.label.is_some() && on_locked {
            flush(&mut page, pages);
        }
        page = PageBuilder { taken: taken.clone(), ..PageBuilder::new(layout, Some(label)) };
        let before = pages.len();
        fill(&mut page, segment.into(), script, pages, element_pages, false);
        on_locked = pages.len() == before;
    }
    if !page.items.is_empty() || on_locked {
        flush(&mut page, pages);
    }
}

/// Production > Lock Pages: each page of `paginated` (the script laid out as it prints now) is
/// locked where it starts, under the number it prints with (docs/production-spec.md §4.2). Any
/// locks the script had are replaced. Returns how many pages were locked.
pub fn lock_pages(script: &mut Script, paginated: &Paginated) -> usize {
    script.unlock_pages();
    let mut count = 0;
    for page in &paginated.pages {
        let Some((element, at)) = page.start else { continue };
        let Some(element) = script.elements.get_mut(element) else { continue };
        element.page_starts.push(crate::model::PageStart { at, label: page.label(), ..Default::default() });
        count += 1;
    }
    count
}

/// Production > Lock A-Pages: the A-pages `paginated` (the script as it prints now, locked) has
/// are locked too, as a new level, so pages added later never take their numbers. Returns how
/// many were locked.
pub fn lock_a_pages(script: &mut Script, paginated: &Paginated) -> usize {
    let taken: std::collections::HashSet<String> = script.elements.iter().flat_map(|e| &e.page_starts).map(|p| p.label.clone()).collect();
    let level = script.lock_level() + 1;
    let mut count = 0;
    for page in &paginated.pages {
        let label = page.label();
        if page.locked.is_none() || taken.contains(&label) || label.contains('-') {
            continue;
        }
        let Some((element, at)) = page.start else { continue };
        let Some(element) = script.elements.get_mut(element) else { continue };
        element.page_starts.push(crate::model::PageStart { at, label, level, ..Default::default() });
        element.page_starts.sort_by_key(|p| p.at);
        count += 1;
    }
    count
}

/// Paginate a script. A script with locked pages keeps them (docs/production-spec.md §4.1),
/// unless they came from an `.fdx` file: where Final Draft's `Position` puts a page that starts
/// inside a paragraph is not read reliably yet, so those pages are kept and written back but
/// paginate as unlocked (docs/research/locked-pages.md).
pub fn paginate(script: &Script, layout: &Layout) -> Paginated {
    let honoured = script.pages_locked() && script.elements.iter().flat_map(|e| &e.page_starts).all(|p| p.fdx_attributes.is_empty());
    paginate_with(script, layout, honoured)
}

/// [`paginate`], keeping the script's locked pages or not.
pub fn paginate_with(script: &Script, layout: &Layout, keep_locks: bool) -> Paginated {
    let groups = groups(script, layout);
    let mut element_pages = vec![0u32; script.elements.len()];
    let mut pages: Vec<Page> = Vec::new();
    if keep_locks && script.pages_locked() {
        paginate_locked(script, layout, groups, &mut pages, &mut element_pages);
    } else {
        let mut page = PageBuilder::new(layout, None);
        fill(&mut page, groups.into(), script, &mut pages, &mut element_pages, false);
        if !page.items.is_empty() {
            flush(&mut page, &mut pages);
        }
    }

    let scenes = script
        .elements
        .iter()
        .enumerate()
        .filter(|(_, element)| element.kind == ElementKind::SceneHeading)
        .map(|(index, element)| SceneInfo {
            element: index,
            number: element.scene_number.clone(),
            heading: crate::inline::plain(&element.text).to_uppercase(),
            page: element_pages.get(index).copied().unwrap_or(0),
        })
        .collect();
    Paginated { title_page: title_page(&script.title_page, layout), pages, scenes, element_pages }
}

/// Where each scene starts and how long it is, as Final Draft records them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SceneLength {
    /// Index of the scene heading element.
    pub element: usize,
    /// Page the scene starts on.
    pub page: u32,
    /// Length in eighths of a page (`10` is `1 2/8`). An omitted scene is `0`.
    pub eighths: u32,
}

/// Each scene's start page and length in eighths. The length counts the scene's lines on each
/// page it touches (blank lines between them included, empty page bottoms not), plus the blank
/// lines before its own heading, rounded up to the next eighth. Measured against Final Draft's
/// recorded lengths, this is exact for 96.7% of the ground-truth scenes and 1/8 long for the
/// rest (docs/research/pagination-calibration.md).
pub fn scene_lengths(script: &Script, paginated: &Paginated, layout: &Layout) -> Vec<SceneLength> {
    let lpp = u32::from(layout.lines_per_page.max(1));
    let top = u32::from(layout.body_top);
    // Per scene: (heading element, start page, [(page, first row, last row)]).
    type Rows = Vec<(usize, u32, u32)>;
    let mut scenes: Vec<(usize, u32, Rows)> = Vec::new();
    for (index, page) in paginated.pages.iter().enumerate() {
        for line in &page.lines {
            let LineRole::Element(kind) = line.role else { continue };
            let row = u32::from(line.row).saturating_sub(top);
            if kind == ElementKind::SceneHeading
                && let Some(element) = line.element
                && scenes.last().is_none_or(|(last, ..)| *last != element)
            {
                scenes.push((element, page.number, Vec::new()));
            }
            let Some((.., parts)) = scenes.last_mut() else { continue };
            match parts.last_mut() {
                Some(part) if part.0 == index => {
                    part.1 = part.1.min(row);
                    part.2 = part.2.max(row);
                }
                _ => parts.push((index, row, row)),
            }
        }
    }
    scenes
        .into_iter()
        .map(|(element, page, parts)| {
            let heading = script.elements.get(element);
            // No length for a scene omitted here, or under Final Draft's omitted-scene marker; a
            // heading that only reads OMITTED is measured like any other, as Final Draft does.
            let omitted = heading.is_some_and(|h| h.omitted.is_some() || h.fdx.as_deref().is_some_and(|f| f.children.contains("<OmittedScene")));
            let style = match heading.and_then(|h| h.fdx.as_deref()).filter(|f| f.kind == ElementKind::SceneHeading) {
                Some(fdx) => crate::fdx::paragraph_style(fdx, layout.scene_heading),
                None => layout.scene_heading,
            };
            let lines: u32 = parts.iter().map(|(_, first, last)| last - first + 1).sum::<u32>() + u32::from(style.space_before);
            let eighths = if omitted { 0 } else { (lines * 8).div_ceil(lpp) };
            SceneLength { element, page, eighths }
        })
        .collect()
}

/// Eighths as Final Draft writes them: `3/8`, `1`, `1 2/8`, and `0/8` for none.
pub fn format_eighths(eighths: u32) -> String {
    match (eighths / 8, eighths % 8) {
        (0, fraction) => format!("{fraction}/8"),
        (whole, 0) => whole.to_string(),
        (whole, fraction) => format!("{whole} {fraction}/8"),
    }
}

/// Lay out the title page, if the script has one.
pub fn title_page(title: &TitlePage, layout: &Layout) -> Option<Page> {
    if title.is_empty() {
        return None;
    }
    let mut lines = Vec::new();
    let center = ElementStyle::new(layout.action.left, layout.action.width, Align::Center, 0);
    let mut row: u16 = 22;
    let centered = |value: Option<&str>, gap_after: u16, row: &mut u16, lines: &mut Vec<PageLine>| {
        let Some(value) = value.filter(|v| !v.trim().is_empty()) else { return };
        for line in layout_lines(&wrap::styled_chars(value, false), center, false) {
            lines.push(PageLine::new(*row, line.col, &line.chars, LineRole::Title, None));
            *row = row.saturating_add(1);
        }
        *row = row.saturating_add(gap_after);
    };
    centered(title.get("Title"), 3, &mut row, &mut lines);
    centered(title.get("Credit"), 1, &mut row, &mut lines);
    centered(title.get("Author"), 3, &mut row, &mut lines);
    centered(title.get("Source"), 0, &mut row, &mut lines);

    let bottom = layout.body_top.saturating_add(layout.lines_per_page).saturating_sub(1);
    let left_block: Vec<&str> = ["Notes", "Copyright", "Contact"].iter().filter_map(|key| title.get(key)).collect();
    let left_text = left_block.join("\n\n");
    let left_style = ElementStyle::new(layout.action.left, 35, Align::Left, 0);
    let left_lines = layout_lines(&wrap::styled_chars(&left_text, false), left_style, false);
    let start = bottom.saturating_sub(u16::try_from(left_lines.len()).unwrap_or(0)).saturating_add(1);
    if !left_text.is_empty() {
        for (index, line) in left_lines.into_iter().enumerate() {
            let row = start.saturating_add(u16::try_from(index).unwrap_or(0));
            lines.push(PageLine::new(row, line.col, &line.chars, LineRole::Title, None));
        }
    }
    if let Some(date) = title.get("Draft date") {
        let right_style = ElementStyle::new(layout.page_number_right.saturating_sub(25), 25, Align::Right, 0);
        let date_lines = layout_lines(&wrap::styled_chars(date, false), right_style, false);
        let start = bottom.saturating_sub(u16::try_from(date_lines.len()).unwrap_or(0)).saturating_add(1);
        for (index, line) in date_lines.into_iter().enumerate() {
            let row = start.saturating_add(u16::try_from(index).unwrap_or(0));
            lines.push(PageLine::new(row, line.col, &line.chars, LineRole::Title, None));
        }
    }
    lines.sort_by_key(|line| (line.row, line.col));
    Some(Page { number: 0, lines, locked: None, start: None })
}

/// Render pages as plain text grids (one character per cell), for golden tests and the CLI.
pub fn render_text(paginated: &Paginated) -> String {
    let mut out = String::new();
    let all = paginated.title_page.iter().chain(paginated.pages.iter());
    for page in all {
        if page.number == 0 {
            out.push_str("==== title page ====\n");
        } else {
            out.push_str(&format!("==== page {} ====\n", page.label()));
        }
        let mut grid: Vec<Vec<char>> = Vec::new();
        for line in &page.lines {
            let row = usize::from(line.row).min(500);
            while grid.len() <= row {
                grid.push(Vec::new());
            }
            if let Some(cells) = grid.get_mut(row) {
                for (col, c) in (usize::from(line.col).min(500)..).zip(line.text().chars()) {
                    while cells.len() <= col {
                        cells.push(' ');
                    }
                    if let Some(cell) = cells.get_mut(col) {
                        *cell = c;
                    }
                }
            }
        }
        for row in grid {
            let text: String = row.into_iter().collect();
            out.push_str(text.trim_end());
            out.push('\n');
        }
    }
    out
}
