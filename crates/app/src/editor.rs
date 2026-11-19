//! The script view: each element at its industry margin on white paper, drawn and edited by
//! the editor itself (see `text_field`): wrapped where it prints, the markup hidden, capitals
//! where its type prints them. Page view shows separate sheets with their page numbers; draft
//! view one continuous column with page-break marks. Enter, Tab, Backspace-at-start and arrows
//! at the edges become engine commands; typing is committed with `element.set_text`.

use std::sync::Arc;

use egui::text::TextFormat;
use egui::{Align, Color32, FontData, FontDefinitions, FontFamily, FontId, Frame, Id, Key, Modifiers, Order, Rect, Sense, Stroke, Ui, pos2, vec2};
use storyteller_core::layout::{Align as PageAlign, POINTS_PER_COLUMN, REVISION_MARK_INCHES, WRAP_ADVANCE_POINTS};
use storyteller_core::model::{Revisions, ShowRevisions};
use storyteller_core::{Element, ElementKind, autocomplete, inline, paginate};
use storyteller_engine::flow;
use storyteller_engine::{Caret, Session};

use crate::automation::Automation;
use crate::text_field::{Geometry, Laid, Look, ShowAs, Shown, ShownChar, replace};

/// Font family name for Courier Prime.
pub const COURIER: &str = "courier_prime";
/// Its bold, italic and bold italic faces, for styled text.
const COURIER_BOLD: &str = "courier_prime_bold";
const COURIER_ITALIC: &str = "courier_prime_italic";
const COURIER_BOLD_ITALIC: &str = "courier_prime_bold_italic";
/// Pixels per inch at 100 % zoom.
const PX_PER_INCH: f32 = 96.0;
/// Space between sheets in page view, in rows.
const SHEET_GAP_ROWS: f32 = 1.5;

pub fn install_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    for (name, data) in [
        (COURIER, storyteller_pdf::COURIER_PRIME_REGULAR),
        (COURIER_BOLD, storyteller_pdf::COURIER_PRIME_BOLD),
        (COURIER_ITALIC, storyteller_pdf::COURIER_PRIME_ITALIC),
        (COURIER_BOLD_ITALIC, storyteller_pdf::COURIER_PRIME_BOLD_ITALIC),
    ] {
        fonts.font_data.insert(name.into(), Arc::new(FontData::from_static(data)));
        fonts.families.insert(FontFamily::Name(name.into()), vec![name.into()]);
    }
    ctx.set_fonts(fonts);
}

/// Autocomplete popup state for the focused element.
#[derive(Default)]
pub struct Completion {
    pub index: usize,
    pub text: String,
    pub items: Vec<String>,
    pub selected: usize,
    pub dismissed: bool,
}

impl Completion {
    pub fn visible_for(&self, index: usize) -> bool {
        self.index == index && !self.dismissed && !self.items.is_empty()
    }
}

/// The element picker Enter opens on an empty element.
pub struct Picker {
    pub index: usize,
    pub selected: usize,
}

/// A selection across paragraphs. Within one paragraph the text field keeps the selection itself.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Span {
    /// Where it started: this end stays put.
    pub anchor: Caret,
    /// Where it has got to: Shift and an arrow, or the pointer, move this end.
    pub head: Caret,
    /// The script's edit count when it was made: any edit since ends it.
    edits: u64,
    /// The column Shift+Up and Shift+Down keep to, on screen.
    column: Option<f32>,
}

impl Span {
    #[cfg(test)]
    pub fn between(anchor: Caret, head: Caret, edits: u64) -> Self {
        Span { anchor, head, edits, column: None }
    }

    /// Its ends in script order.
    pub fn ordered(&self) -> (Caret, Caret) {
        if (self.anchor.element, self.anchor.offset) <= (self.head.element, self.head.offset) {
            (self.anchor, self.head)
        } else {
            (self.head, self.anchor)
        }
    }

    /// The bytes of element `index` inside it.
    fn bytes_in(&self, index: usize, text: &str) -> Option<(usize, usize)> {
        let (from, to) = self.ordered();
        if index < from.element || index > to.element {
            return None;
        }
        let start = if index == from.element { from.offset } else { 0 };
        let end = if index == to.element { to.offset } else { text.len() };
        Some((start, end))
    }
}

/// A paragraph as drawn last, to find places in by position.
#[derive(Clone)]
struct Field {
    index: usize,
    laid: Arc<Laid>,
}

impl Field {
    fn rect(&self) -> Rect {
        self.laid.rect
    }
}

/// The place in the script nearest `pos`, reading rows top to bottom. Going up, a position
/// between two paragraphs counts as in the one above.
fn place_at(fields: &[Field], pos: egui::Pos2, up: bool) -> Option<Caret> {
    let found = if up { fields.iter().rfind(|f| f.rect().top() <= pos.y) } else { fields.iter().find(|f| f.rect().bottom() >= pos.y) };
    let field = found.or(if up { fields.first() } else { fields.last() })?;
    let shown = &field.laid.shown;
    Some(Caret { element: field.index, offset: shown.to_stored(field.laid.place_at(pos)) })
}

/// One row up or down from `head`, in the column `x`, into the next paragraph at its edge.
fn row_from(fields: &[Field], head: Caret, x: Option<f32>, up: bool) -> Option<(Caret, f32)> {
    let field = fields.iter().find(|f| f.index == head.element)?;
    let here = field.laid.caret_rect(field.laid.shown.place_of(head.offset));
    let x = x.unwrap_or(here.center().x);
    let y = if up { here.min.y - here.height() * 0.5 } else { here.max.y + here.height() * 0.5 };
    Some((place_at(fields, pos2(x, y), up)?, x))
}

/// One character on from `head`, over the paragraph's edge into the next.
fn step_from(session: &Session, head: Caret, back: bool) -> Caret {
    let text = session.element(head.element).map_or("", |e| e.text.as_str());
    if back {
        match text.get(..head.offset).and_then(|t| t.char_indices().last()) {
            Some((at, _)) => Caret { element: head.element, offset: at },
            None => match (0..head.element).rfind(|&i| session.is_shown(i)) {
                Some(element) => Caret { element, offset: session.element(element).map_or(0, |e| e.text.len()) },
                None => head,
            },
        }
    } else {
        match text.get(head.offset..).and_then(|t| t.chars().next()) {
            Some(c) => Caret { element: head.element, offset: head.offset + c.len_utf8() },
            None => match (head.element + 1..session.script.elements.len()).find(|&i| session.is_shown(i)) {
                Some(element) => Caret { element, offset: 0 },
                None => head,
            },
        }
    }
}

/// How far beyond the view paragraphs are still laid out, in rows, so a wheel or key that moves a
/// little has text ready.
const OVERSCAN_ROWS: f32 = 30.0;

/// What a paragraph's height depends on, as a hash.
fn height_key(element: &Element, zoom: f32, style_width: u16, show: ShowAs, spacing: u16) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    element.text.hash(&mut hasher);
    element.kind.hash(&mut hasher);
    zoom.to_bits().hash(&mut hasher);
    style_width.hash(&mut hasher);
    show.hash(&mut hasher);
    spacing.hash(&mut hasher);
    hasher.finish()
}

/// The caret and selection in one paragraph, as bytes of its stored text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cursor {
    pub index: usize,
    /// Where the selection started: this end stays put.
    pub anchor: usize,
    /// Where the caret is.
    pub head: usize,
}

impl Cursor {
    pub fn at(index: usize, offset: usize) -> Self {
        Cursor { index, anchor: offset, head: offset }
    }

    /// The selection's bytes in order.
    pub fn ordered(&self) -> (usize, usize) {
        (self.anchor.min(self.head), self.anchor.max(self.head))
    }
}

