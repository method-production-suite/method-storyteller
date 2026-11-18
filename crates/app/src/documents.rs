//! Opening, saving, exporting and recovery copies.

use super::*;

impl StorytellerApp {
    pub(super) fn open_path(&mut self, path: &Path) {
        let mut params = Map::new();
        params.insert("path".into(), json!(path.display().to_string()));
        if let Err(error) = self.try_run("file.open", params) {
            self.error = Some(crate::messages::open_failed(path, error.file_problem(), &error.to_string()));
        } else {
            let shown = path.display().to_string();
            self.recent.push(&shown);
        }
        self.after_open();
    }

    /// The view settings a script is shown with, from the app's settings.
    pub(super) fn view_into_session(&mut self) {
        let view = &self.prefs.get().view;
        self.session.layout.show_scene_numbers = view.scene_numbers;
        self.session.show_revisions = view.revisions;
        self.session.page_colours = view.page_colours;
        self.session.show_outline = view.outline;
        self.session.tab_opens_extensions = self.prefs.get().general.tab_opens_extensions;
    }

    pub(super) fn after_open(&mut self) {
        self.recovery_name = self.fresh_recovery_name();
        self.view_into_session();
        self.session.invalidate_layout();
        self.editor = EditorState::default();
        self.editor.pending_focus = Some((0, 0));
        self.discard_confirmed = false;
        self.sample = false;
        // The sample's line goes with the sample (open_sample sets it again after this).
        if self.status == sample::SAMPLE_LINE {
            self.status.clear();
        }
    }

