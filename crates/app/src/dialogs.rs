//! The dialogs and windows the menus open.

use super::*;

impl StorytellerApp {
    pub(super) fn title_page_dialog(&mut self, ctx: &egui::Context) {
        let Some(draft) = &mut self.title_dialog else { return };
        let mut done = None;
        theme::centered(egui::Window::new("Title Page").collapsible(false).resizable(false), ctx).show(ctx, |ui| {
            egui::Grid::new("storyteller.title_grid").num_columns(2).spacing([12.0, 6.0]).show(ui, |ui| {
                for (key, value) in &mut draft.fields {
                    ui.label(key.as_str());
                    let multiline = matches!(key.as_str(), "Contact" | "Notes" | "Title" | "Author");
                    let edit = if multiline { egui::TextEdit::multiline(value).desired_rows(2) } else { egui::TextEdit::singleline(value) };
                    let response = theme::field(ui, edit.desired_width(320.0));
                    self.automation.add(format!("dialog.title_page.{}", key.to_lowercase().replace(' ', "_")), "textbox", key.clone(), &response);
                    ui.end_row();
                }
            });
            ui.add_space(theme::SPACE_2);
            ui.horizontal(|ui| {
                let ok = ui.button("OK");
                self.automation.add("dialog.title_page.ok", "button", "OK", &ok);
                let cancel = ui.button("Cancel");
                self.automation.add("dialog.title_page.cancel", "button", "Cancel", &cancel);
                if ok.clicked() {
                    done = Some(true);
                }
                if cancel.clicked() {
                    done = Some(false);
                }
            });
        });
        match done {
            Some(true) => {
                if let Some(draft) = self.title_dialog.take() {
                    for (key, value) in draft.fields {
                        if self.session.script.title_page.get(&key).unwrap_or("") != value.trim_end() {
                            self.run("title_page.set", params(&[("key", json!(key)), ("value", json!(value))]));
                        }
                    }
                }
            }
            Some(false) => self.title_dialog = None,
            None => {}
        }
    }

    /// Production > Characters: every speaking character with their speeches, scenes, words
    /// and first scene. A name takes the caret to the character's first cue.
    pub(super) fn characters_window(&mut self, ctx: &egui::Context) {
        if !self.characters {
            return;
        }
        let rows = storyteller_engine::commands::character_table(&self.session.script);
        let mut open = true;
        let mut go = None;
        egui::Window::new("Characters").open(&mut open).collapsible(false).default_width(420.0).show(ctx, |ui| {
            if rows.is_empty() {
                ui.label("No one speaks yet. Character cues appear here as you write them.");
                return;
            }
            egui::ScrollArea::vertical().max_height(420.0).show(ui, |ui| {
                egui::Grid::new("storyteller.character_table").num_columns(5).striped(true).spacing([18.0, 6.0]).show(ui, |ui| {
                    for heading in ["Character", "Speeches", "Scenes", "Words", "First scene"] {
                        ui.label(egui::RichText::new(heading).small().strong());
                    }
                    ui.end_row();
                    for (n, row) in rows.iter().enumerate() {
                        let name = ui.link(row.name.as_str()).on_hover_text("Go to their first line");
                        self.automation.add(format!("characters.row.{n}"), "link", row.name.clone(), &name);
                        if name.clicked() {
                            go = Some(row.first);
                        }
                        ui.label(row.speeches.to_string());
                        ui.label(row.scenes.to_string());
                        ui.label(row.words.to_string());
                        ui.label(row.first_scene.clone().unwrap_or_else(|| "–".into()));
                        ui.end_row();
                    }
                });
            });
        });
        self.characters = open;
        if let Some(cue) = go {
            self.session.caret = Caret { element: cue, offset: 0 };
            self.focus_next_frame = Some((cue, 0));
            ctx.request_repaint();
        }
    }

    /// Production > Reports…: the scene, location, character and cast reports, each read here as a
    /// table and exported as CSV or printed as a PDF table.
    pub(super) fn reports_window(&mut self, ctx: &egui::Context) {
        let Some(current) = self.reports else { return };
        let Some(table) = storyteller_engine::reports::table_for(&mut self.session, current) else {
            self.reports = None;
            return;
        };
        let shown = table.shown();
        let mut open = true;
        let mut pick = None;
        let mut export = None;
        egui::Window::new("Reports").open(&mut open).collapsible(false).default_width(760.0).show(ctx, |ui| {
            ui.horizontal(|ui| {
                for name in storyteller_engine::reports::NAMES {
                    let label = storyteller_engine::reports::title(name);
                    let tab = ui.selectable_label(name == current, label);
                    self.automation.add(format!("reports.tab.{name}"), "tab", label, &tab);
                    if tab.clicked() {
                        pick = Some(name);
                    }
                }
            });
            ui.label(egui::RichText::new(&table.summary).small());
            ui.separator();
            egui::ScrollArea::both().max_height(400.0).auto_shrink([false, true]).show(ui, |ui| {
                if table.rows.is_empty() {
                    ui.label("Nothing to report yet. Scenes and speaking characters appear here as you write them.");
                    return;
                }
                egui::Grid::new(("storyteller.report", current)).num_columns(shown.len()).striped(true).spacing([16.0, 4.0]).show(ui, |ui| {
                    for heading in shown.iter().filter_map(|&c| table.header.get(c)) {
                        ui.label(egui::RichText::new(*heading).small().strong());
                    }
                    ui.end_row();
                    for row in &table.rows {
                        for &c in &shown {
                            let cell = row.get(c).map_or("", String::as_str);
                            ui.add(egui::Label::new(cell).wrap_mode(egui::TextWrapMode::Extend));
                        }
                        ui.end_row();
                    }
                });
            });
            ui.separator();
            ui.horizontal(|ui| {
                let csv = ui.button("Export CSV…");
                self.automation.add("reports.export_csv", "button", "Export CSV…", &csv);
                if csv.clicked() {
                    export = Some("csv");
                }
                let pdf = ui.button("Export PDF…");
                self.automation.add("reports.export_pdf", "button", "Export PDF…", &pdf);
                if pdf.clicked() {
                    export = Some("pdf");
                }
            });
        });
        self.reports = if open { Some(pick.unwrap_or(current)) } else { None };
        if let Some(extension) = export
            && let Some(path) = self.export_path_as(table.name, extension)
        {
            let id = if current == "characters" { "character.table".to_string() } else { format!("report.{current}") };
            let mut params = Map::new();
            params.insert("path".into(), json!(path.display().to_string()));
            self.run(&id, params);
        }
    }