#[derive(Default)]
pub struct EditorState {
    /// Each paragraph's height when last laid out, and what it depended on. A paragraph off
    /// screen with a known height only takes up its space.
    heights: Vec<Option<(u64, f32)>>,
    /// The selection across paragraphs, when there is one.
    pub span: Option<Span>,
    /// Where the pointer went down in a paragraph, while it stays down.
    drag_from: Option<Caret>,
    /// Each paragraph's text as drawn last frame.
    fields: Vec<Field>,
    /// Open while choosing an element for an empty paragraph.
    pub picker: Option<Picker>,
    /// The element that had focus last frame, to give it back after Esc.
    last_focused: Option<usize>,
    /// Focus this element at this byte offset on the next frame.
    pub pending_focus: Option<(usize, usize)>,
    pub focused: Option<usize>,
    /// The caret and selection in the paragraph with the keyboard, kept when a menu takes it.
    pub cursor: Option<Cursor>,
    /// Text the input method is composing in a paragraph, not yet typed.
    composing: Option<(usize, String)>,
    /// When the caret last moved or the text changed, for its blink.
    last_interaction: f64,
    /// The column Up and Down keep to inside a paragraph, on screen.
    column: Option<f32>,
    /// View > Show Formatting Marks: the markup on screen, faint.
    pub show_markup: bool,
    /// The current find match, `(element, start, end)` in bytes, drawn highlighted.
    pub highlight: Option<(usize, usize, usize)>,
    /// Misspelled words to underline, by element: byte ranges (filled by the app each frame).
    pub misspelled: std::collections::HashMap<usize, Vec<(usize, usize)>>,
    /// Scroll this element into view on the next frame (a find match) without focusing it.
    pub scroll_to: Option<usize>,
    scroll_to_focus: bool,
    first_row: bool,
    last_row: bool,
    pub completion: Completion,
}

impl EditorState {
    #[cfg(test)]
    pub fn field_rect(&self, index: usize) -> Option<Rect> {
        self.fields.iter().find(|f| f.index == index).map(Field::rect)
    }

    /// Selects the whole script when it has more than one paragraph.
    pub fn select_whole_script(&mut self, session: &Session) -> bool {
        let count = session.script.elements.len();
        if count < 2 {
            return false;
        }
        let end = session.element(count - 1).map_or(0, |e| e.text.len());
        self.span = Some(Span {
            anchor: Caret { element: 0, offset: 0 },
            head: Caret { element: count - 1, offset: end },
            edits: session.edits(),
            column: None,
        });
        true
    }
}

/// What the view asks the app to do after drawing.
pub enum EditAction {
    SetText {
        index: usize,
        text: String,
        caret: usize,
    },
    Caret {
        index: usize,
        offset: usize,
    },
    Enter {
        index: usize,
        offset: usize,
    },
    Tab {
        index: usize,
        back: bool,
    },
    MergePrevious {
        index: usize,
    },
    Focus {
        index: usize,
        offset: usize,
    },
    Accept {
        index: usize,
        text: String,
        then_enter: bool,
    },
    /// Chosen in the element picker.
    Pick {
        index: usize,
        kind: ElementKind,
    },
    /// Screenplay text in several paragraphs pasted over `start..end` (bytes): it becomes elements.
    PasteScreenplay {
        index: usize,
        start: usize,
        end: usize,
        text: String,
    },
    /// The selection across paragraphs on the clipboard.
    CopyRange {
        from: Caret,
        to: Caret,
    },
    /// The selection across paragraphs replaced by `text`, which may be nothing.
    ReplaceRange {
        from: Caret,
        to: Caret,
        text: String,
    },
}

/// A selection across paragraphs has the keyboard ahead of the text fields: what is typed
/// replaces it, Copy and Cut take all of it, and the arrows move or collapse it.
fn span_keys(ui: &mut Ui, session: &Session, editor: &mut EditorState, actions: &mut Vec<EditAction>) {
    let Some(mut span) = editor.span else { return };
    // The find bar or a dialog has the keyboard: what is typed there is its own.
    let count = session.script.elements.len();
    let elsewhere = ui.memory(|m| m.focused()).is_some_and(|id| !(0..count).any(|n| id == Id::unique(("storyteller.element", n))));
    if elsewhere {
        return;
    }
    let (from, to) = span.ordered();
    let mut end = false;
    let mut moved = false;
    let mut caret = None;
    ui.input_mut(|input| {
        let events = std::mem::take(&mut input.events);
        for event in events {
            match event {
                egui::Event::Copy => actions.push(EditAction::CopyRange { from, to }),
                egui::Event::Cut => {
                    actions.push(EditAction::CopyRange { from, to });
                    actions.push(EditAction::ReplaceRange { from, to, text: String::new() });
                    end = true;
                }
                egui::Event::Paste(text) | egui::Event::Text(text) | egui::Event::Ime(egui::ImeEvent::Commit(text)) => {
                    actions.push(EditAction::ReplaceRange { from, to, text });
                    end = true;
                }
                egui::Event::Key { key, physical_key, pressed: true, repeat, modifiers } if !modifiers.command && !modifiers.ctrl => match key {
                    Key::Backspace | Key::Delete => {
                        actions.push(EditAction::ReplaceRange { from, to, text: String::new() });
                        end = true;
                    }
                    Key::Enter => {
                        actions.push(EditAction::ReplaceRange { from, to, text: String::new() });
                        actions.push(EditAction::Enter { index: from.element, offset: from.offset });
                        end = true;
                    }
                    Key::Escape => caret = Some(span.head),
                    Key::ArrowLeft | Key::ArrowRight | Key::ArrowUp | Key::ArrowDown if modifiers.shift => {
                        let up = key == Key::ArrowUp;
                        if up || key == Key::ArrowDown {
                            if let Some((head, x)) = row_from(&editor.fields, span.head, span.column, up) {
                                span.head = head;
                                span.column = Some(x);
                            }
                        } else {
                            span.head = step_from(session, span.head, key == Key::ArrowLeft);
                            span.column = None;
                        }
                        moved = true;
                    }
                    Key::ArrowLeft | Key::ArrowUp => caret = Some(from),
                    Key::ArrowRight | Key::ArrowDown => caret = Some(to),
                    Key::Tab => {}
                    _ => input.events.push(egui::Event::Key { key, physical_key, pressed: true, repeat, modifiers }),
                },
                other => input.events.push(other),
            }
        }
    });
    if end {
        editor.span = None;
    } else if let Some(caret) = caret {
        editor.span = None;
        editor.pending_focus = Some((caret.element, caret.offset));
    } else if moved {
        settle(ui, editor, span);
    }
}

/// Keeps `span`, or, back inside one paragraph, gives the selection to that paragraph's field.
fn settle(ui: &Ui, editor: &mut EditorState, span: Span) {
    if span.anchor.element != span.head.element {
        editor.span = Some(span);
        return;
    }
    editor.span = None;
    let id = Id::unique(("storyteller.element", span.head.element));
    editor.cursor = Some(Cursor { index: span.head.element, anchor: span.anchor.offset, head: span.head.offset });
    ui.memory_mut(|m| m.request_focus(id));
}

pub fn char_to_byte(text: &str, chars: usize) -> usize {
    text.char_indices().nth(chars).map_or(text.len(), |(byte, _)| byte)
}

pub fn byte_to_char(text: &str, byte: usize) -> usize {
    text.char_indices().take_while(|(b, _)| *b < byte).count()
}

/// A revision shown in an element: the bytes of its revised text, or of the character a
/// deletion mark sits beside, with the set's id.
#[derive(Clone, Copy)]
struct ShownRevision {
    start: usize,
    end: usize,
    set: u32,
    deletion: bool,
}

