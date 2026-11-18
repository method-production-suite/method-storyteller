//! The scene navigator beside the editor.

use super::*;

impl StorytellerApp {
    pub(super) fn navigator(&mut self, ui: &mut egui::Ui) {
        // While the writer types, pages and lengths are the ones from before (see `hold_pages`).
        let scenes = self.session.shown_pages().scenes.clone();
        let lengths: std::collections::HashMap<usize, u32> = self.session.shown_scene_lengths().iter().map(|l| (l.element, l.eighths)).collect();
        let current = self.current_scene();
        ui.add_space(theme::SPACE_2);
        ui.label(egui::RichText::new("SCENES").small().strong().color(ui.visuals().weak_text_color()));
        ui.add_space(theme::SPACE_1);
        let mut copy_from = None;
        let mut properties_of = None;
        // Each New Act line heads the scenes after it; an act with no scenes yet still shows.
        let acts: Vec<(usize, String)> = self
            .session
            .script
            .elements
            .iter()
            .enumerate()
            .filter(|(_, e)| e.kind == ElementKind::NewAct)
            .map(|(i, e)| (i, e.text.trim().to_string()))
            .collect();
        let mut next_act = 0;
        let mut act_clicked = None;
        // The rows as drawn, for a scene being dragged to find where it would land.
        let mut rows: Vec<NavRow> = Vec::with_capacity(scenes.len() + acts.len());
        let mut drag_from = self.dragging_scene;
        let mut dropped = None;
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            // A new script's one empty heading is not a scene yet.
            let blank = matches!(self.session.script.elements.as_slice(), [only] if only.text.trim().is_empty());
            if scenes.is_empty() || blank {
                theme::empty_state(ui, crate::messages::NO_SCENES_TITLE, crate::messages::NO_SCENES_BODY);
            }
            let weak = ui.visuals().weak_text_color();
            let text = ui.visuals().text_color();
            let selected_fill = ui.visuals().selection.bg_fill;
            let hover_fill = ui.visuals().widgets.hovered.weak_bg_fill;
            let ink_soft = ui.visuals().widgets.noninteractive.fg_stroke.color;
            let act_row = |ui: &mut egui::Ui, automation: &mut Automation, k: usize, name: &str| {
                let name = if name.is_empty() { "(untitled act)" } else { name };
                let (rect, response) = ui.allocate_exact_size(egui::vec2(ui.available_width(), theme::NAV_ROW_HEIGHT), egui::Sense::click());
                if response.hovered() {
                    ui.painter().rect_filled(rect, 5.0, hover_fill);
                }
                // Small capitals in ink-soft, the token section labels use.
                let font = egui::FontId::proportional(theme::TYPE_SMALL);
                ui.painter().with_clip_rect(rect).text(
                    rect.left_center() + egui::vec2(8.0, 0.0),
                    egui::Align2::LEFT_CENTER,
                    name.to_uppercase(),
                    font,
                    ink_soft,
                );
                automation.add(format!("navigator.act.{k}"), "listitem", name.to_string(), &response);
                (response.clicked(), rect)
            };
            for (n, scene) in scenes.iter().enumerate().filter(|_| !blank) {
                while let Some((at, name)) = acts.get(next_act) {
                    if *at > scene.element {
                        break;
                    }
                    let (clicked, rect) = act_row(ui, &mut self.automation, next_act, name);
                    if clicked {
                        act_clicked = Some(*at);
                    }
                    rows.push(NavRow { rect, element: *at, act: true });
                    next_act += 1;
                }
                let number = scene.number.as_deref().unwrap_or("");
                // The heading as typed now, which the held pages may not have yet.
                let heading = self
                    .session
                    .script
                    .elements
                    .get(scene.element)
                    .map_or_else(|| scene.heading.clone(), |e| storyteller_core::inline::plain(&e.text).to_uppercase());
                let heading = if heading.trim().is_empty() { "(untitled scene)".to_string() } else { heading };
                // A scene's own title stands in for its heading; its colour marks the row.
                let meta = self.session.script.elements.get(scene.element).map(|e| e.scene.clone()).unwrap_or_default();
                let shown = if meta.title.is_empty() { heading.clone() } else { meta.title.clone() };
                // An omitted scene's row is muted: it is there, but not shot.
                let omitted = self.session.script.elements.get(scene.element).is_some_and(storyteller_core::Element::is_omitted);
                let text = if omitted { weak } else { text };
                let (rect, response) = ui.allocate_exact_size(egui::vec2(ui.available_width(), theme::NAV_ROW_HEIGHT), egui::Sense::click_and_drag());
                rows.push(NavRow { rect, element: scene.element, act: false });
                if response.drag_started() {
                    drag_from = Some(scene.element);
                }
                let selected = n + 1 == current;
                if selected || response.hovered() {
                    ui.painter().rect_filled(rect, 5.0, if selected { selected_fill } else { hover_fill });
                }
                let painter = ui.painter().with_clip_rect(rect);
                if let Some(colour) = meta.rgb() {
                    let stripe = egui::Rect::from_min_size(rect.left_top() + egui::vec2(2.0, 4.0), egui::vec2(4.0, rect.height() - 8.0));
                    painter.rect_filled(stripe, 2.0, theme::scene_stripe(colour, ui.visuals().dark_mode));
                }
                let small = egui::FontId::proportional(theme::TYPE_SMALL);
                painter.text(rect.left_center() + egui::vec2(30.0, 0.0), egui::Align2::RIGHT_CENTER, number, small.clone(), weak);
                painter.text(rect.right_center() - egui::vec2(8.0, 0.0), egui::Align2::RIGHT_CENTER, scene.page.to_string(), small.clone(), weak);
                // Length in eighths of a page, as a schedule counts it.
                let length = lengths.get(&scene.element).map(|&e| storyteller_core::paginate::format_eighths(e));
                if let Some(length) = &length {
                    painter.text(rect.right_center() - egui::vec2(34.0, 0.0), egui::Align2::RIGHT_CENTER, length, small, weak);
                }
                let galley = ui.fonts_mut(|f| {
                    let mut job = egui::text::LayoutJob::simple_singleline(shown.clone(), egui::FontId::proportional(theme::TYPE_BODY), text);
                    job.wrap = egui::text::TextWrapping::truncate_at_width(rect.width() - 118.0);
                    f.layout_job(job)
                });
                painter.galley(rect.left_center() + egui::vec2(38.0, -galley.size().y / 2.0), galley, text);
                let mut detail = match &length {
                    Some(length) => format!("{heading}\nPage {}, length {length}", scene.page),
                    None => format!("{heading}\nPage {}", scene.page),
                };
                if !meta.title.is_empty() {
                    detail = format!("{}\n{detail}", meta.title);
                }
                if !meta.summary.is_empty() {
                    detail = format!("{detail}\n\n{}", meta.summary);
                }
                let response = response.on_hover_text(detail);
                self.automation.add(format!("navigator.scene.{n}"), "listitem", shown.clone(), &response);
                let element = scene.element;
                response.context_menu(|ui| {
                    if ui.button("Scene Properties…").clicked() {
                        properties_of = Some(element);
                        ui.close();
                    }
                    if ui.button("Copy Scene").clicked() {
                        copy_from = Some(element);
                        ui.close();
                    }
                });
                if response.clicked() {
                    // Straight into the scene's action, ready to read or write on.
                    let entry = scene_entry(&self.session.script, scene.element);
                    self.session.caret = Caret { element: entry, offset: 0 };
                    // On the next frame: egui drops the focus at the end of this one, since the
                    // click landed on no text field.
                    self.focus_next_frame = Some((entry, 0));
                }
            }
            for (k, (at, name)) in acts.iter().enumerate().skip(next_act) {
                let (clicked, rect) = act_row(ui, &mut self.automation, k, name);
                if clicked {
                    act_clicked = Some(*at);
                }
                rows.push(NavRow { rect, element: *at, act: true });
            }
            if let Some(from) = drag_from {
                let (pointer, released) = ui.input(|i| (i.pointer.interact_pos(), i.pointer.primary_released() || !i.pointer.primary_down()));
                if let Some(pointer) = pointer {
                    let (to, y) = drop_place(&self.session.script, &rows, pointer.y);
                    let (left, right) = (ui.min_rect().left(), ui.min_rect().right());
                    let stroke = egui::Stroke::new(2.0, ui.visuals().selection.stroke.color);
                    ui.painter().hline(left..=right, y, stroke);
                    let clip = ui.clip_rect();
                    let delta = if pointer.y < clip.top() + 24.0 {
                        6.0
                    } else if pointer.y > clip.bottom() - 24.0 {
                        -6.0
                    } else {
                        0.0
                    };
                    if delta != 0.0 {
                        ui.scroll_with_delta(egui::vec2(0.0, delta));
                    }
                    if released {
                        dropped = Some((from, to));
                    }
                }
                if released {
                    drag_from = None;
                }
            }
        });
        self.dragging_scene = drag_from;
        if let Some((from, to)) = dropped {
            let moved = self.registry.run("scene.move", &mut self.session, &params(&[("index", json!(from)), ("to", json!(to))]));
            match moved {
                Ok(outcome) => {
                    if outcome.value.get("moved").and_then(Value::as_bool) == Some(true) {
                        self.status = "Moved the scene".to_string();
                        self.focus_next_frame = Some((self.session.caret.element, 0));
                    }
                    self.error = None;
                }
                Err(error) => self.error = Some(error.to_string()),
            }
        }
        if let Some(act) = act_clicked {
            // Into the act's first scene, as a scene click does; with no scene, onto the act line.
            let script = &self.session.script;
            let first_scene = script
                .elements
                .iter()
                .enumerate()
                .skip(act + 1)
                .take_while(|(_, e)| e.kind != ElementKind::NewAct)
                .find(|(_, e)| e.kind == ElementKind::SceneHeading)
                .map(|(i, _)| i);
            let entry = first_scene.map_or(act, |heading| scene_entry(script, heading));
            self.session.caret = Caret { element: entry, offset: 0 };
            self.focus_next_frame = Some((entry, 0));
        }
        if let Some(heading) = properties_of {
            self.open_scene_dialog(heading);
        }
        if let Some(heading) = copy_from {
            let ctx = ui.ctx().clone();
            self.copy_scene(&ctx, Some(heading));
        }
    }
}