    /// Production > Number Scenes…: start number, keep existing (letters for new scenes), from
    /// the caret's scene on, and whether I and O are left out.
    pub(super) fn pages_confirm_dialog(&mut self, ctx: &egui::Context) {
        let Some(confirm) = &mut self.pages_confirm else { return };
        let mut go = false;
        let mut close = ctx.input(|i| i.key_pressed(egui::Key::Escape));
        let (title, action) = match confirm {
            PagesConfirm::Lock { .. } => ("Lock Pages", "Lock"),
            PagesConfirm::UnlockAll => ("Unlock All Pages", "Unlock"),
        };
        egui::Window::new(title).collapsible(false).resizable(false).anchor(egui::Align2::CENTER_TOP, [0.0, 120.0]).show(ctx, |ui| {
            match confirm {
                PagesConfirm::Lock { revision_mode } => {
                    ui.label(
                        "Each page keeps its number from now on. New text flows onto A-pages, such as 12A, and a page that loses text stays short.",
                    );
                    if !self.session.script.revisions.mode {
                        let mode = ui.checkbox(revision_mode, "Turn on revision mode too");
                        self.automation.add("dialog.lock_pages.revision_mode", "checkbox", "Turn on revision mode too", &mode);
                    }
                }
                PagesConfirm::UnlockAll => {
                    ui.label("The pages will number straight through again, and A-pages go. Edit > Undo puts the locks back.");
                }
            }
            ui.add_space(theme::SPACE_2);
            ui.horizontal(|ui| {
                let yes = ui.button(action);
                self.automation.add("dialog.pages_confirm.ok", "button", action, &yes);
                let cancel = ui.button("Cancel");
                self.automation.add("dialog.pages_confirm.cancel", "button", "Cancel", &cancel);
                go |= yes.clicked();
                close |= cancel.clicked();
            });
        });
        if close {
            self.pages_confirm = None;
            self.refocus();
            return;
        }
        if !go {
            return;
        }
        let run = match self.pages_confirm.take() {
            Some(PagesConfirm::Lock { revision_mode }) => {
                self.registry.run("pages.lock", &mut self.session, &params(&[("revision_mode", json!(revision_mode))]))
            }
            Some(PagesConfirm::UnlockAll) => self.registry.run("pages.unlock_all", &mut self.session, &Map::new()),
            None => return,
        };
        match run {
            Ok(outcome) => {
                if let Some(message) = outcome.message {
                    self.status = message;
                }
            }
            Err(error) => self.status = error.to_string(),
        }
        self.refocus();
        ctx.request_repaint();
    }