/// An element's revisions that `show` shows, from its spans of printed characters.
fn shown_revisions(element: &Element, revisions: &Revisions, show: ShowRevisions) -> Vec<ShownRevision> {
    let shows = |set: u32| revisions.set(set).is_some() && show.shows(set, revisions.active);
    if show == ShowRevisions::None || (element.revised.is_empty() && element.deletions.is_empty()) {
        return Vec::new();
    }
    let printed = inline::printed_bytes(&element.text);
    let place = |at: u32| usize::try_from(at).ok().map(|at| at.min(printed.len()));
    let mut out = Vec::new();
    for span in element.revised.iter().filter(|r| shows(r.set)) {
        let (Some(from), Some(to)) = (place(span.from), place(span.to)) else { continue };
        if let (Some(first), Some(last)) = (printed.get(from), to.checked_sub(1).and_then(|i| printed.get(i)))
            && first.0 < last.1
        {
            out.push(ShownRevision { start: first.0, end: last.1, set: span.set, deletion: false });
        }
    }
    for deletion in element.deletions.iter().filter(|d| shows(d.set)) {
        // The character before the place, as on the printed page.
        let before = place(deletion.at).and_then(|at| at.checked_sub(1)).and_then(|i| printed.get(i)).or(printed.first());
        let (start, end) = before.copied().unwrap_or((0, 0));
        out.push(ShownRevision { start, end, set: deletion.set, deletion: true });
    }
    out
}

fn preview(text: &str) -> String {
    let mut preview: String = text.chars().take(40).collect();
    if text.chars().count() > 40 {
        preview.push('…');
    }
    preview
}