/// A row of the scene navigator: a scene's or an act's, and its element.
struct NavRow {
    rect: egui::Rect,
    element: usize,
    act: bool,
}

/// Where a scene dropped at height `y` among `rows` would go: the element it would start before,
/// and the height to draw the line at. Above an act's row is the end of the act before it; below
/// it, the act's first place.
fn drop_place(script: &storyteller_core::Script, rows: &[NavRow], y: f32) -> (usize, f32) {
    let end_of = |row: &NavRow| storyteller_engine::commands::scene_body(script, row.element).end;
    for (n, row) in rows.iter().enumerate() {
        if y < row.rect.center().y {
            let to = match (row.act, n.checked_sub(1).and_then(|p| rows.get(p))) {
                (false, _) => row.element,
                // The scene above ends this act: after it, ahead of its closing line.
                (true, Some(above)) if !above.act => end_of(above),
                (true, _) => row.element,
            };
            return (to, row.rect.top());
        }
        if row.act && rows.get(n + 1).is_none_or(|next| y < next.rect.center().y) {
            return (row.element + 1, row.rect.bottom());
        }
    }
    match rows.last() {
        Some(last) if !last.act => (end_of(last), last.rect.bottom()),
        Some(last) => (last.element + 1, last.rect.bottom()),
        None => (script.elements.len(), y),
    }
}

/// Where a click on a scene in the navigator puts the caret: the scene's first action line;
/// with none, the first line under the heading (a scene that opens on dialogue); with nothing
/// under it, the heading itself.
fn scene_entry(script: &storyteller_core::Script, heading: usize) -> usize {
    let scene: Vec<(usize, ElementKind)> = script
        .elements
        .iter()
        .enumerate()
        .skip(heading + 1)
        .take_while(|(_, e)| !matches!(e.kind, ElementKind::SceneHeading | ElementKind::EndOfAct | ElementKind::NewAct))
        .filter(|(_, e)| e.kind.is_printed())
        .map(|(i, e)| (i, e.kind))
        .collect();
    scene.iter().find(|(_, kind)| *kind == ElementKind::Action).or_else(|| scene.first()).map_or(heading, |&(i, _)| i)
}
