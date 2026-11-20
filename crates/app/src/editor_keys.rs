//! The editor's keys and pointer: inside a paragraph, across paragraphs, and in the element
//! picker.

use super::*;

/// A selection across paragraphs has the keyboard ahead of the text fields: what is typed
/// replaces it, Copy and Cut take all of it, and the arrows move or collapse it.
pub(super) fn span_keys(ui: &mut Ui, session: &Session, editor: &mut EditorState, actions: &mut Vec<EditAction>) {
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
pub(super) fn settle(ui: &Ui, editor: &mut EditorState, span: Span) {
    if span.anchor.element != span.head.element {
        editor.span = Some(span);
        return;
    }
    editor.span = None;
    let id = Id::unique(("storyteller.element", span.head.element));
    editor.cursor = Some(Cursor { index: span.head.element, anchor: span.anchor.offset, head: span.head.offset });
    ui.memory_mut(|m| m.request_focus(id));
}

/// The pointer's part in a selection across paragraphs: Shift and a click in another paragraph,
/// or a drag from one into another. Inside one paragraph the text field does it all.
pub(super) fn pointer_span(ui: &Ui, session: &Session, editor: &mut EditorState, fields: &[Field], (caret, selection): (Caret, Option<Cursor>)) {
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
pub(super) fn handle_keys(
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
pub(super) fn field_keys(
    ui: &mut Ui,
    index: usize,
    text: &mut String,
    show: ShowAs,
    editor: &mut EditorState,
    actions: &mut Vec<EditAction>,
) -> bool {
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
pub(super) fn pointer_keys(ui: &Ui, response: &egui::Response, index: usize, laid: &Laid, editor: &mut EditorState, now: f64) -> bool {
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

/// Keys while the element picker is open: a choice's letter, arrows and Enter, or Esc.
pub(super) fn picker_keys(ui: &mut Ui, index: usize, editor: &mut EditorState, actions: &mut Vec<EditAction>) {
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