/// Draw the script and collect the edits to apply.
pub fn show(ui: &mut Ui, session: &mut Session, editor: &mut EditorState, automation: &mut Automation, zoom: f32, pages: bool) -> Vec<EditAction> {
    let mut actions = Vec::new();
    let now = ui.input(|i| i.time);
    // While the writer types in a paragraph, the page breaks are the ones from before the typing
    // began; they catch up after a pause (see `Session::hold_pages`).
    let element_pages = session.shown_pages().element_pages.clone();
    let show = session.show_revisions;
    // Each page's revision label and paper (§3.5): the set it is issued in, and that set's page
    // colour when View > Revised Page Colours is on.
    let page_revisions: Vec<(Option<String>, Color32)> = {
        let revisions = session.script.revisions.clone();
        let colours = session.page_colours;
        let pages = session.shown_pages().pages.iter().map(|page| page.revision(&revisions, show));
        pages
            .map(|set| {
                let paper = set.filter(|_| colours).and_then(|s| s.page_color.as_deref()).and_then(crate::theme::hex);
                (set.map(|s| s.name.clone()), paper.unwrap_or(crate::theme::PAGE))
            })
            .collect()
    };
    let session: &Session = session;
    // An edit, or a jump somewhere else (the navigator, Go To, Undo), ends a selection.
    if editor.span.is_some_and(|span| span.edits != session.edits() || editor.pending_focus.is_some()) {
        editor.span = None;
    }
    // Go To, Find or a bookmark can land in an outline paragraph that is hidden.
    if let Some((target, _)) = editor.pending_focus.filter(|(target, _)| !session.is_shown(*target)) {
        editor.pending_focus = Some((session.nearest_shown(target), 0));
    }
    // The caret and selection before this frame's clicks move them.
    let before_click = (session.caret, editor.cursor);
    span_keys(ui, session, editor, &mut actions);
    if editor.span.is_some() {
        editor.completion.dismissed = true;
    }
    let revisions = &session.script.revisions;
    let paper_of = |page: u32| {
        let index = usize::try_from(page).ok().and_then(|p| p.checked_sub(1));
        index.and_then(|i| page_revisions.get(i)).filter(|_| pages).map_or(crate::theme::PAGE, |r| r.1)
    };
    let layout = &session.layout;
    let col = 0.1 * PX_PER_INCH * zoom;
    let row = PX_PER_INCH / 6.0 * zoom;
    // Text advances as Final Draft's does (see `WRAP_ADVANCE_POINTS`), so lines wrap where they
    // print: Courier Prime a touch smaller, on the same 12-point line.
    let squeeze = WRAP_ADVANCE_POINTS / POINTS_PER_COLUMN;
    let advance = col * squeeze;
    let font = FontId::new(row * squeeze, FontFamily::Name(COURIER.into()));
    let page_width = f32::from(layout.page_columns) * col;
    let page_height = f32::from(layout.page_rows) * row;
    let top_margin = f32::from(layout.body_top) * row;
    let ink = crate::theme::PAGE_INK;
    let faint = crate::theme::PAGE_MUTED;
    let count = session.script.elements.len();
    // A script with nothing in it yet says how to start, on its one empty line.
    let empty_script = matches!(session.script.elements.as_slice(), [only] if only.text.is_empty());
    let automatic = if layout.auto_contd { paginate::auto_contd(&session.script) } else { Vec::new() };
    // egui drops focus on Esc before any widget sees it; Esc closes popups here, so give the
    // caret back to the element it was in.
    let nothing_focused = ui.memory(|m| m.focused().is_none());
    if ui.input(|i| i.key_pressed(Key::Escape))
        && nothing_focused
        && editor.pending_focus.is_none()
        && let Some(last) = editor.last_focused
    {
        let offset = if session.caret.element == last { session.caret.offset } else { usize::MAX };
        editor.pending_focus = Some((last, offset));
        if editor.picker.as_ref().is_some_and(|p| p.index == last) {
            editor.picker = None;
        }
        editor.completion.dismissed = true;
    }
    editor.focused = None;

    // No fade at the scroll edges: on the desk it reads as a smudge across the white page.
    ui.spacing_mut().scroll.fade.strength = 0.0;
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        let side = ((ui.available_width() - page_width) / 2.0).max(8.0);
        ui.add_space(row * 1.5);
        ui.horizontal_top(|ui| {
            ui.add_space(side);
            {
                ui.vertical(|ui| {
                    ui.set_width(page_width);
                    // The paper is painted behind the text once its extent is known.
                    let paper = ui.painter().add(egui::Shape::Noop);
                    let column_left = ui.cursor().left();
                    let mut sheet_top = ui.cursor().top();
                    let mut sheets: Vec<(f32, f32, u32)> = Vec::new();
                    let mut sheet_page = 1u32;
                    ui.add_space(top_margin);
                    *ui.visuals_mut() = crate::theme::page_visuals();
                    let wash = ui.visuals().selection.bg_fill;
                    if editor.span.is_some() {
                        // The wash below is the selection; the field's own caret and selection stay out of it.
                        ui.visuals_mut().selection.bg_fill = Color32::TRANSPARENT;
                        ui.visuals_mut().text_cursor.stroke = Stroke::NONE;
                    }
                    let mut fields: Vec<Field> = Vec::with_capacity(count);
                    editor.heights.resize(count, None);
                    let clip = ui.clip_rect().expand2(vec2(0.0, OVERSCAN_ROWS * row));
                    // A selection across paragraphs finds its places in every paragraph.
                    let lay_out_all = editor.span.is_some() || editor.drag_from.is_some();
                    ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
                    // A text box is exactly one script line tall, at any zoom.
                    ui.spacing_mut().interact_size.y = row;
                    let mut in_speech = false;
                    let mut last_page = 0u32;
                    for (index, element) in session.script.elements.iter().enumerate() {
                        if !session.is_shown(index) {
                            continue;
                        }
                        let kind = element.kind;
                        let style = layout.style(kind);
                        let continues_speech = in_speech && (kind.is_dialogue_part() || kind == ElementKind::Lyrics);
                        in_speech = continues_speech || kind == ElementKind::Character;
                        let page = element_pages.get(index).copied().unwrap_or(0);
                        // Elements that are not printed (notes, sections) report page 0.
                        let new_page = last_page > 0 && page > last_page;
                        if page > 0 {
                            last_page = page;
                        }
                        if index > 0 && new_page && pages {
                            // Finish this sheet and start the next one at its top margin.
                            let y = ui.cursor().top();
                            let bottom = (sheet_top + page_height).max(y + row * 2.0);
                            sheets.push((sheet_top, bottom, sheet_page));
                            sheet_top = bottom + SHEET_GAP_ROWS * row;
                            sheet_page = page;
                            ui.add_space(sheet_top + top_margin - y);
                        } else if index > 0 && new_page {
                            page_break_marker(ui, page, row, &font, faint);
                        } else if index > 0 && !continues_speech {
                            ui.add_space(f32::from(style.space_before) * row);
                        }
                        if kind == ElementKind::PageBreak {
                            let response = ui.add(
                                egui::Label::new(egui::RichText::new("- - - - - - - - page break - - - - - - - -").font(font.clone()).color(faint))
                                    .sense(egui::Sense::click()),
                            );
                            automation.add(format!("editor.element.{index}"), "separator", "Page Break", &response);
                            if response.clicked() {
                                actions.push(EditAction::Focus { index, offset: 0 });
                            }
                            continue;
                        }
                        let id = Id::unique(("storyteller.element", index));
                        let focused = ui.memory(|m| m.has_focus(id));
                        let (box_left, box_width) = match kind {
                            ElementKind::Section | ElementKind::Synopsis => (layout.action.left, layout.action.width),
                            _ => (style.left, style.width),
                        };
                        // Capitals for the kinds that always take them, and for a type the script's
                        // settings print in capitals; the stored text keeps the writer's casing.
                        let show_as = ShowAs { hide_markup: !editor.show_markup, caps: kind.is_caps() || style.caps };
                        let key = height_key(element, zoom, box_width, show_as, style.spacing);
                        let wanted = focused
                            || lay_out_all
                            || editor.pending_focus.is_some_and(|(target, _)| target == index)
                            || editor.scroll_to == Some(index)
                            || editor.highlight.is_some_and(|h| h.0 == index);
                        if !wanted
                            && let Some(Some((known, height))) = editor.heights.get(index).copied()
                            && known == key
                        {
                            let top = ui.cursor().top();
                            if top > clip.bottom() || top + height < clip.top() {
                                // Off screen, and its height is known: only its space.
                                let left = ui.cursor().left() + f32::from(box_left) * col;
                                let rect = Rect::from_min_size(pos2(left, top), vec2(f32::from(box_width) * advance + 1.0, height));
                                automation.add_undrawn(
                                    format!("editor.element.{index}"),
                                    "textbox",
                                    format!("{}: {}", kind.label(), preview(&element.text)),
                                    rect,
                                );
                                ui.add_space(height);
                                continue;
                            }
                        }
                        let box_top = ui.cursor().top();
                        let mut text = element.text.clone();
                        let single_line = kind.is_single_line();
                        let picking = editor.picker.as_ref().is_some_and(|p| p.index == index);
                        let mut edited = false;
                        if focused && picking {
                            picker_keys(ui, index, editor, &mut actions);
                        } else if focused {
                            let shown = Shown::new(&text, show_as, None);
                            handle_keys(ui, id, index, count, &text, &shown, single_line, show_as, session, editor, &mut actions);
                            edited = field_keys(ui, index, &mut text, show_as, editor, &mut actions);
                        }
                        if let Some((target, offset)) = editor.pending_focus
                            && target == index
                        {
                            let mut offset = offset.min(text.len());
                            while !text.is_char_boundary(offset) {
                                offset -= 1;
                            }
                            editor.cursor = Some(Cursor::at(index, offset));
                            editor.composing = None;
                            editor.column = None;
                            editor.last_interaction = now;
                            ui.memory_mut(|m| m.request_focus(id));
                            editor.pending_focus = None;
                            editor.scroll_to_focus = true;
                        }
                        let paper = paper_of(sheet_page);
                        let faint = crate::theme::muted_on(paper);
                        let color = if kind.is_printed() { ink } else { faint };
                        let shown_revisions = shown_revisions(element, revisions, show);
                        let revised: Vec<(usize, usize, Color32)> = shown_revisions
                            .iter()
                            .filter(|r| !r.deletion)
                            .map(|r| (r.start, r.end, crate::theme::revised_ink(revisions.set(r.set).and_then(|s| s.color.as_deref()), paper)))
                            .collect();
                        let marked = editor.highlight.filter(|h| h.0 == index).map(|h| (h.1, h.2));
                        let spanned = editor.span.and_then(|span| span.bytes_in(index, &element.text));
                        let misspelled = editor.misspelled.get(&index).cloned().unwrap_or_default();
                        let cursor = editor.cursor.filter(|c| c.index == index);
                        let composing_text = editor.composing.as_ref().filter(|(i, _)| *i == index).map(|(_, t)| t.clone());
                        let composing = composing_text.as_deref().zip(cursor).map(|(t, c)| (c.head, t));
                        let shown = Shown::new(&text, show_as, composing);
                        let geometry = Geometry {
                            origin: pos2(column_left + f32::from(box_left) * col, box_top),
                            columns: box_width,
                            width: f32::from(box_width) * advance + 1.0,
                            align: style.align,
                            row,
                            step: row * f32::from(style.spacing.max(1)),
                            advance,
                            gaps: Vec::new(),
                        };
                        let format =
                            |_: usize, c: &ShownChar| char_format(c, (&marked, &spanned, wash), &misspelled, &revised, &font, (color, faint));
                        let laid = Arc::new(Laid::new(ui, shown, &geometry, format));
                        ui.advance_cursor_after_rect(laid.rect);
                        let response = ui.interact(laid.rect, id, Sense::click_and_drag()).on_hover_cursor(egui::CursorIcon::Text);
                        if !picking && pointer_keys(ui, &response, index, &laid, editor, now) {
                            response.request_focus();
                        }
                        // The edit's own id holds focus.
                        let has_focus = ui.memory(|m| m.has_focus(id));
                        let cursor = editor.cursor.filter(|c| c.index == index);
                        let head = cursor.map(|c| laid.shown.place_of(c.head) + composing.map_or(0, |(_, t)| t.chars().count()));
                        if has_focus {
                            ui.memory_mut(|m| {
                                m.set_focus_lock_filter(
                                    id,
                                    egui::EventFilter { tab: true, horizontal_arrows: true, vertical_arrows: true, escape: false },
                                );
                            });
                            if let Some(cursor) = cursor
                                && editor.span.is_none()
                            {
                                let (from, to) = cursor.ordered();
                                let (from, to) = (laid.shown.place_of(from), laid.shown.place_of(to));
                                paint_selection(ui, &laid, from, to, ui.visuals().selection.bg_fill, advance);
                            }
                        }
                        for line in &laid.lines {
                            ui.painter().galley(line.pos, line.galley.clone(), color);
                        }
                        if text.is_empty() && composing.is_none() {
                            let hint = if empty_script { crate::messages::EMPTY_SCRIPT_HINT } else { kind.label() };
                            let (anchor, x) = match style.align {
                                PageAlign::Left => (egui::Align2::LEFT_TOP, laid.rect.left()),
                                PageAlign::Right => (egui::Align2::RIGHT_TOP, laid.rect.right()),
                                PageAlign::Center => (egui::Align2::CENTER_TOP, laid.rect.center().x),
                            };
                            ui.painter().text(pos2(x, laid.rect.top()), anchor, hint, font.clone(), crate::theme::PAGE_MUTED);
                        }
                        if let Some(head) = head
                            && has_focus
                            && editor.span.is_none()
                        {
                            let caret = laid.caret_rect(head);
                            if ui.input(|i| i.focused) {
                                egui::text_selection::visuals::paint_text_cursor(ui, ui.painter(), caret, now - editor.last_interaction);
                            }
                            if edited || editor.last_interaction == now {
                                ui.scroll_to_rect(caret.expand(row), None);
                            }
                            let to_global = ui.ctx().layer_transform_to_global(ui.layer_id()).unwrap_or_default();
                            ui.output_mut(|o| {
                                o.ime = Some(egui::output::IMEOutput {
                                    purpose: egui::IMEPurpose::Normal,
                                    rect: to_global * laid.rect,
                                    cursor_rect: to_global * caret,
                                    should_interrupt_composition: false,
                                });
                            });
                            editor.first_row = laid.on_first_line(head);
                            editor.last_row = laid.on_last_line(head);
                        }
                        response.widget_info(|| egui::WidgetInfo::text_edit(true, element.text.as_str(), text.as_str(), kind.label()));
                        if let Some(slot) = editor.heights.get_mut(index) {
                            *slot = Some((key, laid.rect.height()));
                        }
                        let rect = laid.rect;
                        fields.push(Field { index, laid: laid.clone() });
                        automation.add(
                            format!("editor.element.{index}"),
                            "textbox",
                            format!("{}: {}", kind.label(), preview(&element.text)),
                            &response,
                        );
                        if element.bookmark.is_some() {
                            // A small ribbon just left of the text, on screen only.
                            let at = rect.left_top() + vec2(-row * 0.75, row * 0.15);
                            let (w, h) = (row * 0.4, row * 0.7);
                            let points = vec![at, at + vec2(w, 0.0), at + vec2(w, h), at + vec2(w * 0.5, h * 0.72), at + vec2(0.0, h)];
                            ui.painter().add(egui::Shape::convex_polygon(points, faint, Stroke::NONE));
                        }
                        if kind == ElementKind::SceneHeading
                            && layout.show_scene_numbers
                            && let Some(number) = &element.scene_number
                        {
                            let sides = session.script.scene_number_margins();
                            scene_numbers(ui, rect, number, sides, (layout.scene_number_left_end, layout.scene_number_right), col, &font, ink);
                        }
                        if !shown_revisions.is_empty() {
                            let mark_x = column_left + REVISION_MARK_INCHES * PX_PER_INCH * zoom;
                            revision_marks(ui, &laid, &shown_revisions, (revisions, show), mark_x, (&font, paper));
                        }
                        if kind == ElementKind::Character
                            && automatic.get(index).copied().unwrap_or(false)
                            && let Some(first) = laid.lines.first()
                        {
                            // The automatic (CONT'D), drawn faint: it is added at layout, not typed.
                            let at = first.pos + vec2(first.galley.size().x + advance, 0.0);
                            ui.painter().text(at, egui::Align2::LEFT_TOP, &layout.contd, font.clone(), faint);
                        }
                        if editor.scroll_to == Some(index) {
                            response.scroll_to_me(Some(Align::Center));
                            editor.scroll_to = None;
                        }
                        if editor.scroll_to_focus && has_focus {
                            response.scroll_to_me(Some(Align::Center));
                            editor.scroll_to_focus = false;
                        }
                        if has_focus {
                            editor.focused = Some(index);
                            if let Some(cursor) = cursor
                                && !edited
                            {
                                actions.push(EditAction::Caret { index, offset: cursor.head });
                            }
                            if picking {
                                picker_popup(ui, editor, automation, index, rect, &mut actions);
                            } else {
                                completion_popup(ui, session, editor, automation, index, kind, &text, rect, &font, &mut actions);
                            }
                        }
                    }
                    pointer_span(ui, session, editor, &fields, before_click);
                    if editor.focused.is_none() {
                        editor.composing = None;
                    }
                    editor.fields = fields;
                    let y = ui.cursor().top() + row * 6.0;
                    let bottom = if pages { (sheet_top + page_height).max(y) } else { y };
                    sheets.push((sheet_top, bottom, sheet_page));
                    ui.add_space(bottom - ui.cursor().top());
                    let papers: Vec<Color32> = sheets.iter().map(|s| paper_of(s.2)).collect();
                    ui.painter().set(paper, egui::Shape::Vec(paper_shapes(&sheets, &papers, column_left, page_width, pages)));
                    if pages {
                        // Page numbers sit in the top margin, clear of the text; a revised page's
                        // set and date go under its number.
                        let right = column_left + f32::from(layout.page_number_right) * col;
                        for (top, _, number) in &sheets {
                            let at = pos2(right, top + f32::from(layout.page_number_row) * row);
                            if *number > 1 {
                                ui.painter().text(at, egui::Align2::RIGHT_TOP, format!("{number}."), font.clone(), ink);
                            }
                            let label = usize::try_from(*number)
                                .ok()
                                .and_then(|n| n.checked_sub(1))
                                .and_then(|i| page_revisions.get(i))
                                .and_then(|r| r.0.as_ref());
                            if let Some(label) = label {
                                ui.painter().text(at + vec2(0.0, row), egui::Align2::RIGHT_TOP, label, font.clone(), ink);
                            }
                        }
                    }
                });
            }
        });
        ui.add_space(row * 3.0);
    });
    if editor.focused.is_some() {
        editor.last_focused = editor.focused;
    }
    // Clicking elsewhere closes the picker.
    if editor.picker.as_ref().is_some_and(|p| editor.focused != Some(p.index)) && editor.pending_focus.is_none() {
        editor.picker = None;
    }
    actions
}