    pub(super) fn numbering_dialog(&mut self, ctx: &egui::Context) {
        let Some(draft) = &mut self.numbering else { return };
        let mut ok = false;
        let mut close = ctx.input(|i| i.key_pressed(egui::Key::Escape));
        egui::Window::new("Number Scenes").collapsible(false).resizable(false).anchor(egui::Align2::CENTER_TOP, [0.0, 120.0]).show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label("Start at");
                let start = ui.add_enabled(!draft.keep_existing, egui::TextEdit::singleline(&mut draft.start).desired_width(60.0));
                theme::field_border(ui, &start);
                self.automation.add("dialog.numbering.start", "textbox", "Start at", &start);
            });
            let keep = ui.checkbox(&mut draft.keep_existing, "Keep existing numbers (new scenes get letters, such as 12A)");
            self.automation.add("dialog.numbering.keep", "checkbox", "Keep existing numbers", &keep);
            let from = ui.checkbox(&mut draft.from_caret, "From the current scene on");
            self.automation.add("dialog.numbering.from_caret", "checkbox", "From the current scene on", &from);
            let skip = ui.checkbox(&mut draft.skip_io, "Leave out the letters I and O");
            self.automation.add("dialog.numbering.skip_io", "checkbox", "Leave out the letters I and O", &skip);
            ui.add_space(theme::SPACE_2);
            let left = ui.checkbox(&mut draft.left, "Show in the left margin");
            self.automation.add("dialog.numbering.left", "checkbox", "Show in the left margin", &left);
            let right = ui.checkbox(&mut draft.right, "Show in the right margin");
            self.automation.add("dialog.numbering.right", "checkbox", "Show in the right margin", &right);
            if let Some(error) = &draft.error {
                ui.colored_label(ui.visuals().error_fg_color, error);
            }
            ui.add_space(theme::SPACE_2);
            ui.horizontal(|ui| {
                let number = ui.button("Number");
                self.automation.add("dialog.numbering.ok", "button", "Number", &number);
                let cancel = ui.button("Cancel");
                self.automation.add("dialog.numbering.cancel", "button", "Cancel", &cancel);
                ok |= number.clicked();
                close |= cancel.clicked();
            });
        });
        if close {
            self.numbering = None;
            self.refocus();
            return;
        }
        if !ok {
            return;
        }
        let Some(start) = draft.start.trim().parse::<u64>().ok().filter(|n| *n >= 1) else {
            draft.error = Some("Start at a whole number, 1 or more.".into());
            return;
        };
        let target = params(&[
            ("keep_existing", json!(draft.keep_existing)),
            ("start", json!(start)),
            ("from_caret", json!(draft.from_caret)),
            ("skip_io", json!(draft.skip_io)),
        ]);
        let skip_io = draft.skip_io;
        let margins = params(&[("left", json!(draft.left)), ("right", json!(draft.right))]);
        self.prefs.update(|p| p.general.skip_io_letters = skip_io);
        // The margins are their own Undo step, before the numbers.
        if let Err(error) = self.registry.run("scene.number_margins", &mut self.session, &margins) {
            if let Some(draft) = &mut self.numbering {
                draft.error = Some(error.to_string());
            }
            return;
        }
        match self.registry.run("scene.number", &mut self.session, &target) {
            Ok(outcome) => {
                self.numbering = None;
                if let Some(message) = outcome.message {
                    self.status = message;
                }
                self.refocus();
                ctx.request_repaint();
            }
            Err(error) => {
                if let Some(draft) = &mut self.numbering {
                    draft.error = Some(error.to_string());
                }
            }
        }
    }

    /// File > Export PDF…: all pages, revised pages only, a page range or a scene range; a clean
    /// copy; coloured revised pages. The choices are checked before asking where to save, and a
    /// problem shows in the dialog.
    pub(super) fn export_pdf_dialog(&mut self, ctx: &egui::Context) {
        let Some(draft) = &mut self.export_pdf else { return };
        let mut ok = false;
        let mut close = ctx.input(|i| i.key_pressed(egui::Key::Escape));
        egui::Window::new("Export PDF").collapsible(false).resizable(false).anchor(egui::Align2::CENTER_TOP, [0.0, 120.0]).show(ctx, |ui| {
            let all = ui.radio_value(&mut draft.which, ExportPages::All, "All pages");
            self.automation.add("dialog.export_pdf.all", "radio", "All pages", &all);
            let revised = ui.radio_value(&mut draft.which, ExportPages::Revised, "Revised pages only");
            self.automation.add("dialog.export_pdf.revised", "radio", "Revised pages only", &revised);
            ui.horizontal(|ui| {
                let pages = ui.radio_value(&mut draft.which, ExportPages::Pages, "Pages");
                self.automation.add("dialog.export_pdf.pages", "radio", "Pages", &pages);
                let text = theme::field(ui, egui::TextEdit::singleline(&mut draft.pages).hint_text("12-18, 22A").desired_width(140.0));
                self.automation.add("dialog.export_pdf.page_range", "textbox", "Page range", &text);
                if text.changed() {
                    draft.which = ExportPages::Pages;
                }
            });
            ui.horizontal(|ui| {
                let scenes = ui.radio_value(&mut draft.which, ExportPages::Scenes, "Scenes");
                self.automation.add("dialog.export_pdf.scenes", "radio", "Scenes", &scenes);
                let text = theme::field(ui, egui::TextEdit::singleline(&mut draft.scenes).hint_text("12-15").desired_width(140.0));
                self.automation.add("dialog.export_pdf.scene_range", "textbox", "Scene range", &text);
                if text.changed() {
                    draft.which = ExportPages::Scenes;
                }
            });
            ui.add_space(theme::SPACE_2);
            let clean = ui.checkbox(&mut draft.clean, "Clean copy (no revision marks, labels or colours)");
            self.automation.add("dialog.export_pdf.clean", "checkbox", "Clean copy", &clean);
            let colour = ui.add_enabled(!draft.clean, egui::Checkbox::new(&mut draft.colour, "Coloured revised pages"));
            self.automation.add("dialog.export_pdf.colour", "checkbox", "Coloured revised pages", &colour);
            if let Some(error) = &draft.error {
                ui.colored_label(ui.visuals().error_fg_color, error);
            }
            ui.add_space(theme::SPACE_2);
            ui.horizontal(|ui| {
                let export = ui.button("Export…");
                self.automation.add("dialog.export_pdf.ok", "button", "Export", &export);
                let cancel = ui.button("Cancel");
                self.automation.add("dialog.export_pdf.cancel", "button", "Cancel", &cancel);
                ok |= export.clicked();
                close |= cancel.clicked();
            });
        });
        if close {
            self.export_pdf = None;
            self.refocus();
            return;
        }
        if !ok {
            return;
        }
        draft.error = None;
        let which = draft.which;
        let selection = storyteller_core::paginate::PageSelection {
            pages: Some(draft.pages.clone()).filter(|_| which == ExportPages::Pages),
            scenes: Some(draft.scenes.clone()).filter(|_| which == ExportPages::Scenes),
            revised_only: which == ExportPages::Revised,
        };
        let mut target = params(&[("clean", json!(draft.clean)), ("colour", json!(draft.colour && !draft.clean))]);
        // Checked before the save dialog, so a mistyped range is fixed here.
        let show = self.session.show_revisions;
        let revisions = self.session.script.revisions.clone();
        let problem = if matches!(which, ExportPages::Pages | ExportPages::Scenes) && selection.is_all() {
            Some(if which == ExportPages::Pages { "Name the pages, such as 12-18, 22A." } else { "Name the scenes, such as 12-15." }.to_string())
        } else {
            match self.session.paginated().select(&selection, &revisions, show) {
                Err(error) => Some(sentence(&error)),
                Ok(chosen) if chosen.is_empty() => Some("No page matches these choices.".into()),
                Ok(_) => None,
            }
        };
        if let Some(problem) = problem {
            if let Some(draft) = &mut self.export_pdf {
                draft.error = Some(problem);
            }
            return;
        }
        if let Some(pages) = selection.pages {
            target.insert("pages".into(), json!(pages));
        }
        if let Some(scenes) = selection.scenes {
            target.insert("scenes".into(), json!(scenes));
        }
        if selection.revised_only {
            target.insert("revised_only".into(), json!(true));
        }
        let Some(path) = self.export_path(FileFormat::Pdf.extension()) else { return };
        target.insert("path".into(), json!(path.display().to_string()));
        match self.registry.run("file.export_pdf", &mut self.session, &target) {
            Ok(outcome) => {
                self.export_pdf = None;
                if let Some(message) = outcome.message {
                    self.status = message;
                }
                self.refocus();
            }
            Err(error) => {
                if let Some(draft) = &mut self.export_pdf {
                    draft.error = Some(error.to_string());
                }
            }
        }
    }

    /// Production > Revisions…: every set with its name, mark and page colour, the active one
    /// chosen, the next in the standard order added, and a set deleted (asking first when it
    /// marks anything). Each change is one Undo.
    pub(super) fn revisions_window(&mut self, ctx: &egui::Context) {
        if self.revisions.is_none() {
            return;
        }
        let sets = self.session.script.revisions.sets.clone();
        let active = self.session.script.revisions.active;
        let mut run: Vec<(&'static str, Vec<(&'static str, Value)>)> = Vec::new();
        let mut open = true;
        let Some(draft) = self.revisions.as_mut() else { return };
        egui::Window::new("Revisions").open(&mut open).collapsible(false).resizable(false).default_width(520.0).show(ctx, |ui| {
            if sets.is_empty() {
                ui.label("No revision sets yet. The first is White; each next one follows the standard colours.");
            } else {
                egui::Grid::new("storyteller.revisions").num_columns(6).striped(true).spacing([12.0, 6.0]).show(ui, |ui| {
                    for heading in ["Active", "Name", "Mark", "Page", "Marks", ""] {
                        ui.label(egui::RichText::new(heading).small().strong());
                    }
                    ui.end_row();
                    for set in &sets {
                        let on = ui.radio(active == Some(set.id), "");
                        self.automation.add(format!("revisions.active.{}", set.id), "radio", set.name.clone(), &on);
                        if on.clicked() {
                            run.push(("revisions.set_active", vec![("id", json!(set.id))]));
                        }
                        let name = draft.names.entry(set.id).or_insert_with(|| set.name.clone());
                        let field = theme::field(ui, egui::TextEdit::singleline(name).desired_width(220.0));
                        self.automation.add(format!("revisions.name.{}", set.id), "textbox", "Name", &field);
                        if field.lost_focus() && *name != set.name {
                            run.push(("revisions.update", vec![("id", json!(set.id)), ("name", json!(name.clone()))]));
                        }
                        let mark = draft.marks.entry(set.id).or_insert_with(|| set.mark.clone());
                        let field = theme::field(ui, egui::TextEdit::singleline(mark).desired_width(28.0).char_limit(2));
                        self.automation.add(format!("revisions.mark.{}", set.id), "textbox", "Mark", &field);
                        if field.lost_focus() && *mark != set.mark {
                            run.push(("revisions.update", vec![("id", json!(set.id)), ("mark", json!(mark.clone()))]));
                        }
                        let page = storyteller_core::model::SceneMeta { color: set.page_color.clone(), ..Default::default() }.rgb();
                        let (rect, _) = ui.allocate_exact_size(egui::vec2(28.0, 16.0), egui::Sense::hover());
                        let fill = page.map_or(egui::Color32::WHITE, |(r, g, b)| egui::Color32::from_rgb(r, g, b));
                        ui.painter().rect_filled(rect, 3.0, fill);
                        ui.painter().rect_stroke(rect, 3.0, egui::Stroke::new(1.0, ui.visuals().weak_text_color()), egui::StrokeKind::Inside);
                        let marks = self.session.script.revision_marks(set.id);
                        ui.label(marks.to_string());
                        let delete = ui.small_button("Delete");
                        self.automation.add(format!("revisions.delete.{}", set.id), "button", "Delete", &delete);
                        if delete.clicked() {
                            if marks == 0 {
                                run.push(("revisions.delete", vec![("id", json!(set.id))]));
                            } else {
                                draft.confirm_delete = Some(set.id);
                            }
                        }
                        ui.end_row();
                    }
                });
            }
            if let Some(id) = draft.confirm_delete {
                let name = sets.iter().find(|s| s.id == id).map(|s| s.name.clone()).unwrap_or_default();
                let before = sets.iter().position(|s| s.id == id).and_then(|at| at.checked_sub(1)).and_then(|b| sets.get(b)).map(|s| s.name.clone());
                let marks = self.session.script.revision_marks(id);
                ui.add_space(theme::SPACE_2);
                ui.label(match before {
                    Some(before) => format!("“{name}” marks {marks} passages. Delete it and move them to “{before}”?"),
                    None => format!("“{name}” marks {marks} passages. Delete it and take their marks away?"),
                });
                ui.horizontal(|ui| {
                    let yes = ui.button("Delete");
                    self.automation.add("revisions.confirm_delete", "button", "Delete", &yes);
                    let no = ui.button("Keep");
                    self.automation.add("revisions.keep", "button", "Keep", &no);
                    if yes.clicked() {
                        run.push(("revisions.delete", vec![("id", json!(id))]));
                        draft.confirm_delete = None;
                    }
                    if no.clicked() {
                        draft.confirm_delete = None;
                    }
                });
            }
            if let Some(error) = &draft.error {
                ui.colored_label(ui.visuals().error_fg_color, error);
            }
            ui.add_space(theme::SPACE_2);
            let next = storyteller_core::model::revision_color(sets.len()).0;
            let add = ui.button(format!("Add {next} Revision"));
            self.automation.add("revisions.add", "button", "Add", &add);
            if add.clicked() {
                run.push(("revisions.add", Vec::new()));
            }
        });
        for (id, args) in run {
            let target = params(&args.iter().map(|(k, v)| (*k, v.clone())).collect::<Vec<_>>());
            let result = self.registry.run(id, &mut self.session, &target);
            if let Some(draft) = self.revisions.as_mut() {
                draft.error = result.err().map(|e| match e {
                    storyteller_engine::CommandError::Invalid { reason, .. } => sentence(&reason),
                    other => other.to_string(),
                });
                if id == "revisions.delete" {
                    draft.names.clear();
                    draft.marks.clear();
                }
            }
        }
        if !open {
            self.revisions = None;
            self.refocus();
        }
    }

    /// Production > Edit Scene Number…: the caret's scene's number, by hand.
    pub(super) fn scene_number_dialog(&mut self, ctx: &egui::Context) {
        let Some(draft) = &mut self.scene_number else { return };
        let heading = self.session.element(draft.heading).map(|e| e.text.clone()).unwrap_or_default();
        let mut ok = false;
        let mut close = ctx.input(|i| i.key_pressed(egui::Key::Escape));
        egui::Window::new("Edit Scene Number").collapsible(false).resizable(false).anchor(egui::Align2::CENTER_TOP, [0.0, 120.0]).show(ctx, |ui| {
            ui.label(egui::RichText::new(heading.as_str()).strong());
            let number =
                theme::field(ui, egui::TextEdit::singleline(&mut draft.number).hint_text("Such as 12 or 12A; empty for none").desired_width(200.0));
            self.automation.add("dialog.scene_number.number", "textbox", "Number", &number);
            if !draft.focused {
                number.request_focus();
                draft.focused = true;
            }
            ok |= number.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            if let Some(error) = &draft.error {
                ui.colored_label(ui.visuals().error_fg_color, error);
            }
            ui.horizontal(|ui| {
                let save = ui.button("OK");
                self.automation.add("dialog.scene_number.ok", "button", "OK", &save);
                let cancel = ui.button("Cancel");
                self.automation.add("dialog.scene_number.cancel", "button", "Cancel", &cancel);
                ok |= save.clicked();
                close |= cancel.clicked();
            });
        });
        if close {
            self.scene_number = None;
            self.refocus();
            return;
        }
        if !ok {
            return;
        }
        let target = params(&[("index", json!(draft.heading)), ("number", json!(draft.number))]);
        match self.registry.run("scene.set_number", &mut self.session, &target) {
            Ok(_) => {
                self.scene_number = None;
                self.refocus();
                ctx.request_repaint();
            }
            Err(error) => {
                if let Some(draft) = &mut self.scene_number {
                    draft.error = Some(match error {
                        storyteller_engine::CommandError::Invalid { reason, .. } => sentence(&reason),
                        other => other.to_string(),
                    });
                }
            }
        }
    }

    /// Opens Scene Properties for the scene headed at `heading`.
    pub(super) fn open_scene_dialog(&mut self, heading: usize) {
        let meta = self.session.script.elements.get(heading).map(|e| e.scene.clone()).unwrap_or_default();
        // A colour from a file in another shade is shown as the nearest of ours only when it is one.
        let color = meta.rgb().map(|(r, g, b)| format!("#{r:02X}{g:02X}{b:02X}"));
        self.scene_dialog = Some(SceneDraft { heading, title: meta.title, summary: meta.summary, color, focused: false });
    }

    /// Scene Properties: the scene's title, summary and colour, one Undo.
    pub(super) fn scene_properties_dialog(&mut self, ctx: &egui::Context) {
        let Some(draft) = &mut self.scene_dialog else { return };
        let heading = self.session.script.elements.get(draft.heading).map(|e| e.text.clone()).unwrap_or_default();
        let mut ok = false;
        let mut close = ctx.input(|i| i.key_pressed(egui::Key::Escape));
        egui::Window::new("Scene Properties").collapsible(false).resizable(false).anchor(egui::Align2::CENTER_TOP, [0.0, 120.0]).show(ctx, |ui| {
            ui.label(egui::RichText::new(if heading.trim().is_empty() { "(untitled scene)" } else { heading.as_str() }).strong());
            ui.add_space(theme::SPACE_2);
            ui.label("Title");
            let title =
                theme::field(ui, egui::TextEdit::singleline(&mut draft.title).hint_text("A few words for the navigator").desired_width(320.0));
            self.automation.add("dialog.scene.title", "textbox", "Title", &title);
            if !draft.focused {
                title.request_focus();
                draft.focused = true;
            }
            ui.label("Summary");
            let summary = theme::field(
                ui,
                egui::TextEdit::multiline(&mut draft.summary).hint_text("What happens in the scene").desired_rows(3).desired_width(320.0),
            );
            self.automation.add("dialog.scene.summary", "textbox", "Summary", &summary);
            ui.label("Colour");
            ui.horizontal(|ui| {
                let none = ui.selectable_label(draft.color.is_none(), "None");
                self.automation.add("dialog.scene.color.none", "radio", "None", &none);
                if none.clicked() {
                    draft.color = None;
                }
                for colour in &theme::SCENE_COLOURS {
                    let (name, hex) = (colour.name, colour.file);
                    let fill = if ui.visuals().dark_mode { colour.dark } else { colour.light };
                    let chosen = draft.color.as_deref() == Some(hex);
                    let (rect, swatch) = ui.allocate_exact_size(egui::vec2(20.0, 20.0), egui::Sense::click());
                    ui.painter().rect_filled(rect.shrink(2.0), 4.0, fill);
                    if chosen {
                        ui.painter().rect_stroke(rect, 5.0, egui::Stroke::new(2.0, ui.visuals().strong_text_color()), egui::StrokeKind::Inside);
                    }
                    let swatch = swatch.on_hover_text(name);
                    self.automation.add(format!("dialog.scene.color.{}", name.to_lowercase()), "radio", name, &swatch);
                    if swatch.clicked() {
                        draft.color = Some(hex.to_string());
                    }
                }
            });
            ui.add_space(theme::SPACE_2);
            ui.horizontal(|ui| {
                let save = ui.button("OK");
                self.automation.add("dialog.scene.ok", "button", "OK", &save);
                let cancel = ui.button("Cancel");
                self.automation.add("dialog.scene.cancel", "button", "Cancel", &cancel);
                ok |= save.clicked();
                close |= cancel.clicked();
            });
        });
        if ok && let Some(draft) = self.scene_dialog.take() {
            let target = params(&[
                ("index", json!(draft.heading)),
                ("title", json!(draft.title)),
                ("summary", json!(draft.summary)),
                ("color", json!(draft.color.unwrap_or_default())),
            ]);
            if let Err(error) = self.registry.run("scene.set_properties", &mut self.session, &target) {
                self.status = error.to_string();
            }
            self.refocus();
            ctx.request_repaint();
        } else if close {
            self.scene_dialog = None;
            self.refocus();
        }
    }

    pub(super) fn rename_dialog(&mut self, ctx: &egui::Context) {
        let Some(draft) = &mut self.rename else { return };
        let mut go = false;
        let mut close = ctx.input(|i| i.key_pressed(egui::Key::Escape));
        egui::Window::new("Rename Character").collapsible(false).resizable(false).anchor(egui::Align2::CENTER_TOP, [0.0, 120.0]).show(ctx, |ui| {
            egui::Grid::new("storyteller.rename_grid").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
                let label = ui.label("Character");
                let combo =
                    egui::ComboBox::from_id_salt("storyteller.rename_from").selected_text(draft.from.as_str()).width(240.0).show_ui(ui, |ui| {
                        for name in &draft.names {
                            ui.selectable_value(&mut draft.from, name.clone(), name.as_str());
                        }
                    });
                let _ = combo.response.clone().labelled_by(label.id);
                self.automation.add("dialog.rename.from", "combobox", "Character", &combo.response);
                ui.end_row();
                let label = ui.label("New name");
                let to = theme::field(ui, egui::TextEdit::singleline(&mut draft.to).desired_width(240.0)).labelled_by(label.id);
                self.automation.add("dialog.rename.to", "textbox", "New name", &to);
                if !draft.focused {
                    to.request_focus();
                    draft.focused = true;
                }
                if to.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    go = true;
                }
                ui.end_row();
            });
            let everywhere = ui.checkbox(&mut draft.everywhere, "Also where the name is capitalised in action, dialogue and other text");
            self.automation.add("dialog.rename.everywhere", "checkbox", "Also in other text", &everywhere);
            if let Some(error) = &draft.error {
                ui.colored_label(ui.visuals().error_fg_color, error);
            }
            ui.add_space(theme::SPACE_2);
            ui.horizontal(|ui| {
                let label = if draft.merge { "Merge" } else { "Rename" };
                let ok = ui.button(label);
                self.automation.add("dialog.rename.ok", "button", label, &ok);
                let cancel = ui.button("Cancel");
                self.automation.add("dialog.rename.cancel", "button", "Cancel", &cancel);
                go |= ok.clicked();
                close |= cancel.clicked();
            });
        });
        if close {
            self.rename = None;
            self.refocus();
            return;
        }
        if !go {
            return;
        }
        // Renaming onto another character merges the two: ask once, then the button says Merge.
        let new_name = draft.to.trim().to_uppercase();
        if !draft.merge && new_name != draft.from && draft.names.contains(&new_name) {
            draft.merge = true;
            draft.error = Some(format!("{new_name} is already a character. Merge {} into {new_name}?", draft.from));
            return;
        }
        let merge = draft.merge && draft.names.contains(&new_name);
        let request =
            params(&[("from", json!(draft.from)), ("to", json!(draft.to)), ("everywhere", json!(draft.everywhere)), ("merge", json!(merge))]);
        match self.registry.run("character.rename", &mut self.session, &request) {
            Ok(outcome) => {
                self.rename = None;
                self.status = outcome.message.unwrap_or_default();
                self.refocus();
            }
            Err(error) => {
                if let Some(draft) = &mut self.rename {
                    draft.error = Some(match error {
                        storyteller_engine::CommandError::Invalid { reason, .. } => sentence(&reason),
                        other => other.to_string(),
                    });
                }
            }
        }
    }

    /// File > New TV Script: one-hour, half-hour or multi-camera, the last choice first.
    pub(super) fn new_tv_dialog(&mut self, ctx: &egui::Context) {
        use storyteller_core::model::TvShape;
        let Some(mut shape) = self.new_tv else { return };
        let mut close = ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
        let mut create = false;
        egui::Window::new("New TV Script").collapsible(false).resizable(false).anchor(egui::Align2::CENTER_TOP, [0.0, 120.0]).show(ctx, |ui| {
            for (option, label, acts) in [
                (TvShape::OneHour, "One-hour", "Teaser, Acts One to Five, Tag"),
                (TvShape::HalfHour, "Half-hour", "Cold Open, Acts One to Three, Tag"),
                (TvShape::MultiCam, "Multi-camera half-hour", "Cold Open, Acts One and Two, Tag; lettered scenes, double-spaced dialogue"),
            ] {
                let choice = ui.radio_value(&mut shape, option, format!("{label}: {acts}"));
                self.automation.add(format!("dialog.new_tv.{}", option.id()), "radio", label, &choice);
                if choice.double_clicked() {
                    create = true;
                }
            }
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                let ok = ui.button("Create");
                self.automation.add("dialog.new_tv.create", "button", "Create", &ok);
                create |= ok.clicked() || ui.input(|i| i.key_pressed(egui::Key::Enter));
                let cancel = ui.button("Cancel");
                self.automation.add("dialog.new_tv.cancel", "button", "Cancel", &cancel);
                close |= cancel.clicked();
            });
        });
        self.new_tv = Some(shape);
        if create {
            self.new_tv = None;
            self.prefs.update(|p| p.general.tv_shape = shape);
            self.guard_unsaved(ctx, Pending::NewTv(shape));
        } else if close {
            self.new_tv = None;
            self.refocus();
        }
    }

    pub(super) fn go_to_dialog(&mut self, ctx: &egui::Context) {
        let Some(draft) = &mut self.go_to else { return };
        let mut go = false;
        let mut close = ctx.input(|i| i.key_pressed(egui::Key::Escape));
        if std::mem::take(&mut draft.refocus) {
            draft.focused = false;
        }
        egui::Window::new("Go To").collapsible(false).resizable(false).anchor(egui::Align2::CENTER_TOP, [0.0, 120.0]).show(ctx, |ui| {
            ui.horizontal(|ui| {
                let scene = ui.selectable_value(&mut draft.mode, GoToMode::Scene, "Scene");
                self.automation.add("dialog.go_to.scene", "radio", "Scene", &scene);
                let page = ui.selectable_value(&mut draft.mode, GoToMode::Page, "Page");
                self.automation.add("dialog.go_to.page", "radio", "Page", &page);
                let act = ui.selectable_value(&mut draft.mode, GoToMode::Act, "Act");
                self.automation.add("dialog.go_to.act", "radio", "Act", &act);
                let bookmark = ui.selectable_value(&mut draft.mode, GoToMode::Bookmark, "Bookmark");
                self.automation.add("dialog.go_to.bookmark", "radio", "Bookmark", &bookmark);
                if scene.clicked() || page.clicked() || act.clicked() || bookmark.clicked() {
                    draft.refocus = true;
                    ui.ctx().request_repaint();
                }
            });
            let hint = match draft.mode {
                GoToMode::Scene => "Scene number, such as 12 or 12A",
                GoToMode::Page => "Page number",
                GoToMode::Act => "Act, such as 2, TAG or ACT TWO",
                GoToMode::Bookmark => "Bookmark name",
            };
            let response = theme::field(ui, egui::TextEdit::singleline(&mut draft.text).hint_text(hint).desired_width(240.0));
            self.automation.add("dialog.go_to.text", "textbox", "Go to", &response);
            if !draft.focused {
                response.request_focus();
                draft.focused = true;
            }
            if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                go = true;
            }
            if let Some(error) = &draft.error {
                ui.colored_label(ui.visuals().error_fg_color, error);
            }
            if draft.mode == GoToMode::Bookmark {
                ui.add_space(theme::SPACE_1);
                let names: Vec<&str> = self.session.script.elements.iter().filter_map(|e| e.bookmark.as_deref()).collect();
                if names.is_empty() {
                    ui.label(egui::RichText::new(crate::messages::NO_BOOKMARKS).weak());
                }
                egui::ScrollArea::vertical().max_height(160.0).show(ui, |ui| {
                    for (n, name) in names.iter().enumerate() {
                        let row = ui.selectable_label(false, *name);
                        self.automation.add(format!("dialog.go_to.bookmark.{n}"), "option", name.to_string(), &row);
                        if row.clicked() {
                            draft.text = name.to_string();
                            go = true;
                        }
                    }
                });
            }
            ui.add_space(theme::SPACE_2);
            ui.horizontal(|ui| {
                let ok = ui.button("Go");
                self.automation.add("dialog.go_to.ok", "button", "Go", &ok);
                let cancel = ui.button("Cancel");
                self.automation.add("dialog.go_to.cancel", "button", "Cancel", &cancel);
                go |= ok.clicked();
                close |= cancel.clicked();
            });
        });
        if close {
            self.go_to = None;
            self.refocus();
            return;
        }
        if !go {
            return;
        }
        let text = draft.text.trim().to_string();
        let target = match draft.mode {
            GoToMode::Page => match text.parse::<u64>() {
                Ok(page) => params(&[("page", json!(page))]),
                Err(_) => {
                    draft.error = Some("Type a page number.".into());
                    return;
                }
            },
            GoToMode::Scene => params(&[("scene", json!(text))]),
            GoToMode::Act => params(&[("act", json!(text))]),
            GoToMode::Bookmark => params(&[("bookmark", json!(text))]),
        };
        match self.registry.run("nav.go_to", &mut self.session, &target) {
            Ok(_) => {
                self.go_to = None;
                self.editor.pending_focus = Some((self.session.caret.element, 0));
                // Draw again now: this frame's navigator still shows the scene left.
                ctx.request_repaint();
            }
            Err(error) => {
                if let Some(draft) = &mut self.go_to {
                    draft.error = Some(match error {
                        storyteller_engine::CommandError::Invalid { reason, .. } => sentence(&reason),
                        other => other.to_string(),
                    });
                }
            }
        }
    }

    pub(super) fn lists_window(&mut self, ctx: &egui::Context) {
        let Some(draft) = &mut self.lists else { return };
        const TABS: [(&str, &str); 6] = [
            ("characters", "Characters"),
            ("extensions", "Extensions"),
            ("intros", "Scene Intros"),
            ("locations", "Locations"),
            ("times", "Times"),
            ("transitions", "Transitions"),
        ];
        let mut open = true;
        let mut request = None;
        egui::Window::new("Autocomplete Lists").open(&mut open).collapsible(false).default_width(420.0).show(ctx, |ui| {
            ui.horizontal(|ui| {
                for (n, (id, label)) in TABS.iter().enumerate() {
                    let tab = ui.selectable_value(&mut draft.tab, n, *label);
                    self.automation.add(format!("dialog.lists.{id}"), "radio", *label, &tab);
                }
            });
            ui.add_space(theme::SPACE_1);
            ui.label(egui::RichText::new("Autocomplete also offers everything the script uses, listed here or not.").weak());
            ui.add_space(theme::SPACE_1);
            let (name, _) = TABS.get(draft.tab).copied().unwrap_or(TABS[0]);
            let entries = self.session.script.lists.list(name).cloned().unwrap_or_default();
            egui::ScrollArea::vertical().max_height(320.0).auto_shrink([false, true]).show(ui, |ui| {
                if entries.is_empty() {
                    ui.label(egui::RichText::new("Nothing here yet.").weak());
                }
                for (n, entry) in entries.iter().enumerate() {
                    ui.horizontal(|ui| {
                        let remove = ui.small_button("Remove");
                        self.automation.add(format!("dialog.lists.remove.{n}"), "button", format!("Remove {entry}"), &remove);
                        if remove.clicked() {
                            request = Some(("lists.remove", params(&[("list", json!(name)), ("entry", json!(entry))])));
                        }
                        ui.label(entry);
                    });
                }
            });
            ui.add_space(theme::SPACE_2);
            ui.horizontal(|ui| {
                let field = theme::field(ui, egui::TextEdit::singleline(&mut draft.entry).hint_text("New entry").desired_width(240.0));
                self.automation.add("dialog.lists.entry", "textbox", "New entry", &field);
                let enter = field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                let add = ui.button("Add");
                self.automation.add("dialog.lists.add", "button", "Add", &add);
                if (add.clicked() || enter) && !draft.entry.trim().is_empty() {
                    request = Some(("lists.add", params(&[("list", json!(name)), ("entry", json!(draft.entry.trim()))])));
                    draft.entry.clear();
                    field.request_focus();
                }
                let rebuild = ui.button("Rebuild from Script").on_hover_text("Replace all six lists with what the script uses now");
                self.automation.add("dialog.lists.rebuild", "button", "Rebuild from Script", &rebuild);
                if rebuild.clicked() {
                    request = Some(("lists.rebuild", Map::new()));
                }
            });
        });
        if let Some((id, request)) = request {
            self.run(id, request);
        }
        if !open {
            self.lists = None;
            self.refocus();
        }
    }

    pub(super) fn bookmark_dialog(&mut self, ctx: &egui::Context) {
        let Some(draft) = &mut self.bookmark else { return };
        let mut add = false;
        let mut close = ctx.input(|i| i.key_pressed(egui::Key::Escape));
        egui::Window::new("Add Bookmark").collapsible(false).resizable(false).anchor(egui::Align2::CENTER_TOP, [0.0, 120.0]).show(ctx, |ui| {
            let response = theme::field(ui, egui::TextEdit::singleline(&mut draft.name).hint_text("Name").desired_width(240.0));
            self.automation.add("dialog.bookmark.name", "textbox", "Bookmark name", &response);
            if !draft.focused {
                response.request_focus();
                draft.focused = true;
            }
            add |= response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            if let Some(error) = &draft.error {
                ui.colored_label(ui.visuals().error_fg_color, error);
            }
            ui.add_space(theme::SPACE_2);
            ui.horizontal(|ui| {
                let ok = ui.button("Add");
                self.automation.add("dialog.bookmark.ok", "button", "Add", &ok);
                let cancel = ui.button("Cancel");
                self.automation.add("dialog.bookmark.cancel", "button", "Cancel", &cancel);
                add |= ok.clicked();
                close |= cancel.clicked();
            });
        });
        if close {
            self.bookmark = None;
            self.refocus();
            return;
        }
        if !add {
            return;
        }
        let request = params(&[("index", json!(self.session.caret.element)), ("name", json!(draft.name))]);
        match self.registry.run("bookmark.add", &mut self.session, &request) {
            Ok(_) => {
                self.bookmark = None;
                self.status = "Added the bookmark".to_string();
                self.refocus();
            }
            Err(error) => {
                if let Some(draft) = &mut self.bookmark {
                    // Back in the field, ready to type the name.
                    draft.focused = false;
                    draft.error = Some(match error {
                        storyteller_engine::CommandError::Invalid { reason, .. } => sentence(&reason),
                        other => other.to_string(),
                    });
                }
            }
        }
    }

    pub(super) fn help_windows(&mut self, ctx: &egui::Context) {
        let rows = if self.help.shortcuts { self.shortcut_rows(ctx) } else { Vec::new() };
        match self.help.show(ctx, &mut self.automation, &rows) {
            Some(HelpAction::Open(url)) => self.open_link(url, "Opened the website in your browser."),
            Some(HelpAction::Licenses) => self.queued.push("help.licenses".into()),
            None => {}
        }
    }

    pub(super) fn unsaved_prompt(&mut self, ctx: &egui::Context) {
        if self.prompt.is_none() {
            return;
        }
        let mut answer = None;
        egui::Modal::new(egui::Id::unique("storyteller.unsaved")).frame(theme::modal_frame(&ctx.global_style())).show(ctx, |ui| {
            ui.heading("Save changes?");
            ui.label(format!("“{}” has changes that are not saved.", self.display_title().trim_end_matches(" — edited")));
            ui.add_space(theme::SPACE_2);
            ui.horizontal(|ui| {
                for (id, label) in [("save", "Save"), ("discard", "Don't Save"), ("cancel", "Cancel")] {
                    let response = ui.button(label);
                    self.automation.add(format!("prompt.{id}"), "button", label, &response);
                    if response.clicked() {
                        answer = Some(id);
                    }
                }
            });
        });
        match answer {
            Some("save") => {
                if self.save(false)
                    && let Some(action) = self.prompt.take()
                {
                    self.perform(ctx, action);
                }
            }
            Some("discard") => {
                if let Some(action) = self.prompt.take() {
                    self.forget_recovery();
                    self.session.dirty = false;
                    self.perform(ctx, action);
                }
            }
            Some(_) => self.prompt = None,
            None => {}
        }
    }
}
