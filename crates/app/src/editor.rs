//! The script view: each element is a frameless text box at its industry margin, on white
//! paper. Page view shows separate sheets with their page numbers; draft view one continuous
//! column with page-break marks. Enter, Tab, Backspace-at-start and arrows at the edges become
//! engine commands; typing is committed with `element.set_text`.

use std::sync::Arc;

use egui::text::{CCursor, CCursorRange, LayoutJob};
use egui::text_edit::TextEditState;
use egui::{
    Align, Color32, FontData, FontDefinitions, FontFamily, FontId, Frame, Id, Key, Margin, Modifiers, Order, Rect, Stroke, TextEdit, Ui, pos2, vec2,
};
use storyteller_core::layout::{Align as PageAlign, POINTS_PER_COLUMN, WRAP_ADVANCE_POINTS};
use storyteller_core::{ElementKind, autocomplete, paginate};
use storyteller_engine::Session;
use storyteller_engine::flow;

use crate::automation::Automation;

/// Font family name for Courier Prime.
pub const COURIER: &str = "courier_prime";
/// Pixels per inch at 100 % zoom.
const PX_PER_INCH: f32 = 96.0;
/// Space between sheets in page view, in rows.
const SHEET_GAP_ROWS: f32 = 1.5;

pub fn install_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    fonts.font_data.insert(COURIER.into(), Arc::new(FontData::from_static(storyteller_pdf::COURIER_PRIME_REGULAR)));
    fonts.families.insert(FontFamily::Name(COURIER.into()), vec![COURIER.into()]);
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

#[derive(Default)]
pub struct EditorState {
    /// Open while choosing an element for an empty paragraph.
    pub picker: Option<Picker>,
    /// The element that had focus last frame, to give it back after Esc.
    last_focused: Option<usize>,
    /// Focus this element at this byte offset on the next frame.
    pub pending_focus: Option<(usize, usize)>,
    pub focused: Option<usize>,
    /// The current find match, `(element, start, end)` in bytes, drawn highlighted.
    pub highlight: Option<(usize, usize, usize)>,
    /// Scroll this element into view on the next frame (a find match) without focusing it.
    pub scroll_to: Option<usize>,
    scroll_to_focus: bool,
    first_row: bool,
    last_row: bool,
    pub completion: Completion,
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
}

pub fn char_to_byte(text: &str, chars: usize) -> usize {
    text.char_indices().nth(chars).map_or(text.len(), |(byte, _)| byte)
}

pub fn byte_to_char(text: &str, byte: usize) -> usize {
    text.char_indices().take_while(|(b, _)| *b < byte).count()
}