/// White sheets (or one continuous column) with a soft shadow.
fn paper_shapes(sheets: &[(f32, f32, u32)], papers: &[Color32], left: f32, width: f32, pages: bool) -> Vec<egui::Shape> {
    let mut shapes = Vec::new();
    let spans: Vec<(f32, f32, Color32)> = if pages {
        sheets.iter().zip(papers.iter().chain(std::iter::repeat(&crate::theme::PAGE))).map(|(s, &paper)| (s.0, s.1, paper)).collect()
    } else {
        let top = sheets.first().map_or(0.0, |s| s.0);
        let bottom = sheets.last().map_or(top, |s| s.1);
        vec![(top, bottom, crate::theme::PAGE)]
    };
    for (top, bottom, paper) in spans {
        let rect = Rect::from_min_max(pos2(left, top), pos2(left + width, bottom));
        for (spread, alpha) in [(6.0, 10u8), (3.0, 18), (1.0, 34)] {
            shapes.push(egui::Shape::rect_filled(rect.expand(spread).translate(vec2(0.0, spread * 0.5)), 3.0, Color32::from_black_alpha(alpha)));
        }
        shapes.push(egui::Shape::rect_filled(rect, 1.0, paper));
    }
    shapes
}

/// The pointer's part in a selection across paragraphs: Shift and a click in another paragraph,
/// or a drag from one into another. Inside one paragraph the text field does it all.
fn pointer_span(ui: &Ui, session: &Session, editor: &mut EditorState, fields: &[Field], (caret, selection): (Caret, Option<Cursor>)) {
    let (pressed, down, shift, pos) =
        ui.input(|i| (i.pointer.primary_pressed(), i.pointer.primary_down(), i.modifiers.shift, i.pointer.interact_pos()));
    if !down && !pressed {
        editor.drag_from = None;
        return;
    }
    let Some(pos) = pos else { return };
    if pressed {
        let layer = ui.layer_id();
        let hit = fields.iter().any(|f| ui.ctx().rect_contains_pointer(layer, f.rect()));
        if !hit {
            return;
        }
        let Some(place) = place_at(fields, pos, false) else { return };
        let anchor = if !shift {
            place
        } else if let Some(span) = editor.span {
            span.anchor
        } else {
            // The far end of the selection there was, or the caret.
            let far = selection.filter(|c| c.index == caret.element).map(|c| Caret { element: caret.element, offset: c.anchor });
            far.unwrap_or(caret)
        };
        editor.drag_from = Some(anchor);
        editor.span = (anchor.element != place.element).then_some(Span { anchor, head: place, edits: session.edits(), column: None });
        return;
    }
    let Some(anchor) = editor.drag_from else { return };
    let Some(place) = place_at(fields, pos, false) else { return };
    editor.span = (anchor.element != place.element).then_some(Span { anchor, head: place, edits: session.edits(), column: None });
    // Near the top or bottom of the view, the script moves under the pointer.
    let clip = ui.clip_rect();
    let reach = 24.0;
    let delta = if pos.y < clip.top() + reach {
        8.0
    } else if pos.y > clip.bottom() - reach {
        -8.0
    } else {
        0.0
    };
    if delta != 0.0 {
        ui.scroll_with_delta(vec2(0.0, delta));
    }
}