    /// Save, asking for a path when there is none (or `as_new`). Returns whether it saved.
    pub(super) fn save(&mut self, as_new: bool) -> bool {
        let old_stem = Some(self.recovery_name.clone());
        let path = match (&self.session.path, as_new) {
            (Some(path), false) => Some(path.clone()),
            _ => {
                let preferred = self.preferred_extension();
                let name = self.session.path.as_ref().and_then(|p| p.file_name()).map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| {
                    let stem = if self.showing_sample() { sample::SAMPLE_NAME } else { "Untitled" };
                    format!("{stem}.{preferred}")
                });
                // The working format the writer chose comes first; the others stay one click away.
                let mut formats = vec![("Method Storyteller Package", "mstory"), ("Open Screenplay Format", "osf"), ("Fountain", "fountain")];
                formats.sort_by_key(|(_, ext)| *ext != preferred);
                formats
                    .into_iter()
                    .fold(rfd::FileDialog::new(), |dialog, (name, ext)| dialog.add_filter(name, &[ext]))
                    .add_filter("Open Screenplay Format, in a ZIP archive", &["fadein"])
                    .add_filter("FDX", &["fdx"])
                    .set_file_name(name)
                    .save_file()
            }
        };
        let Some(mut path) = path else { return false };
        if path.extension().is_none() {
            path.set_extension(self.preferred_extension());
        }
        let mut params = Map::new();
        params.insert("path".into(), json!(path.display().to_string()));
        if let Err(error) = self.try_run("file.save_as", params) {
            self.error = Some(crate::messages::save_failed(&path, error.file_problem(), &error.to_string()));
        }
        let saved = self.error.is_none() && !self.session.dirty;
        if saved {
            // Saved under its (possibly new) name: an earlier copy under the old name goes too.
            if let (Some(dir), Some(old)) = (&self.recovery_dir, &old_stem) {
                crate::recovery::forget(dir, old);
            }
            self.recovery_name = self.fresh_recovery_name();
            let shown = path.display().to_string();
            self.recent.push(&shown);
        }
        saved
    }

    /// A file opened from the Finder while the app runs, after asking about unsaved changes.
    /// Waits while a question is already on screen.
    pub(super) fn open_from_finder(&mut self, ctx: &egui::Context) {
        if self.prompt.is_some() {
            return;
        }
        if let Some(path) = self.first_from_finder() {
            self.guard_unsaved(ctx, Pending::Open(Some(path)));
        }
    }

    /// The first of the files opened from the Finder. The window holds one script, so any
    /// others are named in the status line rather than dropped silently.
    pub(super) fn first_from_finder(&mut self) -> Option<PathBuf> {
        let mut paths = storyteller_platform::take().into_iter();
        let first = paths.next()?;
        let others: Vec<String> =
            paths.map(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| p.display().to_string())).collect();
        if !others.is_empty() {
            self.status = format!("One script at a time: opened the first. Not opened: {}.", others.join(", "));
        }
        Some(first)
    }

    /// The extension new scripts are saved with: the writer's choice of working format.
    pub(super) fn preferred_extension(&self) -> &'static str {
        match self.prefs.get().general.new_script_format.as_str() {
            "osf" => "osf",
            "fountain" => "fountain",
            _ => "mstory",
        }
    }

    /// Asks where to save an export with this extension, named after the script.
    pub(super) fn export_path(&self, extension: &str) -> Option<PathBuf> {
        self.export_path_as("", extension)
    }

    /// Asks where to save an export named after the script and then `suffix`, such as
    /// `Field Station Scenes.csv`.
    pub(super) fn export_path_as(&self, suffix: &str, extension: &str) -> Option<PathBuf> {
        let stem =
            self.session.path.as_ref().and_then(|p| p.file_stem()).map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "Untitled".into());
        let name = if suffix.is_empty() { format!("{stem}.{extension}") } else { format!("{stem} {suffix}.{extension}") };
        rfd::FileDialog::new().add_filter(extension, &[extension]).set_file_name(name).save_file()
    }

    /// File > Export Plain Text…
    pub(super) fn export_text(&mut self) {
        if let Some(path) = self.export_path("txt") {
            let mut params = Map::new();
            params.insert("path".into(), json!(path.display().to_string()));
            self.run("file.export_text", params);
        }
    }

    /// File > Export for Method Schedule…
    pub(super) fn export_scenes(&mut self) {
        if let Some(path) = self.export_path(storyteller_engine::scenes_file::EXTENSION) {
            let mut params = Map::new();
            params.insert("path".into(), json!(path.display().to_string()));
            self.run("file.export_scenes", params);
        }
    }

    pub(super) fn export(&mut self, format: FileFormat) {
        if let Some(path) = self.export_path(format.extension()) {
            let id = match format {
                FileFormat::Pdf => "file.export_pdf",
                FileFormat::Fdx => "file.export_fdx",
                FileFormat::Fountain => "file.export_fountain",
                FileFormat::Osf | FileFormat::FadeIn | FileFormat::Mstory => "file.export_osf",
            };
            let mut params = Map::new();
            params.insert("path".into(), json!(path.display().to_string()));
            self.run(id, params);
        }
    }

    pub(super) fn autosave(&mut self) {
        if !self.session.dirty || self.last_autosave.elapsed() < AUTOSAVE_EVERY {
            return;
        }
        self.last_autosave = Instant::now();
        let Some(dir) = self.recovery_dir.clone() else { return };
        if let Ok(bytes) = files::encode(&self.session.script, FileFormat::Fountain, &self.session.layout) {
            let _ = crate::recovery::write(&dir, &self.recovery_name, &bytes);
        }
    }

    /// Looks for a recovery copy to offer; whether there is one.
    pub(super) fn find_recovery(&mut self) -> bool {
        self.recovered = self.recovery_dir.as_deref().and_then(|dir| crate::recovery::waiting(dir).into_iter().next());
        self.recovered.is_some()
    }

    /// A recovery name for the open script: its file's name, or Untitled, never one a copy
    /// still waiting to be answered has.
    pub(super) fn fresh_recovery_name(&self) -> String {
        let stem =
            self.session.path.as_ref().and_then(|p| p.file_stem()).map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "Untitled".into());
        match &self.recovery_dir {
            Some(dir) => crate::recovery::free_name(dir, &stem),
            None => stem,
        }
    }

    /// The open script's changes are deliberately thrown away: its copy goes.
    pub(super) fn forget_recovery(&self) {
        if let Some(dir) = &self.recovery_dir {
            crate::recovery::forget(dir, &self.recovery_name);
        }
    }

    /// At launch, a recovery copy left by a launch that ended without saving: open it as an
    /// unsaved script, throw it away, or leave it for later.
    pub(super) fn recovery_prompt(&mut self, ctx: &egui::Context) {
        let Some(copy) = self.recovered.clone() else { return };
        if self.prompt.is_some() {
            return;
        }
        let name = crate::recovery::name_of(&copy);
        let mut answer = None;
        egui::Modal::new(egui::Id::unique("storyteller.recovery")).frame(theme::modal_frame(&ctx.global_style())).show(ctx, |ui| {
            ui.heading("Recover unsaved changes?");
            ui.label(format!("Method Storyteller closed without saving the changes to “{name}”. A copy of them was kept."));
            ui.add_space(theme::SPACE_2);
            ui.horizontal(|ui| {
                for (id, label) in [("open", "Open the Copy"), ("discard", "Throw It Away"), ("later", "Later")] {
                    let response = ui.button(label);
                    self.automation.add(format!("recovery.{id}"), "button", label, &response);
                    if response.clicked() {
                        answer = Some(id);
                    }
                }
            });
        });
        match answer {
            Some("open") => {
                self.recovered = None;
                match files::open(&copy) {
                    Ok((script, _)) => {
                        self.session = Session::new(script);
                        self.session.dirty = true;
                        self.after_open();
                        // The copy stays, and this script's copies go on under its name, until
                        // the work is saved or thrown away: nothing is unprotected meanwhile.
                        self.recovery_name = name.clone();
                        self.status = format!("Recovered “{name}”: save it to keep it (File > Save As…).");
                    }
                    Err(error) => self.error = Some(crate::messages::open_failed(&copy, Some(error.problem()), &error.to_string())),
                }
            }
            Some("discard") => {
                self.recovered = None;
                let _ = std::fs::remove_file(&copy);
            }
            Some(_) => self.recovered = None,
            None => {}
        }
    }
}