fn align(align: PageAlign) -> Align {
    match align {
        PageAlign::Left => Align::LEFT,
        PageAlign::Right => Align::RIGHT,
        PageAlign::Center => Align::Center,
    }
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
    let element_pages = session.paginated().element_pages.clone();
    let session: &Session = session;
    let layout = &session.layout;
    let col = 0.1 * PX_PER_INCH * zoom;
    let row = PX_PER_INCH / 6.0 * zoom;
    // Text advances as Final Draft's does (see `WRAP_ADVANCE_POINTS`), so lines wrap where they
    // print: Courier Prime a touch smaller, on the same 12-point line.
    let squeeze = WRAP_ADVANCE_POINTS / POINTS_PER_COLUMN;
    let advance = col * squeeze;
    let font = FontId::new(row * squeeze, FontFamily::Name(COURIER.into()));
    // A text box's minimum height and caret come from its own font's row height; size that font
    // so one row is exactly one script line, or single-line elements would stand apart.
    let font_row = ui.fonts_mut(|f| f.row_height(&font)).max(1.0);
    let box_font = FontId::new(font.size * (row / font_row), font.family.clone());
    let page_width = f32::from(layout.page_columns) * col;
    let page_height = f32::from(layout.page_rows) * row;
    let top_margin = f32::from(layout.body_top) * row;
    let ink = Color32::from_gray(25);
    let faint = crate::theme::faint_ink();
    let count = session.script.elements.len();
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
                    *ui.visuals_mut() = egui::Visuals::light();
                    ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
                    // A text box is exactly one script line tall, at any zoom.
                    ui.spacing_mut().interact_size.y = row;
                    let mut in_speech = false;
                    let mut last_page = 0u32;
                    for (index, element) in session.script.elements.iter().enumerate() {
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
                        let id = Id::new(("storyteller.element", index));
                        let mut text = element.text.clone();
                        let focused = ui.memory(|m| m.has_focus(id));
                        let single_line = kind.is_single_line();
                        let picking = editor.picker.as_ref().is_some_and(|p| p.index == index);
                        if focused && picking {
                            picker_keys(ui, index, editor, &mut actions);
                        } else if focused {
                            handle_keys(ui, id, index, count, &text, single_line, editor, &mut actions);
                        }
                        if let Some((target, offset)) = editor.pending_focus
                            && target == index
                        {
                            let mut state = TextEditState::load(ui.ctx(), id).unwrap_or_default();
                            state.cursor.set_char_range(Some(CCursorRange::one(CCursor::new(byte_to_char(&text, offset)))));
                            state.store(ui.ctx(), id);
                            ui.memory_mut(|m| m.request_focus(id));
                            editor.pending_focus = None;
                            editor.scroll_to_focus = true;
                        }
                        let color = if kind.is_printed() { ink } else { faint };
                        let text_align = align(style.align);
                        let row_height = row;
                        let job_font = font.clone();
                        let marked = editor.highlight.filter(|h| h.0 == index).map(|h| (h.1, h.2));
                        let mark = crate::theme::FIND_MARK;
                        let mut layouter = move |ui: &Ui, buffer: &dyn egui::TextBuffer, wrap_width: f32| {
                            let mut job = highlighted_job(buffer.as_str(), marked, &job_font, color, mark, wrap_width);
                            job.halign = text_align;
                            for section in &mut job.sections {
                                section.format.line_height = Some(row_height);
                            }
                            ui.fonts_mut(|f| f.layout_job(job))
                        };
                        let (left, width) = match kind {
                            ElementKind::Section | ElementKind::Synopsis => (layout.action.left, layout.action.width),
                            _ => (style.left, style.width),
                        };
                        let output = ui
                            .horizontal_top(|ui| {
                                ui.add_space(f32::from(left) * col);
                                TextEdit::multiline(&mut text)
                                    .id(id)
                                    .frame(Frame::NONE)
                                    .margin(Margin::ZERO)
                                    .desired_width(f32::from(width) * advance + 1.0)
                                    .desired_rows(1)
                                    .lock_focus(true)
                                    .font(box_font.clone())
                                    .horizontal_align(text_align)
                                    .hint_text(egui::RichText::new(kind.label()).color(Color32::from_gray(205)).font(font.clone()))
                                    .layouter(&mut layouter)
                                    .show(ui)
                            })
                            .inner;
                        let response = &output.response.response;
                        // The edit's own id holds focus; the outer response may carry another id.
                        let has_focus = ui.memory(|m| m.has_focus(id));
                        automation.add(
                            format!("editor.element.{index}"),
                            "textbox",
                            format!("{}: {}", kind.label(), preview(&element.text)),
                            response,
                        );
                        if kind == ElementKind::SceneHeading
                            && layout.show_scene_numbers
                            && let Some(number) = &element.scene_number
                        {
                            scene_numbers(ui, response.rect, number, layout.scene_number_left_end, layout.scene_number_right, col, &font, ink);
                        }
                        if kind == ElementKind::Character && automatic.get(index).copied().unwrap_or(false) {
                            // The automatic (CONT'D), drawn faint: it is added at layout, not typed.
                            let at = output.galley_pos + output.galley.rect.right_top().to_vec2() + vec2(advance, 0.0);
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
                        let caret_char = output.cursor_range.map(|r| r.primary.index.into());
                        if response.changed() {
                            let caret = caret_char.map_or(text.len(), |c| char_to_byte(&text, c));
                            actions.push(EditAction::SetText { index, text: text.clone(), caret });
                        }
                        if has_focus {
                            editor.focused = Some(index);
                            if let Some(cursor) = output.cursor_range {
                                let pos = output.galley.pos_from_cursor(cursor.primary);
                                editor.first_row = pos.min.y < row * 0.5;
                                editor.last_row = pos.max.y > output.galley.rect.height() - row * 0.5;
                                if !response.changed() {
                                    actions.push(EditAction::Caret { index, offset: char_to_byte(&text, cursor.primary.index.into()) });
                                }
                            }
                            if picking {
                                picker_popup(ui, editor, automation, index, response.rect, &mut actions);
                            } else {
                                completion_popup(ui, session, editor, automation, index, kind, &text, response.rect, &font, &mut actions);
                            }
                        }
                    }
                    let y = ui.cursor().top() + row * 6.0;
                    let bottom = if pages { (sheet_top + page_height).max(y) } else { y };
                    sheets.push((sheet_top, bottom, sheet_page));
                    ui.add_space(bottom - ui.cursor().top());
                    ui.painter().set(paper, egui::Shape::Vec(paper_shapes(&sheets, column_left, page_width, pages)));
                    if pages {
                        // Page numbers sit in the top margin, clear of the text.
                        for (top, _, number) in sheets.iter().filter(|s| s.2 > 1) {
                            let at = pos2(column_left + f32::from(layout.page_number_right) * col, top + f32::from(layout.page_number_row) * row);
                            ui.painter().text(at, egui::Align2::RIGHT_TOP, format!("{number}."), font.clone(), ink);
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
fn paper_shapes(sheets: &[(f32, f32, u32)], left: f32, width: f32, pages: bool) -> Vec<egui::Shape> {
    let mut shapes = Vec::new();
    let spans: Vec<(f32, f32)> = if pages {
        sheets.iter().map(|s| (s.0, s.1)).collect()
    } else {
        let top = sheets.first().map_or(0.0, |s| s.0);
        let bottom = sheets.last().map_or(top, |s| s.1);
        vec![(top, bottom)]
    };
    for (top, bottom) in spans {
        let rect = Rect::from_min_max(pos2(left, top), pos2(left + width, bottom));
        for (spread, alpha) in [(6.0, 10u8), (3.0, 18), (1.0, 34)] {
            shapes.push(egui::Shape::rect_filled(rect.expand(spread).translate(vec2(0.0, spread * 0.5)), 3.0, Color32::from_black_alpha(alpha)));
        }
        shapes.push(egui::Shape::rect_filled(rect, 1.0, Color32::WHITE));
    }
    shapes
}

#[allow(clippy::too_many_arguments)]
fn handle_keys(
    ui: &mut Ui,
    id: Id,
    index: usize,
    count: usize,
    text: &str,
    single_line: bool,
    editor: &mut EditorState,
    actions: &mut Vec<EditAction>,
) {
    let range = TextEditState::load(ui.ctx(), id).and_then(|s| s.cursor.char_range());
    let caret_char = range.map_or_else(|| text.chars().count(), |r| r.primary.index.into());
    let has_selection = range.is_some_and(|r| r.primary != r.secondary);
    let caret = char_to_byte(text, caret_char);
    let popup = editor.completion.visible_for(index);
    ui.input_mut(|input| {
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
                actions.push(EditAction::SetText {
                    index,
                    text: format!("{}\n{}", text.get(..caret).unwrap_or(""), text.get(caret..).unwrap_or("")),
                    caret: caret + 1,
                });
                actions.push(EditAction::Focus { index, offset: caret + 1 });
            }
        } else if input.consume_key(Modifiers::NONE, Key::Enter) {
            actions.push(EditAction::Enter { index, offset: caret });
        } else if input.consume_key(Modifiers::SHIFT, Key::Tab) {
            actions.push(EditAction::Tab { index, back: true });
        } else if input.consume_key(Modifiers::NONE, Key::Tab) {
            actions.push(EditAction::Tab { index, back: false });
        } else if index > 0 && !has_selection && caret_char == 0 && input.consume_key(Modifiers::NONE, Key::Backspace) {
            actions.push(EditAction::MergePrevious { index });
        } else if !popup && index > 0 && editor.first_row && input.consume_key(Modifiers::NONE, Key::ArrowUp) {
            actions.push(EditAction::Focus { index: index - 1, offset: usize::MAX });
        } else if !popup && index + 1 < count && editor.last_row && input.consume_key(Modifiers::NONE, Key::ArrowDown) {
            actions.push(EditAction::Focus { index: index + 1, offset: 0 });
        }
    });
}

fn page_break_marker(ui: &mut Ui, page: u32, row: f32, font: &FontId, color: Color32) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), row * 2.0), egui::Sense::hover());
    let y = rect.center().y;
    let painter = ui.painter();
    let mut x = rect.left();
    while x < rect.right() {
        painter.line_segment([pos2(x, y), pos2((x + 6.0).min(rect.right()), y)], Stroke::new(1.0, Color32::from_gray(190)));
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

#[allow(clippy::too_many_arguments)]
fn scene_numbers(ui: &Ui, rect: Rect, number: &str, left_end: u16, right: u16, col: f32, font: &FontId, color: Color32) {
    let page_left = ui.min_rect().left();
    let painter = ui.painter();
    let top = rect.top();
    painter.text(pos2(page_left + f32::from(left_end) * col, top), egui::Align2::RIGHT_TOP, number, font.clone(), color);
    painter.text(pos2(page_left + f32::from(right) * col, top), egui::Align2::LEFT_TOP, number, font.clone(), color);
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
    egui::Area::new(Id::new("storyteller.autocomplete")).order(Order::Foreground).fixed_pos(rect.left_bottom() + vec2(0.0, 2.0)).show(
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
    egui::Area::new(Id::new("storyteller.picker")).order(Order::Foreground).fixed_pos(rect.left_bottom() + vec2(0.0, 2.0)).show(ui.ctx(), |ui| {
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

/// One element's text laid out, with an optional byte range drawn as a find match.
fn highlighted_job(text: &str, marked: Option<(usize, usize)>, font: &FontId, color: Color32, mark: Color32, wrap_width: f32) -> LayoutJob {
    let range = marked.filter(|&(start, end)| start < end && end <= text.len() && text.is_char_boundary(start) && text.is_char_boundary(end));
    let Some((start, end)) = range else {
        return LayoutJob::simple(text.to_owned(), font.clone(), color, wrap_width);
    };
    let mut job = LayoutJob { wrap: egui::text::TextWrapping { max_width: wrap_width, ..Default::default() }, ..Default::default() };
    let plain = egui::TextFormat { font_id: font.clone(), color, ..Default::default() };
    let found = egui::TextFormat { background: mark, ..plain.clone() };
    for (part, format) in [(text.get(..start), &plain), (text.get(start..end), &found), (text.get(end..), &plain)] {
        let part = part.unwrap_or("");
        job.append(part, 0.0, format.clone());
    }
    job
}