#[allow(clippy::too_many_arguments)]
fn handle_keys(
    ui: &mut Ui,
    id: Id,
    index: usize,
    count: usize,
    text: &str,
    shown: &Shown,
    single_line: bool,
    show_as: ShowAs,
    session: &Session,
    editor: &mut EditorState,
    actions: &mut Vec<EditAction>,
) {
    let _ = id;
    let cursor = editor.cursor.filter(|c| c.index == index).unwrap_or(Cursor::at(index, text.len()));
    let (from, to) = cursor.ordered();
    let has_selection = from != to;
    let caret = cursor.head;
    let at = shown.place_of(caret);
    let popup = editor.completion.visible_for(index);
    // The paragraphs the arrows move to: the nearest drawn ones.
    let above = (0..index).rfind(|&i| session.is_shown(i));
    let below = (index + 1..count).find(|&i| session.is_shown(i));
    if editor.span.is_none() {
        let anchor = Caret { element: index, offset: cursor.anchor };
        let head = Caret { element: index, offset: caret };
        let edits = session.edits();
        let start = |column| Span { anchor, head, edits, column };
        let mut span = None;
        if count > 1 && ui.input_mut(|i| i.consume_key(Modifiers::COMMAND, Key::A)) {
            editor.select_whole_script(session);
            return;
        }
        if !popup {
            let up = editor.first_row && above.is_some() && ui.input_mut(|i| i.consume_key(Modifiers::SHIFT, Key::ArrowUp));
            let down = !up && editor.last_row && below.is_some() && ui.input_mut(|i| i.consume_key(Modifiers::SHIFT, Key::ArrowDown));
            if up || down {
                span = row_from(&editor.fields, head, None, up).map(|(head, x)| Span { head, column: Some(x), ..start(None) });
            } else if at == 0 && above.is_some() && ui.input_mut(|i| i.consume_key(Modifiers::SHIFT, Key::ArrowLeft)) {
                span = Some(Span { head: step_from(session, head, true), ..start(None) });
            } else if at == shown.len() && below.is_some() && ui.input_mut(|i| i.consume_key(Modifiers::SHIFT, Key::ArrowRight)) {
                span = Some(Span { head: step_from(session, head, false), ..start(None) });
            }
        }
        if span.is_some() {
            editor.span = span;
            return;
        }
    }
    ui.input_mut(|input| {
        // Screenplay text in several paragraphs (from Copy Scene, a Fountain file, another app)
        // becomes elements; a paste within one paragraph stays the paragraph's own.
        let screenplay = input.events.iter().position(|e| matches!(e, egui::Event::Paste(text) if session.pastes_as_elements(text)));
        if let Some(egui::Event::Paste(pasted)) = screenplay.map(|at| input.events.remove(at)) {
            actions.push(EditAction::PasteScreenplay { index, start: from, end: to, text: pasted });
            return;
        }
        if popup {
            let items = editor.completion.items.len();
            if input.consume_key(Modifiers::NONE, Key::ArrowDown) {
                editor.completion.selected = (editor.completion.selected + 1) % items.max(1);
            }
            if input.consume_key(Modifiers::NONE, Key::ArrowUp) {
                editor.completion.selected = editor.completion.selected.checked_sub(1).unwrap_or(items.saturating_sub(1));
            }
            if input.consume_key(Modifiers::NONE, Key::Escape) {
                editor.completion.dismissed = true;
            }
            let chosen = editor.completion.items.get(editor.completion.selected).cloned();
            if let Some(choice) = chosen {
                if input.consume_key(Modifiers::NONE, Key::Enter) {
                    actions.push(EditAction::Accept { index, text: choice, then_enter: true });
                    return;
                }
                if input.consume_key(Modifiers::NONE, Key::Tab) {
                    actions.push(EditAction::Accept { index, text: choice, then_enter: false });
                    return;
                }
            }
        }
        if input.consume_key(Modifiers::SHIFT, Key::Enter) {
            if !single_line {
                let (a, b) = (shown.place_of(cursor.anchor), at);
                let edit = replace(text, show_as, a, b, "\n");
                actions.push(EditAction::SetText { index, text: edit.text, caret: edit.caret });
                actions.push(EditAction::Focus { index, offset: edit.caret });
            }
        } else if input.consume_key(Modifiers::NONE, Key::Enter) {
            actions.push(EditAction::Enter { index, offset: caret });
        } else if input.consume_key(Modifiers::SHIFT, Key::Tab) {
            actions.push(EditAction::Tab { index, back: true });
        } else if input.consume_key(Modifiers::NONE, Key::Tab) {
            actions.push(EditAction::Tab { index, back: false });
        } else if index > 0 && !has_selection && at == 0 && input.consume_key(Modifiers::NONE, Key::Backspace) {
            actions.push(EditAction::MergePrevious { index });
        } else if !popup && above.is_some() && editor.first_row && input.consume_key(Modifiers::NONE, Key::ArrowUp) {
            actions.extend(above.map(|index| EditAction::Focus { index, offset: usize::MAX }));
        } else if !popup && below.is_some() && editor.last_row && input.consume_key(Modifiers::NONE, Key::ArrowDown) {
            actions.extend(below.map(|index| EditAction::Focus { index, offset: 0 }));
        }
    });
}

