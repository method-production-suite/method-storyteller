//! What the editor paints besides the text: paper, selections, page-break marks, scene
//! numbers, revision marks, the popups, and each character's format.

use super::*;

/// White sheets (or one continuous column) with a soft shadow.
pub(super) fn paper_shapes(sheets: &[(f32, f32, u32)], papers: &[Color32], left: f32, width: f32, pages: bool) -> Vec<egui::Shape> {
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

/// The selection inside one paragraph, `from..to` on screen, as a wash behind each line.
pub(super) fn paint_selection(ui: &Ui, laid: &Laid, from: usize, to: usize, wash: Color32, advance: f32) {
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

pub(super) fn page_break_marker(ui: &mut Ui, page: u32, row: f32, font: &FontId, color: Color32) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), row * 2.0), egui::Sense::hover());
    paint_page_break(ui.painter(), rect, page, font, color);
}

/// Draft view's mark where a page starts: a dashed rule across `rect`, and the page's number.
pub(super) fn paint_page_break(painter: &egui::Painter, rect: Rect, page: u32, font: &FontId, color: Color32) {
    let y = rect.center().y;
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
pub(super) fn scene_numbers(
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
pub(super) fn completion_popup(
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

/// The element picker under an empty paragraph.
pub(super) fn picker_popup(ui: &Ui, editor: &EditorState, automation: &mut Automation, index: usize, rect: Rect, actions: &mut Vec<EditAction>) {
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
pub(super) fn revision_marks(
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
pub(super) type TextRange = Option<(usize, usize)>;

/// How one character on screen is drawn: bold, italic and underlined text in Courier Prime's
/// own faces, notes (and the markup, while it shows) faint; the current find match (or spelling
/// word) on a rose wash, the selection across paragraphs on `wash`; misspelled words with a thin
/// red underline; revised text in its set's ink. Ranges are bytes of the stored text.
pub(super) fn char_format(
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