/// Typing, deleting, moving and selecting inside the paragraph with the keyboard, on `text`
/// as it stands this frame. Returns whether the text changed.
fn field_keys(ui: &mut Ui, index: usize, text: &mut String, show: ShowAs, editor: &mut EditorState, actions: &mut Vec<EditAction>) -> bool {
    let mac = cfg!(target_os = "macos");
    let laid = editor.fields.iter().find(|f| f.index == index).map(|f| f.laid.clone());
    let mut cursor = editor.cursor.filter(|c| c.index == index).unwrap_or(Cursor::at(index, text.len()));
    let mut changed = false;
    let mut moved = false;
    let mut column = editor.column;
    let events = ui.input_mut(|i| std::mem::take(&mut i.events));
    let mut kept = Vec::with_capacity(events.len());
    for event in events {
        let shown = Shown::new(text, show, None);
        let (anchor, head) = (shown.place_of(cursor.anchor), shown.place_of(cursor.head));
        let (lo, hi) = (anchor.min(head), anchor.max(head));
        // The same text as last frame: its lines say where Up, Down, Home and End go.
        let lines = laid.as_ref().filter(|l| !changed && l.shown.chars == shown.chars);
        let mut edit = |from: usize, to: usize, typed: &str, text: &mut String, cursor: &mut Cursor| {
            let edit = replace(text, show, from, to, typed);
            *text = edit.text;
            *cursor = Cursor::at(index, edit.caret);
            changed = true;
        };
        match event {
            egui::Event::Text(typed) | egui::Event::Paste(typed) if !typed.is_empty() => edit(lo, hi, &typed, text, &mut cursor),
            egui::Event::Ime(egui::ImeEvent::Commit(typed)) => {
                editor.composing = None;
                if !typed.is_empty() && typed != "\n" && typed != "\r" {
                    edit(lo, hi, &typed, text, &mut cursor);
                }
            }
            egui::Event::Ime(egui::ImeEvent::Preedit { text: composing, .. }) => {
                if lo < hi && !composing.is_empty() {
                    edit(lo, hi, "", text, &mut cursor);
                }
                editor.composing = (!composing.is_empty() && composing != "\n").then_some((index, composing));
            }
            egui::Event::Copy if lo < hi => {
                let (from, to) = shown.stored_range(lo, hi);
                actions.push(EditAction::CopyRange { from: Caret { element: index, offset: from }, to: Caret { element: index, offset: to } });
            }
            egui::Event::Cut if lo < hi => {
                let (from, to) = shown.stored_range(lo, hi);
                actions.push(EditAction::CopyRange { from: Caret { element: index, offset: from }, to: Caret { element: index, offset: to } });
                edit(lo, hi, "", text, &mut cursor);
            }
            egui::Event::Key { key, pressed: true, modifiers, .. } if editor.composing.is_none() => {
                let word = if mac { modifiers.alt } else { modifiers.ctrl };
                let whole = mac && modifiers.mac_cmd;
                let line_ends = |at: usize| lines.map_or((0, shown.len()), |l| l.line_ends(at));
                let target = match key {
                    Key::Backspace => {
                        if lo < hi {
                            edit(lo, hi, "", text, &mut cursor);
                        } else if head > 0 {
                            let from = if whole {
                                line_ends(head).0.min(head - 1)
                            } else if word {
                                shown.word_left(head)
                            } else {
                                shown.prev(head)
                            };
                            edit(from, head, "", text, &mut cursor);
                        }
                        None
                    }
                    Key::Delete => {
                        if lo < hi {
                            edit(lo, hi, "", text, &mut cursor);
                        } else if head < shown.len() {
                            let to = if word { shown.word_right(head) } else { shown.next(head) };
                            edit(head, to, "", text, &mut cursor);
                        }
                        None
                    }
                    Key::ArrowLeft if whole => Some(line_ends(head).0),
                    Key::ArrowRight if whole => Some(line_ends(head).1),
                    Key::ArrowLeft if word => Some(shown.word_left(head)),
                    Key::ArrowRight if word => Some(shown.word_right(head)),
                    Key::ArrowLeft if lo < hi && !modifiers.shift => Some(lo),
                    Key::ArrowRight if lo < hi && !modifiers.shift => Some(hi),
                    Key::ArrowLeft => Some(shown.prev(head)),
                    Key::ArrowRight => Some(shown.next(head)),
                    Key::Home => Some(if modifiers.command { 0 } else { line_ends(head).0 }),
                    Key::End => Some(if modifiers.command { shown.len() } else { line_ends(head).1 }),
                    Key::ArrowUp | Key::ArrowDown => {
                        let up = key == Key::ArrowUp;
                        if modifiers.command {
                            Some(if up { 0 } else { shown.len() })
                        } else {
                            let target = lines.and_then(|l| {
                                let x = column.unwrap_or_else(|| l.caret_rect(head).center().x);
                                column = Some(x);
                                l.line_step(head, x, up)
                            });
                            let target = target.unwrap_or(if up { 0 } else { shown.len() });
                            moved = true;
                            if !modifiers.shift {
                                cursor = Cursor::at(index, shown.to_stored(target));
                            } else {
                                cursor.head = shown.to_stored(target);
                            }
                            continue;
                        }
                    }
                    Key::A if modifiers.command => {
                        cursor = Cursor { index, anchor: shown.to_stored(0), head: text.len() };
                        moved = true;
                        None
                    }
                    _ => {
                        kept.push(egui::Event::Key { key, pressed: true, modifiers, physical_key: None, repeat: false });
                        continue;
                    }
                };
                if let Some(target) = target {
                    let byte = shown.to_stored(target);
                    if modifiers.shift {
                        cursor.head = byte;
                    } else {
                        cursor = Cursor::at(index, byte);
                    }
                    moved = true;
                }
                column = None;
            }
            other => kept.push(other),
        }
    }
    ui.input_mut(|i| i.events = kept);
    if changed {
        editor.composing = None;
        column = None;
        actions.push(EditAction::SetText { index, text: text.clone(), caret: cursor.head });
    }
    if changed || moved {
        editor.cursor = Some(cursor);
        editor.last_interaction = ui.input(|i| i.time);
    }
    editor.column = column;
    changed
}

/// The pointer inside one paragraph: a press puts the caret there (Shift extends the
/// selection), a drag selects, a double click selects a word and a triple click the paragraph.
/// Returns whether the paragraph should take the keyboard.
fn pointer_keys(ui: &Ui, response: &egui::Response, index: usize, laid: &Laid, editor: &mut EditorState, now: f64) -> bool {
    let (pressed, shift, pos) = ui.input(|i| (i.pointer.primary_pressed(), i.modifiers.shift, i.pointer.interact_pos()));
    let Some(pos) = pos else { return false };
    let at = laid.place_at(pos);
    let byte = laid.shown.to_stored(at);
    let mine = editor.cursor.filter(|c| c.index == index);
    let mut focus = false;
    if response.triple_clicked() {
        editor.cursor = Some(Cursor { index, anchor: laid.shown.to_stored(0), head: laid.shown.to_stored(laid.shown.len()) });
    } else if response.double_clicked() {
        let (from, to) = laid.shown.word_at(at);
        editor.cursor = Some(Cursor { index, anchor: laid.shown.to_stored(from), head: laid.shown.to_stored(to) });
    } else if pressed && response.contains_pointer() {
        let anchor = if shift { mine.map_or(byte, |c| c.anchor) } else { byte };
        editor.cursor = Some(Cursor { index, anchor, head: byte });
        editor.composing = None;
        focus = true;
    } else if response.dragged_by(egui::PointerButton::Primary) && editor.span.is_none() && mine.is_some() {
        editor.cursor = mine.map(|c| Cursor { head: byte, ..c });
    } else {
        return false;
    }
    editor.column = None;
    editor.last_interaction = now;
    focus
}

/// The selection inside one paragraph, `from..to` on screen, as a wash behind each line.
fn paint_selection(ui: &Ui, laid: &Laid, from: usize, to: usize, wash: Color32, advance: f32) {
    if from >= to {
        return;
    }
    let last = laid.lines.len().saturating_sub(1);
    for (n, line) in laid.lines.iter().enumerate() {
        let start = from.max(line.start);
        let end = to.min(line.end);
        // A selection that runs on past a line's end takes in the space or break there.
        let runs_on = to > line.end && n < last;
        if start > end || (start == end && !runs_on) {
            continue;
        }
        let left = laid.caret_rect(start).left();
        let right = if runs_on { line.pos.x + line.galley.size().x + advance } else { laid.caret_rect(end).left() };
        let top = line.pos.y;
        let rect = Rect::from_min_max(pos2(left, top), pos2(right.max(left), top + laid.caret_rect(start).height()));
        ui.painter().rect_filled(rect, 0.0, wash);
    }
}

fn page_break_marker(ui: &mut Ui, page: u32, row: f32, font: &FontId, color: Color32) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), row * 2.0), egui::Sense::hover());
    let y = rect.center().y;
    let painter = ui.painter();
    let mut x = rect.left();
    while x < rect.right() {
        painter.line_segment([pos2(x, y), pos2((x + 6.0).min(rect.right()), y)], Stroke::new(1.0, crate::theme::PAGE_RULE));
        x += 12.0;
    }
    painter.text(
        pos2(rect.right() - 8.0, y - 2.0),
        egui::Align2::RIGHT_BOTTOM,
        format!("{page}."),
        FontId::new(font.size * 0.8, font.family.clone()),
        color,
    );
}

/// A scene's number in the margins the script prints it in.
#[allow(clippy::too_many_arguments)]
fn scene_numbers(
    ui: &Ui,
    rect: Rect,
    number: &str,
    (on_left, on_right): (bool, bool),
    (left_end, right): (u16, u16),
    col: f32,
    font: &FontId,
    color: Color32,
) {
    let page_left = ui.min_rect().left();
    let painter = ui.painter();
    let top = rect.top();
    if on_left {
        painter.text(pos2(page_left + f32::from(left_end) * col, top), egui::Align2::RIGHT_TOP, number, font.clone(), color);
    }
    if on_right {
        painter.text(pos2(page_left + f32::from(right) * col, top), egui::Align2::LEFT_TOP, number, font.clone(), color);
    }
}

#[allow(clippy::too_many_arguments)]
fn completion_popup(
    ui: &Ui,
    session: &Session,
    editor: &mut EditorState,
    automation: &mut Automation,
    index: usize,
    kind: ElementKind,
    text: &str,
    rect: Rect,
    font: &FontId,
    actions: &mut Vec<EditAction>,
) {
    if !matches!(kind, ElementKind::Character | ElementKind::SceneHeading | ElementKind::Transition) {
        editor.completion.items.clear();
        return;
    }
    let completion = &mut editor.completion;
    if completion.index != index || completion.text != text {
        *completion = Completion {
            index,
            text: text.to_string(),
            items: autocomplete::suggest_at(&session.script, index, kind, text),
            selected: 0,
            dismissed: false,
        };
    }
    if !completion.visible_for(index) {
        return;
    }
    let items = completion.items.clone();
    let selected = completion.selected;
    egui::Area::new(Id::unique("storyteller.autocomplete")).order(Order::Foreground).fixed_pos(rect.left_bottom() + vec2(0.0, 2.0)).show(
        ui.ctx(),
        |ui| {
            Frame::popup(ui.style()).show(ui, |ui| {
                for (n, item) in items.iter().enumerate() {
                    let response = ui.selectable_label(n == selected, egui::RichText::new(item).font(font.clone()));
                    automation.add(format!("autocomplete.item.{n}"), "option", item.clone(), &response);
                    if response.clicked() {
                        actions.push(EditAction::Accept { index, text: item.clone(), then_enter: false });
                    }
                }
            });
        },
    );
}

/// Keys while the element picker is open: a choice's letter, arrows and Enter, or Esc.
fn picker_keys(ui: &mut Ui, index: usize, editor: &mut EditorState, actions: &mut Vec<EditAction>) {
    let Some(picker) = editor.picker.as_mut() else { return };
    let count = flow::PICKER.len();
    let mut chosen = None;
    let mut close = false;
    ui.input_mut(|input| {
        if input.consume_key(Modifiers::NONE, Key::ArrowDown) {
            picker.selected = (picker.selected + 1) % count;
        }
        if input.consume_key(Modifiers::NONE, Key::ArrowUp) {
            picker.selected = picker.selected.checked_sub(1).unwrap_or(count - 1);
        }
        if input.consume_key(Modifiers::NONE, Key::Escape) {
            close = true;
        }
        if input.consume_key(Modifiers::NONE, Key::Enter) {
            chosen = flow::PICKER.get(picker.selected).map(|(_, kind)| *kind);
        }
        for (letter, kind) in flow::PICKER {
            if let Some(key) = Key::from_name(&letter.to_string())
                && (input.consume_key(Modifiers::NONE, key) || input.consume_key(Modifiers::SHIFT, key))
            {
                chosen = Some(kind);
            }
        }
        // The picker has the keyboard: nothing typed reaches the text.
        input.events.retain(|event| !matches!(event, egui::Event::Text(_) | egui::Event::Key { .. }));
    });
    if let Some(kind) = chosen {
        actions.push(EditAction::Pick { index, kind });
        editor.picker = None;
    } else if close {
        editor.picker = None;
    }
}

/// The element picker under an empty paragraph.
fn picker_popup(ui: &Ui, editor: &EditorState, automation: &mut Automation, index: usize, rect: Rect, actions: &mut Vec<EditAction>) {
    let Some(picker) = editor.picker.as_ref() else { return };
    egui::Area::new(Id::unique("storyteller.picker")).order(Order::Foreground).fixed_pos(rect.left_bottom() + vec2(0.0, 2.0)).show(ui.ctx(), |ui| {
        Frame::popup(ui.style()).show(ui, |ui| {
            ui.label(egui::RichText::new("Element").small().weak());
            for (n, (letter, kind)) in flow::PICKER.iter().enumerate() {
                let response = ui.selectable_label(n == picker.selected, format!("{letter}   {}", kind.label()));
                automation.add(format!("picker.{}", kind.id()), "option", kind.label(), &response);
                if response.clicked() {
                    actions.push(EditAction::Pick { index, kind: *kind });
                }
            }
        });
    });
}

/// Each line with a shown revision gets its latest set's mark at 7.75", in that set's ink.
fn revision_marks(
    ui: &Ui,
    laid: &Laid,
    shown: &[ShownRevision],
    (revisions, show): (&Revisions, ShowRevisions),
    mark_x: f32,
    (font, paper): (&FontId, Color32),
) {
    for line in &laid.lines {
        let chars = laid.shown.chars.get(line.start..line.end).unwrap_or(&[]);
        // An empty line, or the space a deletion mark sits beside at the end of one.
        let edge = laid.shown.chars.get(line.end).or(chars.last());
        let sets: Vec<u32> = shown
            .iter()
            .filter(|r| {
                let on = |c: &ShownChar| c.start < c.end && r.start < c.end && c.start < r.end.max(r.start + 1);
                chars.iter().any(on) || (r.start == r.end && edge.is_some_and(on))
            })
            .map(|r| r.set)
            .collect();
        if let Some(set) = revisions.latest(&sets, show) {
            let at = pos2(mark_x, line.pos.y);
            ui.painter().text(at, egui::Align2::LEFT_TOP, &set.mark, font.clone(), crate::theme::revised_ink(set.color.as_deref(), paper));
        }
    }
}

/// A range of bytes in a paragraph's text, if there is one.
type TextRange = Option<(usize, usize)>;

/// How one character on screen is drawn: bold, italic and underlined text in Courier Prime's
/// own faces, notes (and the markup, while it shows) faint; the current find match (or spelling
/// word) on a rose wash, the selection across paragraphs on `wash`; misspelled words with a thin
/// red underline; revised text in its set's ink. Ranges are bytes of the stored text.
fn char_format(
    c: &ShownChar,
    (marked, spanned, wash): (&TextRange, &TextRange, Color32),
    misspelled: &[(usize, usize)],
    revised: &[(usize, usize, Color32)],
    font: &FontId,
    (color, faint): (Color32, Color32),
) -> TextFormat {
    let inside = |&(s, e): &(usize, usize)| c.start < c.end && s < c.end && c.end <= e;
    let mut format = TextFormat { font_id: font.clone(), color, ..Default::default() };
    match c.look {
        Look::Text(style) => {
            // Revised text in its set's ink; the markup around it stays faint.
            if let Some(r) = revised.iter().find(|&&(s, e, _)| inside(&(s, e))) {
                format.color = r.2;
            }
            let face = match (style.bold, style.italic) {
                (true, true) => Some(COURIER_BOLD_ITALIC),
                (true, false) => Some(COURIER_BOLD),
                (false, true) => Some(COURIER_ITALIC),
                (false, false) => None,
            };
            if let Some(face) = face {
                format.font_id = FontId::new(font.size, FontFamily::Name(face.into()));
            }
            if style.underline {
                format.underline = Stroke::new(1.0, format.color);
            }
        }
        Look::Faint => format.color = faint,
        Look::Composing => format.underline = Stroke::new(1.0, color),
    }
    if marked.as_ref().is_some_and(inside) {
        format.background = crate::theme::FIND_MARK;
    }
    if spanned.as_ref().is_some_and(inside) {
        format.background = wash;
    }
    if misspelled.iter().any(inside) {
        format.underline = Stroke::new(1.5, crate::theme::SPELL_MARK);
    }
    format
}
