//! The desktop app: menus, scene navigator, script view, status bar, dialogs and the control
//! channel host.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender};
use std::time::{Duration, Instant};

use craft_settings::{Location, SettingsStore};
use egui::{Color32, Event, Key, KeyboardShortcut, Modifiers, ViewportCommand};
use serde_json::{Map, Value, json};
use storyteller_core::ElementKind;
use storyteller_engine::control::{ControlRequest, UiHost, handle_line};
use storyteller_engine::files::{self, FileFormat};
use storyteller_engine::flow::SHORTCUT_KINDS;
use storyteller_engine::{APP_NAME, Caret, Registry, Session};

use crate::automation::Automation;
use crate::editor::{self, EditAction, EditorState};
use crate::help::{self, HelpAction, HelpWindows, ShortcutRow};
use crate::prefs::{APP_ID, Prefs, RECOVERY_DIR};
use crate::theme;

const AUTOSAVE_EVERY: Duration = Duration::from_secs(60);
const MENUS: [&str; 6] = ["File", "Edit", "Format", "Production", "View", "Help"];

/// Commands that belong to the app rather than the document.
struct AppCommand {
    id: &'static str,
    label: &'static str,
    menu: &'static str,
    shortcut: Option<&'static str>,
}

/// ⌘⌥G on macOS; Ctrl+Shift+G elsewhere, where Ctrl+Alt is AltGr.
const GO_TO_KEY: &str = if cfg!(target_os = "macos") { "Cmd+Alt+G" } else { "Cmd+Shift+G" };

/// Find Next and Previous: ⌘G and ⇧⌘G on macOS; F3 and Shift+F3 elsewhere.
const FIND_NEXT_KEY: &str = if cfg!(target_os = "macos") { "Cmd+G" } else { "F3" };
const FIND_PREVIOUS_KEY: &str = if cfg!(target_os = "macos") { "Cmd+Shift+G" } else { "Shift+F3" };

const APP_COMMANDS: [AppCommand; 28] = [
    AppCommand { id: "app.title_page", label: "Title Page…", menu: "File/Title Page…", shortcut: Some("Cmd+Shift+T") },
    AppCommand { id: "prefs.new_fountain", label: "Save New Scripts as Fountain", menu: "File/Save New Scripts as Fountain", shortcut: None },
    AppCommand {
        id: "prefs.new_osf",
        label: "Save New Scripts as Open Screenplay Format",
        menu: "File/Save New Scripts as Open Screenplay Format",
        shortcut: None,
    },
    AppCommand { id: "app.quit", label: "Quit", menu: "File/Quit", shortcut: Some("Cmd+Q") },
    // No shortcuts here on purpose: ⌘X, ⌘C, ⌘V and ⌘A stay with the text fields, which handle
    // them; the menu items do the same for a writer who reaches for the menu.
    AppCommand { id: "app.cut", label: "Cut", menu: "Edit/Cut", shortcut: None },
    AppCommand { id: "app.copy", label: "Copy", menu: "Edit/Copy", shortcut: None },
    AppCommand { id: "app.paste", label: "Paste", menu: "Edit/Paste", shortcut: None },
    AppCommand { id: "app.select_all", label: "Select All", menu: "Edit/Select All", shortcut: None },
    AppCommand { id: "app.find", label: "Find…", menu: "Edit/Find…", shortcut: Some("Cmd+F") },
    AppCommand { id: "app.find_next", label: "Find Next", menu: "Edit/Find Next", shortcut: Some(FIND_NEXT_KEY) },
    AppCommand { id: "app.find_previous", label: "Find Previous", menu: "Edit/Find Previous", shortcut: Some(FIND_PREVIOUS_KEY) },
    AppCommand { id: "app.rename_character", label: "Rename Character…", menu: "Production/Rename Character…", shortcut: None },
    AppCommand { id: "app.go_to", label: "Go To…", menu: "Edit/Go To…", shortcut: Some(GO_TO_KEY) },
    AppCommand { id: "prefs.smart_quotes", label: "Smart Quotes", menu: "Format/Smart Quotes", shortcut: None },
    AppCommand { id: "view.navigator", label: "Scene Navigator", menu: "View/Scene Navigator", shortcut: Some("Cmd+Shift+N") },
    AppCommand { id: "view.page_view", label: "Page View", menu: "View/Page View", shortcut: Some("Cmd+Shift+P") },
    AppCommand { id: "view.zoom_in", label: "Zoom In", menu: "View/Zoom In", shortcut: Some("Cmd+Equals") },
    AppCommand { id: "view.zoom_out", label: "Zoom Out", menu: "View/Zoom Out", shortcut: Some("Cmd+Minus") },
    AppCommand { id: "view.zoom_reset", label: "Actual Size", menu: "View/Actual Size", shortcut: Some("Cmd+0") },
    AppCommand { id: "help.about", label: "About Method Storyteller", menu: "Help/About Method Storyteller", shortcut: None },
    AppCommand { id: "help.user_guide", label: "User Guide", menu: "Help/User Guide", shortcut: None },
    AppCommand { id: "help.shortcuts", label: "Keyboard Shortcuts", menu: "Help/Keyboard Shortcuts", shortcut: None },
    AppCommand { id: "help.whats_new", label: "What's New", menu: "Help/What's New", shortcut: None },
    AppCommand { id: "help.check_updates", label: "Check for Updates…", menu: "Help/Check for Updates…", shortcut: None },
    AppCommand {
        id: "prefs.check_updates_automatically",
        label: "Check for Updates Automatically",
        menu: "Help/Check for Updates Automatically",
        shortcut: None,
    },
    AppCommand { id: "help.report_bug", label: "Report a Bug", menu: "Help/Report a Bug", shortcut: None },
    AppCommand { id: "help.licenses", label: "Licenses and Notices", menu: "Help/Licenses and Notices", shortcut: None },
    AppCommand { id: "help.website", label: "methodproduction.com", menu: "Help/methodproduction.com", shortcut: None },
];

/// Commands handled by the flow keys inside the editor, not as global shortcuts.
const EDITOR_KEYS: [&str; 3] = ["Enter", "Tab", "Shift+Tab"];

fn parse_shortcut(text: &str) -> Option<KeyboardShortcut> {
    let mut modifiers = Modifiers::NONE;
    let mut key = None;
    for part in text.split('+') {
        match part {
            "Cmd" => modifiers = modifiers.plus(Modifiers::COMMAND),
            "Shift" => modifiers = modifiers.plus(Modifiers::SHIFT),
            "Alt" => modifiers = modifiers.plus(Modifiers::ALT),
            name => key = Key::from_name(name),
        }
    }
    Some(KeyboardShortcut::new(modifiers, key?))
}

enum Pending {
    Close,
    New,
    Open(Option<PathBuf>),
}

struct TitleDraft {
    fields: Vec<(String, String)>,
}

/// The Rename Character dialog.
#[derive(Default)]
struct RenameDraft {
    names: Vec<String>,
    from: String,
    to: String,
    everywhere: bool,
    error: Option<String>,
    /// The new name belongs to another character and the writer has been asked about merging.
    merge: bool,
    focused: bool,
}

/// The Go To dialog: a scene number or a page.
#[derive(Default)]
struct GoToDraft {
    text: String,
    page: bool,
    error: Option<String>,
    focused: bool,
}

pub struct StorytellerApp {
    session: Session,
    registry: Registry,
    prefs: SettingsStore<Prefs>,
    editor: EditorState,
    automation: Automation,
    control: Option<Receiver<ControlRequest>>,
    pending_screenshot: Option<(Value, Sender<String>, PathBuf)>,
    queued: Vec<String>,
    prompt: Option<Pending>,
    discard_confirmed: bool,
    title_dialog: Option<TitleDraft>,
    go_to: Option<GoToDraft>,
    rename: Option<RenameDraft>,
    find: Option<find_bar::FindBar>,
    help: HelpWindows,
    status: String,
    error: Option<String>,
    last_autosave: Instant,
    window_title: String,
    shortcuts: Vec<(KeyboardShortcut, &'static str)>,
    /// The macOS menu bar; `None` elsewhere, or if it could not be installed (the window menu shows).
    #[cfg(target_os = "macos")]
    native: Option<crate::native_menu::NativeMenu>,
    updates: crate::updates::Updates,
    /// The last update line put in the status, to put it there only when it changes.
    update_line: Option<String>,
    /// Input for the next frame: Edit > Cut and Copy, after the text field has its focus back.
    next_frame_input: Vec<Event>,
    /// Drawn for the offscreen pictures: the window as a Mac shows it, menus in the menu bar.
    #[cfg(test)]
    pictures: bool,
}

/// One menu entry: command id, label, shortcut text and whether it can run now.
struct MenuEntry {
    id: String,
    label: String,
    shortcut: Option<&'static str>,
    enabled: bool,
}

impl StorytellerApp {
    pub fn new(cc: &eframe::CreationContext<'_>, file: Option<PathBuf>, control_port: Option<u16>) -> Self {
        let mut app = Self::without_menu(cc, file, control_port, Location::Auto);
        app.install_menu(&cc.egui_ctx);
        app
    }

    /// The app without the macOS menu bar, which offscreen tests and pictures leave out.
    /// Tests pass [`Location::Memory`] so they never touch real settings.
    fn without_menu(cc: &eframe::CreationContext<'_>, file: Option<PathBuf>, control_port: Option<u16>, settings: Location) -> Self {
        editor::install_fonts(&cc.egui_ctx);
        theme::apply(&cc.egui_ctx);
        // Settings in memory (tests) keep the updater's switch in memory too.
        let in_memory = matches!(settings, Location::Memory);
        let (prefs, report) = SettingsStore::<Prefs>::open(settings);
        let prefs_automatic = prefs.get().updates.check_automatically;
        let registry = Registry::new();
        let mut shortcuts = Vec::new();
        for command in registry.iter() {
            if let Some(text) = command.shortcut.filter(|s| !EDITOR_KEYS.contains(s))
                && let Some(shortcut) = parse_shortcut(text)
            {
                shortcuts.push((shortcut, command.id));
            }
        }
        for command in &APP_COMMANDS {
            if let Some(shortcut) = command.shortcut.and_then(parse_shortcut) {
                shortcuts.push((shortcut, command.id));
            }
        }
        // Longer modifier sets first, so Cmd+Shift+S wins over Cmd+S.
        shortcuts.sort_by_key(|(s, _)| std::cmp::Reverse(u8::from(s.modifiers.shift) + u8::from(s.modifiers.alt)));

        let mut app = StorytellerApp {
            session: Session::default(),
            registry,
            prefs,
            editor: EditorState::default(),
            automation: Automation::default(),
            control: None,
            pending_screenshot: None,
            queued: Vec::new(),
            prompt: None,
            discard_confirmed: false,
            title_dialog: None,
            go_to: None,
            rename: None,
            find: None,
            help: HelpWindows::default(),
            status: String::new(),
            error: report.has_problems().then(|| "Settings could not be fully loaded; defaults are in use.".to_string()),
            last_autosave: Instant::now(),
            window_title: String::new(),
            shortcuts,
            #[cfg(target_os = "macos")]
            native: None,
            updates: crate::updates::Updates::start(
                &cc.egui_ctx,
                if in_memory { None } else { craft_settings::paths::data_dir(crate::prefs::APP_ID).map(|d| d.join("updates.json")) },
                prefs_automatic,
            ),
            update_line: None,
            next_frame_input: Vec::new(),
            #[cfg(test)]
            pictures: false,
        };
        let last_seen = app.prefs.get().general.last_seen_version.clone();
        app.help.whats_new = help::whats_new_due(&last_seen, help::VERSION);
        if help::record_version(&last_seen, help::VERSION) {
            app.prefs.update(|p| p.general.last_seen_version = help::VERSION.to_string());
        }
        app.session.layout.show_scene_numbers = app.prefs.get().view.scene_numbers;
        if let Some(path) = file {
            app.open_path(&path);
        }
        if let Some(port) = control_port {
            let ctx = cc.egui_ctx.clone();
            match storyteller_engine::control::serve(port, move || ctx.request_repaint()) {
                Ok(receiver) => {
                    app.control = Some(receiver);
                    app.status = format!("Control channel on 127.0.0.1:{port}");
                }
                Err(error) => app.error = Some(format!("Control channel unavailable on port {port}: {error}")),
            }
        }
        app.editor.pending_focus = Some((0, 0));
        app
    }

    // -----------------------------------------------------------------------------------------
    // Commands

    fn run(&mut self, id: &str, params: Map<String, Value>) {
        match self.registry.run(id, &mut self.session, &params) {
            Ok(outcome) => {
                if let Some(message) = outcome.message {
                    self.status = message;
                }
                self.error = None;
            }
            Err(error) => self.error = Some(error.to_string()),
        }
    }

    fn run_command(&mut self, ctx: &egui::Context, id: &str) {
        let zoom = self.prefs.get().zoom();
        match id {
            "file.new" => self.guard_unsaved(ctx, Pending::New),
            "file.open" => self.guard_unsaved(ctx, Pending::Open(None)),
            "file.save" => {
                self.save(false);
            }
            "file.save_as" => {
                self.save(true);
            }
            "file.export_pdf" => self.export(FileFormat::Pdf),
            "file.export_fdx" => self.export(FileFormat::Fdx),
            "file.export_fountain" => self.export(FileFormat::Fountain),
            "file.export_osf" => self.export(FileFormat::Osf),
            "app.title_page" => {
                let page = &self.session.script.title_page;
                let fields =
                    storyteller_core::TitlePage::STANDARD_KEYS.iter().map(|k| (k.to_string(), page.get(k).unwrap_or("").to_string())).collect();
                self.title_dialog = Some(TitleDraft { fields });
            }
            "app.go_to" => self.go_to = Some(GoToDraft::default()),
            // The text field the caret is in acts on these as on its own keys. Clicking the menu
            // drawn in the window can take its focus, so it gets the focus back first; its
            // selection is kept with it. Paste asks the window, which reads the clipboard.
            "app.cut" => {
                self.focus_caret_field(ctx);
                self.next_frame_input.push(Event::Cut);
            }
            "app.copy" => {
                self.focus_caret_field(ctx);
                self.next_frame_input.push(Event::Copy);
            }
            "app.paste" => {
                self.focus_caret_field(ctx);
                ctx.send_viewport_cmd(ViewportCommand::RequestPaste);
            }
            "app.select_all" => self.select_all(ctx),
            "app.rename_character" => {
                let names: Vec<String> = storyteller_engine::commands::character_names(&self.session.script).into_iter().map(|(n, _)| n).collect();
                // The cue the caret is in, if any, else the character with the most speeches.
                let here = self.session.current().filter(|e| e.kind == ElementKind::Character).map(|e| e.character_name().to_uppercase());
                let from = here.filter(|n| names.contains(n)).or_else(|| names.first().cloned()).unwrap_or_default();
                if names.is_empty() {
                    self.status = "There are no characters to rename yet.".into();
                } else {
                    self.rename = Some(RenameDraft { names, from, ..RenameDraft::default() });
                }
            }
            "app.find" => match &mut self.find {
                Some(bar) => bar.focus(),
                None => self.find = Some(find_bar::FindBar::open()),
            },
            "app.find_next" | "app.find_previous" => {
                if self.find.is_none() {
                    self.find = Some(find_bar::FindBar::open());
                }
                self.find_step(if id == "app.find_next" { find_bar::Step::Next } else { find_bar::Step::Previous });
            }
            "app.quit" => ctx.send_viewport_cmd(ViewportCommand::Close),
            "view.navigator" => {
                self.prefs.update(|p| p.view.navigator = !p.view.navigator);
            }
            "view.page_view" => {
                self.prefs.update(|p| p.view.page_view = !p.view.page_view);
            }
            "prefs.smart_quotes" => {
                self.prefs.update(|p| p.general.smart_quotes = !p.general.smart_quotes);
            }
            "prefs.new_fountain" => {
                self.prefs.update(|p| p.general.new_script_format = "fountain".into());
                self.status = "New scripts will be saved as Fountain (.fountain).".into();
            }
            "prefs.new_osf" => {
                self.prefs.update(|p| p.general.new_script_format = "osf".into());
                self.status = "New scripts will be saved as Open Screenplay Format (.osf).".into();
            }
            "view.zoom_in" => {
                self.prefs.update(|p| p.view.zoom = (zoom + 0.1).min(3.0));
            }
            "view.zoom_out" => {
                self.prefs.update(|p| p.view.zoom = (zoom - 0.1).max(0.5));
            }
            "view.zoom_reset" => {
                self.prefs.update(|p| p.view.zoom = 1.0);
            }
            "help.about" => self.help.about = true,
            "help.shortcuts" => self.help.shortcuts = true,
            "help.whats_new" => self.help.whats_new = true,
            "help.licenses" => self.help.licenses = Some(self.help.licenses.unwrap_or(0)),
            "help.user_guide" => self.open_link(help::USER_GUIDE, "Opened the user guide in your browser."),
            "help.website" => self.open_link(help::SUITE_WEBSITE, "Opened methodproduction.com in your browser."),
            "help.report_bug" => {
                let url = help::new_issue_url(help::VERSION, &help::os_label());
                self.open_link(&url, "Opened a new bug report in your browser. Nothing is sent until you submit it.");
            }
            "help.check_updates" => match self.updates.unavailable() {
                Some(why) => self.status = format!("{why}."),
                None => {
                    self.status = "Checking for updates…".into();
                    self.updates.check(ctx);
                }
            },
            "help.restart_to_update" => self.restart_to_update(),
            "prefs.check_updates_automatically" => {
                let on = !self.updates.automatic();
                if let Err(error) = self.updates.set_automatic(on) {
                    self.error = Some(format!("Could not save the update setting: {error}"));
                }
                self.prefs.update(|p| p.updates.check_automatically = on);
            }
            "view.scene_numbers" => {
                self.run(id, Map::new());
                let show = self.session.layout.show_scene_numbers;
                self.prefs.update(|p| p.view.scene_numbers = show);
            }
            _ => {
                self.run(id, Map::new());
                self.refocus();
            }
        }
    }

    fn open_link(&mut self, url: &str, done: &str) {
        match help::open_link(url) {
            Ok(()) => {
                self.status = done.to_string();
                self.error = None;
            }
            Err(error) => self.error = Some(error),
        }
    }

    /// Every command with a shortcut, grouped by menu, for Help > Keyboard Shortcuts.
    fn shortcut_rows(&self, ctx: &egui::Context) -> Vec<ShortcutRow> {
        let commands = self.registry.iter().map(|c| (c.menu, c.label, c.shortcut)).chain(APP_COMMANDS.iter().map(|c| (c.menu, c.label, c.shortcut)));
        let mut rows: Vec<ShortcutRow> = commands
            .filter_map(|(menu, label, shortcut)| {
                let text = shortcut?;
                let keys =
                    parse_shortcut(text).map_or_else(|| text.to_string(), |s| ctx.format_shortcut(&s)).replace("Equals", "=").replace("Minus", "-");
                let group = menu.split_once('/').map_or("Writing", |(group, _)| group);
                Some(ShortcutRow { group: group.to_string(), label: label.trim_end_matches('…').to_string(), keys })
            })
            .collect();
        // Menus in menu-bar order, the writing keys last; within a group, registry order.
        let rank = |group: &str| MENUS.iter().position(|m| *m == group).unwrap_or(MENUS.len());
        rows.sort_by_key(|row| rank(&row.group));
        rows
    }

    /// The keyboard focus back on the text field of the paragraph the caret is in, with the
    /// selection it had when it last held the keyboard.
    fn focus_caret_field(&mut self, ctx: &egui::Context) -> egui::Id {
        self.session.clamp_caret();
        let index = self.session.caret.element;
        let id = egui::Id::new(("storyteller.element", index));
        if let Some((_, range)) = self.editor.selection.filter(|(i, _)| *i == index) {
            let mut state = egui::text_edit::TextEditState::load(ctx, id).unwrap_or_default();
            state.cursor.set_char_range(Some(range));
            state.store(ctx, id);
        }
        ctx.memory_mut(|m| m.request_focus(id));
        id
    }

    /// Edit > Select All: the whole of the paragraph the caret is in, as ⌘A selects it.
    fn select_all(&mut self, ctx: &egui::Context) {
        let id = self.focus_caret_field(ctx);
        let Some(text) = self.session.element(self.session.caret.element).map(|e| e.text.clone()) else { return };
        let mut state = egui::text_edit::TextEditState::load(ctx, id).unwrap_or_default();
        let range = egui::text::CCursorRange::two(egui::text::CCursor::new(0), egui::text::CCursor::new(text.chars().count()));
        state.cursor.set_char_range(Some(range));
        state.store(ctx, id);
    }

    fn refocus(&mut self) {
        self.session.clamp_caret();
        self.editor.pending_focus = Some((self.session.caret.element, self.session.caret.offset));
    }

    fn guard_unsaved(&mut self, ctx: &egui::Context, action: Pending) {
        if self.session.dirty {
            self.prompt = Some(action);
        } else {
            self.perform(ctx, action);
        }
    }

    fn perform(&mut self, ctx: &egui::Context, action: Pending) {
        match action {
            Pending::Close => {
                self.discard_confirmed = true;
                ctx.send_viewport_cmd(ViewportCommand::Close);
            }
            Pending::New => {
                self.run("file.new", Map::new());
                self.after_open();
            }
            Pending::Open(path) => {
                let path = path
                    .or_else(|| rfd::FileDialog::new().add_filter("Screenplay", &["fountain", "spmd", "txt", "osf", "fadein", "fdx"]).pick_file());
                if let Some(path) = path {
                    self.open_path(&path);
                }
            }
        }
    }

    fn open_path(&mut self, path: &Path) {
        let mut params = Map::new();
        params.insert("path".into(), json!(path.display().to_string()));
        self.run("file.open", params);
        if let Some(detail) = self.error.take() {
            self.error = Some(crate::messages::open_failed(path, &detail));
        } else {
            let shown = path.display().to_string();
            self.prefs.update(|p| p.push_recent(&shown));
        }
        self.after_open();
    }

    fn after_open(&mut self) {
        self.session.layout.show_scene_numbers = self.prefs.get().view.scene_numbers;
        self.session.invalidate_layout();
        self.editor = EditorState::default();
        self.editor.pending_focus = Some((0, 0));
        self.discard_confirmed = false;
    }

    /// Save, asking for a path when there is none (or `as_new`). Returns whether it saved.
    fn save(&mut self, as_new: bool) -> bool {
        let path = match (&self.session.path, as_new) {
            (Some(path), false) => Some(path.clone()),
            _ => {
                let preferred = self.preferred_extension();
                let name = self
                    .session
                    .path
                    .as_ref()
                    .and_then(|p| p.file_name())
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| format!("Untitled.{preferred}"));
                // The working format the writer chose comes first; the others stay one click away.
                let fountain = ("Fountain", "fountain");
                let osf = ("Open Screenplay Format", "osf");
                let (first, second) = if preferred == "osf" { (osf, fountain) } else { (fountain, osf) };
                rfd::FileDialog::new()
                    .add_filter(first.0, &[first.1])
                    .add_filter(second.0, &[second.1])
                    .add_filter("Open Screenplay Format in a .fadein archive", &["fadein"])
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
        self.run("file.save_as", params);
        if let Some(detail) = self.error.take() {
            self.error = Some(crate::messages::save_failed(&path, &detail));
        }
        let saved = self.error.is_none() && !self.session.dirty;
        if saved {
            let shown = path.display().to_string();
            self.prefs.update(|p| p.push_recent(&shown));
        }
        saved
    }

    /// The extension new scripts are saved with: the writer's choice of working format.
    fn preferred_extension(&self) -> &'static str {
        if self.prefs.get().general.new_script_format == "osf" { "osf" } else { "fountain" }
    }

    fn export(&mut self, format: FileFormat) {
        let stem =
            self.session.path.as_ref().and_then(|p| p.file_stem()).map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "Untitled".into());
        let picked = rfd::FileDialog::new()
            .add_filter(format.extension(), &[format.extension()])
            .set_file_name(format!("{stem}.{}", format.extension()))
            .save_file();
        if let Some(path) = picked {
            let id = match format {
                FileFormat::Pdf => "file.export_pdf",
                FileFormat::Fdx => "file.export_fdx",
                FileFormat::Fountain => "file.export_fountain",
                FileFormat::Osf | FileFormat::FadeIn => "file.export_osf",
            };
            let mut params = Map::new();
            params.insert("path".into(), json!(path.display().to_string()));
            self.run(id, params);
        }
    }

    fn apply(&mut self, actions: Vec<EditAction>) {
        for action in actions {
            match action {
                EditAction::SetText { index, mut text, mut caret } => {
                    // Smart quotes in prose only: capitalised elements (headings, cues, transitions)
                    // keep straight apostrophes, so O'BRIEN and CONT'D match as typed.
                    let prose = self.session.element(index).is_some_and(|e| !e.kind.is_caps());
                    if self.prefs.get().general.smart_quotes && prose {
                        let old = self.session.element(index).map(|e| e.text.clone()).unwrap_or_default();
                        if let Some((curled, at)) = storyteller_engine::flow::smart_quote(&old, &text, caret) {
                            text = curled;
                            caret = at;
                            self.editor.pending_focus = Some((index, caret));
                        }
                        if let Some(fixed) = storyteller_engine::flow::fix_leading_apostrophe(&text, caret) {
                            text = fixed;
                            self.editor.pending_focus = Some((index, caret));
                        }
                    }
                    let _ = storyteller_engine::commands::set_text(&mut self.session, index, &text, true);
                    self.session.caret = Caret { element: index, offset: caret };
                    self.session.clamp_caret();
                }
                EditAction::Caret { index, offset } => {
                    self.session.caret = Caret { element: index, offset };
                    self.session.clamp_caret();
                }
                EditAction::Enter { index, offset } => {
                    match self.registry.run("flow.enter", &mut self.session, &params(&[("index", json!(index)), ("offset", json!(offset))])) {
                        Ok(outcome) if outcome.value.get("picker").and_then(Value::as_bool) == Some(true) => {
                            let current = self.session.element(index).map_or(ElementKind::Action, |e| e.kind);
                            let selected = storyteller_engine::flow::PICKER.iter().position(|(_, k)| *k == current).unwrap_or(1);
                            self.editor.picker = Some(editor::Picker { index, selected });
                        }
                        Ok(_) => self.error = None,
                        Err(error) => self.error = Some(error.to_string()),
                    }
                    self.refocus();
                }
                EditAction::Pick { index, kind } => {
                    if let Err(error) = storyteller_engine::commands::element_shortcut(&mut self.session, index, kind) {
                        self.error = Some(error.to_string());
                    }
                    self.refocus();
                }
                EditAction::Tab { index, back } => {
                    self.run(if back { "flow.shift_tab" } else { "flow.tab" }, params(&[("index", json!(index))]));
                    self.refocus();
                }
                EditAction::MergePrevious { index } => {
                    self.run("element.merge_previous", params(&[("index", json!(index))]));
                    self.refocus();
                }
                EditAction::Focus { index, offset } => {
                    let offset = self.session.element(index).map_or(0, |e| offset.min(e.text.len()));
                    self.session.caret = Caret { element: index, offset };
                    self.session.break_typing();
                    self.editor.pending_focus = Some((index, offset));
                }
                EditAction::Accept { index, text, then_enter } => {
                    self.session.break_typing();
                    self.run("element.set_text", params(&[("index", json!(index)), ("text", json!(text))]));
                    let end = self.session.element(index).map_or(0, |e| e.text.len());
                    self.session.caret = Caret { element: index, offset: end };
                    self.editor.completion.dismissed = true;
                    if then_enter {
                        self.run("flow.enter", params(&[("index", json!(index)), ("offset", json!(end))]));
                    }
                    self.refocus();
                }
            }
        }
    }

    // -----------------------------------------------------------------------------------------
    // Control channel

    fn handle_control(&mut self, ctx: &egui::Context) {
        let Some(receiver) = self.control.take() else { return };
        // One request at a time while synthetic input is still being delivered.
        while self.automation.injected.is_empty() && self.queued.is_empty() {
            let Ok(request) = receiver.try_recv() else { break };
            let parsed: Value = serde_json::from_str(&request.line).unwrap_or(Value::Null);
            if parsed.get("method").and_then(Value::as_str) == Some("ui.screenshot") {
                let id = parsed.get("id").cloned().unwrap_or(Value::Null);
                let path = parsed
                    .pointer("/params/path")
                    .and_then(Value::as_str)
                    .map(PathBuf::from)
                    .unwrap_or_else(|| std::env::temp_dir().join("method-storyteller-screenshot.png"));
                ctx.send_viewport_cmd(ViewportCommand::Screenshot(egui::UserData::default()));
                self.pending_screenshot = Some((id, request.reply, path));
                break;
            }
            let reply = {
                let mut host = AppHost {
                    automation: &mut self.automation,
                    editor: &mut self.editor,
                    queued: &mut self.queued,
                    status: &self.status,
                    error: self.error.as_deref(),
                    navigator: self.prefs.get().view.navigator,
                    zoom: self.prefs.get().zoom(),
                    dialogs: (self.title_dialog.is_some(), self.prompt.is_some()),
                    help_windows: self.help.open(),
                };
                handle_line(&request.line, &mut self.session, &self.registry, &mut host)
            };
            let _ = request.reply.send(reply);
            ctx.request_repaint();
        }
        self.control = Some(receiver);
        ctx.request_repaint_after(Duration::from_millis(50));
    }

    fn handle_screenshot(&mut self, ctx: &egui::Context) {
        if self.pending_screenshot.is_none() {
            return;
        }
        let image = ctx.input(|i| {
            i.raw.events.iter().find_map(|e| match e {
                Event::Screenshot { image, .. } => Some(image.clone()),
                _ => None,
            })
        });
        let Some(image) = image else { return };
        let Some((id, reply, path)) = self.pending_screenshot.take() else { return };
        let message = match encode_png(&image) {
            Ok(bytes) => match craft_atomic::atomic_write(&path, &bytes) {
                Ok(()) => json!({ "id": id, "result": { "path": path.display().to_string(), "width": image.size[0], "height": image.size[1] } }),
                Err(error) => json!({ "id": id, "error": format!("could not write {}: {error}", path.display()) }),
            },
            Err(error) => json!({ "id": id, "error": error }),
        };
        let _ = reply.send(message.to_string());
    }

    fn autosave(&mut self) {
        if !self.session.dirty || self.last_autosave.elapsed() < AUTOSAVE_EVERY {
            return;
        }
        self.last_autosave = Instant::now();
        let Some(dir) = craft_settings::paths::data_dir(APP_ID).map(|d| d.join(RECOVERY_DIR)) else { return };
        if std::fs::create_dir_all(&dir).is_err() {
            return;
        }
        let stem =
            self.session.path.as_ref().and_then(|p| p.file_stem()).map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "Untitled".into());
        if let Ok(bytes) = files::encode(&self.session.script, FileFormat::Fountain, &self.session.layout) {
            let _ = craft_atomic::atomic_write(&dir.join(format!("{stem}.recovery.fountain")), &bytes);
        }
    }

    // -----------------------------------------------------------------------------------------
    // Drawing

    fn menu_entries(&self, menu: &str) -> Vec<MenuEntry> {
        let prefix = format!("{menu}/");
        let mut items: Vec<MenuEntry> = self
            .registry
            .iter()
            .filter(|c| c.menu.starts_with(&prefix))
            .map(|c| MenuEntry {
                id: c.id.to_string(),
                label: c.menu.trim_start_matches(&prefix).to_string(),
                shortcut: c.shortcut,
                enabled: (c.enabled)(&self.session) || c.id == "file.save",
            })
            .collect();
        for command in APP_COMMANDS.iter().filter(|c| c.menu.starts_with(&prefix)) {
            let enabled = command.id != "help.check_updates" || self.updates.unavailable().is_none();
            items.push(MenuEntry { id: command.id.to_string(), label: command.label.to_string(), shortcut: command.shortcut, enabled });
        }
        items
    }

    /// The checked state of a toggle command, or `None` for an ordinary one.
    fn checked(&self, id: &str) -> Option<bool> {
        match id {
            "view.navigator" => Some(self.prefs.get().view.navigator),
            "view.page_view" => Some(self.prefs.get().view.page_view),
            "view.scene_numbers" => Some(self.session.layout.show_scene_numbers),
            "prefs.new_fountain" => Some(self.prefs.get().general.new_script_format != "osf"),
            "prefs.new_osf" => Some(self.prefs.get().general.new_script_format == "osf"),
            "prefs.smart_quotes" => Some(self.prefs.get().general.smart_quotes),
            "prefs.check_updates_automatically" => Some(self.updates.automatic()),
            _ => None,
        }
    }

    /// macOS: the real menu bar, with the same entries as the window menu. Elsewhere, and if
    /// that fails, the app menu craft-quit provides (Quit still asks about unsaved changes).
    fn install_menu(&mut self, ctx: &egui::Context) {
        #[cfg(target_os = "macos")]
        {
            let menus: Vec<(&str, Vec<crate::native_menu::Entry>)> = MENUS
                .iter()
                .map(|menu| {
                    let entries = self
                        .menu_entries(menu)
                        .into_iter()
                        .map(|e| crate::native_menu::Entry {
                            check: self.checked(&e.id),
                            id: e.id,
                            label: e.label,
                            shortcut: e.shortcut.map(str::to_string),
                        })
                        .collect();
                    (*menu, entries)
                })
                .collect();
            match crate::native_menu::NativeMenu::install(ctx, APP_NAME, &menus) {
                Ok(menu) => {
                    self.native = Some(menu);
                    return;
                }
                Err(error) => eprintln!("could not install the menu bar: {error}"),
            }
        }
        if let Err(error) = craft_quit::install_app_menu(ctx, APP_NAME) {
            eprintln!("could not install the app menu: {error}");
        }
    }

    /// Whether menus are drawn inside the window (not on macOS with the menu bar installed).
    fn window_menu(&self) -> bool {
        #[cfg(test)]
        if self.pictures {
            return false;
        }
        #[cfg(target_os = "macos")]
        {
            self.native.is_none()
        }
        #[cfg(not(target_os = "macos"))]
        {
            true
        }
    }

    /// macOS: queue chosen menu-bar commands and keep enabled and checked states current.
    fn sync_native_menu(&mut self) {
        #[cfg(target_os = "macos")]
        {
            let Some(mut native) = self.native.take() else { return };
            self.queued.extend(native.take_events());
            for menu in MENUS {
                for entry in self.menu_entries(menu) {
                    let checked = self.checked(&entry.id);
                    native.sync(&entry.id, entry.enabled, checked);
                }
            }
            self.native = Some(native);
        }
    }

    /// Whether the menu bar handles this command's shortcut itself.
    fn menu_owns_shortcut(&self, id: &str) -> bool {
        #[cfg(target_os = "macos")]
        {
            self.native.as_ref().is_some_and(|n| n.has(id))
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = id;
            false
        }
    }

    fn menu_bar(&mut self, ui: &mut egui::Ui) {
        let mut clicked = None;
        egui::MenuBar::new().ui(ui, |ui| {
            for menu in MENUS {
                let items = self.menu_entries(menu);
                let response = ui.menu_button(menu, |ui| {
                    for MenuEntry { id, label, shortcut, enabled } in &items {
                        let mut button = egui::Button::new(label.as_str());
                        if let Some(shortcut) = shortcut.and_then(parse_shortcut) {
                            button = button.shortcut_text(ui.ctx().format_shortcut(&shortcut));
                        }
                        let mut response = match self.checked(id) {
                            Some(on) => ui.add_enabled(*enabled, egui::Button::selectable(on, label.as_str())),
                            None => ui.add_enabled(*enabled, button),
                        };
                        if id == "help.check_updates"
                            && let Some(why) = self.updates.unavailable()
                        {
                            response = response.on_disabled_hover_text(why);
                        }
                        self.automation.add(format!("menu.item.{id}"), "menuitem", label.clone(), &response);
                        if response.clicked() {
                            clicked = Some(id.clone());
                            ui.close();
                        }
                    }
                });
                self.automation.add(format!("menu.{}", menu.to_lowercase()), "menu", menu, &response.response);
            }
        });
        if let Some(id) = clicked {
            self.queued.push(id);
        }
    }

    /// The bar along the top: on macOS it is the window's title bar (drag to move, double-click
    /// to zoom), beside the traffic lights; elsewhere a toolbar under the menus.
    fn top_bar(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let rect = ui.max_rect();
        let title_bar = cfg!(target_os = "macos");
        if title_bar {
            let background = ui.interact(rect, egui::Id::new("storyteller.title_bar"), egui::Sense::click_and_drag());
            if background.drag_started() {
                ctx.send_viewport_cmd(ViewportCommand::StartDrag);
            }
            if background.double_clicked() {
                let maximized = ctx.input(|i| i.viewport().maximized.unwrap_or(false));
                ctx.send_viewport_cmd(ViewportCommand::Maximized(!maximized));
            }
        }
        let title = self.session.title();
        let (name, edited) = match title.strip_suffix(" — edited") {
            Some(name) => (name.to_string(), true),
            None => (title, false),
        };
        let weak = ui.visuals().weak_text_color();
        ui.painter().text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            &name,
            egui::FontId::proportional(theme::TYPE_TITLE),
            ui.visuals().strong_text_color(),
        );
        if edited {
            let width = ui.fonts_mut(|f| f.layout_no_wrap(name.clone(), egui::FontId::proportional(theme::TYPE_TITLE), Color32::WHITE).size().x);
            ui.painter().text(
                rect.center() + egui::vec2(width / 2.0 + 6.0, 0.0),
                egui::Align2::LEFT_CENTER,
                "Edited",
                egui::FontId::proportional(theme::TYPE_SMALL),
                weak,
            );
        }
        let mut clicked = None;
        ui.horizontal_centered(|ui| {
            // The traffic lights sit in the first 76 points of a macOS title bar.
            ui.add_space(if title_bar { 76.0 } else { theme::SPACE_1 });
            let navigator = self.prefs.get().view.navigator;
            let tip = |ui: &egui::Ui, text: &str, shortcut: &str| match parse_shortcut(shortcut) {
                Some(s) => format!("{text} ({})", ui.ctx().format_shortcut(&s)),
                None => text.to_string(),
            };
            let response =
                ui.add(egui::Button::selectable(navigator, "Scenes")).on_hover_text(tip(ui, "Show or hide the scene navigator", "Cmd+Shift+N"));
            self.automation.add("toolbar.navigator", "button", "Scenes", &response);
            if response.clicked() {
                clicked = Some("view.navigator");
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_space(theme::SPACE_2);
                let zoom = self.prefs.get().zoom();
                let response = ui.small_button("+").on_hover_text(tip(ui, "Zoom in", "Cmd+Equals"));
                self.automation.add("toolbar.view.zoom_in", "button", "Zoom in", &response);
                if response.clicked() {
                    clicked = Some("view.zoom_in");
                }
                let percent = ui.add(egui::Button::new(format!("{:.0}%", zoom * 100.0)).frame(false)).on_hover_text(tip(ui, "Actual size", "Cmd+0"));
                self.automation.add("toolbar.view.zoom_reset", "button", "Actual size", &percent);
                if percent.clicked() {
                    clicked = Some("view.zoom_reset");
                }
                let response = ui.small_button("−").on_hover_text(tip(ui, "Zoom out", "Cmd+Minus"));
                self.automation.add("toolbar.view.zoom_out", "button", "Zoom out", &response);
                if response.clicked() {
                    clicked = Some("view.zoom_out");
                }
                ui.add_space(theme::SPACE_3);
                let pages = self.prefs.get().view.page_view;
                for (on, label) in [(false, "Draft"), (true, "Pages")] {
                    let what = if on { "Separate pages, as they print" } else { "One continuous column" };
                    let response = ui.add(egui::Button::selectable(pages == on, label)).on_hover_text(tip(ui, what, "Cmd+Shift+P"));
                    self.automation.add(format!("toolbar.view.{}", label.to_lowercase()), "button", label, &response);
                    if response.clicked() && pages != on {
                        clicked = Some("view.page_view");
                    }
                }
            });
        });
        if let Some(id) = clicked {
            self.queued.push(id.to_string());
        }
    }

    fn navigator(&mut self, ui: &mut egui::Ui) {
        let scenes = self.session.paginated().scenes.clone();
        let lengths: std::collections::HashMap<usize, u32> = self.session.scene_lengths().iter().map(|l| (l.element, l.eighths)).collect();
        let current =
            self.session.script.elements.iter().take(self.session.caret.element + 1).filter(|e| e.kind == ElementKind::SceneHeading).count();
        ui.add_space(theme::SPACE_2);
        ui.label(egui::RichText::new("SCENES").small().strong().color(ui.visuals().weak_text_color()));
        ui.add_space(theme::SPACE_1);
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
            for (n, scene) in scenes.iter().enumerate().filter(|_| !blank) {
                let number = scene.number.as_deref().unwrap_or("");
                let heading = if scene.heading.trim().is_empty() { "(untitled scene)" } else { scene.heading.as_str() };
                let (rect, response) = ui.allocate_exact_size(egui::vec2(ui.available_width(), theme::NAV_ROW_HEIGHT), egui::Sense::click());
                let selected = n + 1 == current;
                if selected || response.hovered() {
                    ui.painter().rect_filled(rect, 5.0, if selected { selected_fill } else { hover_fill });
                }
                let painter = ui.painter().with_clip_rect(rect);
                let small = egui::FontId::proportional(theme::TYPE_SMALL);
                painter.text(rect.left_center() + egui::vec2(30.0, 0.0), egui::Align2::RIGHT_CENTER, number, small.clone(), weak);
                painter.text(rect.right_center() - egui::vec2(8.0, 0.0), egui::Align2::RIGHT_CENTER, scene.page.to_string(), small.clone(), weak);
                // Length in eighths of a page, as a schedule counts it.
                let length = lengths.get(&scene.element).map(|&e| storyteller_core::paginate::format_eighths(e));
                if let Some(length) = &length {
                    painter.text(rect.right_center() - egui::vec2(34.0, 0.0), egui::Align2::RIGHT_CENTER, length, small, weak);
                }
                let galley = ui.fonts_mut(|f| {
                    let mut job = egui::text::LayoutJob::simple_singleline(heading.to_string(), egui::FontId::proportional(theme::TYPE_BODY), text);
                    job.wrap = egui::text::TextWrapping::truncate_at_width(rect.width() - 118.0);
                    f.layout_job(job)
                });
                painter.galley(rect.left_center() + egui::vec2(38.0, -galley.size().y / 2.0), galley, text);
                let detail = match &length {
                    Some(length) => format!("{heading}\nPage {}, length {length}", scene.page),
                    None => format!("{heading}\nPage {}", scene.page),
                };
                let response = response.on_hover_text(detail);
                self.automation.add(format!("navigator.scene.{n}"), "listitem", heading.to_string(), &response);
                if response.clicked() {
                    self.editor.pending_focus = Some((scene.element, 0));
                    self.session.caret = Caret { element: scene.element, offset: 0 };
                }
            }
        });
    }

    fn status_bar(&mut self, ui: &mut egui::Ui) {
        let caret = self.session.caret;
        let kind = self.session.current().map_or(ElementKind::Action, |e| e.kind);
        let pages = self.session.paginated().page_count().max(1);
        let page = self.session.paginated().element_pages.get(caret.element).copied().unwrap_or(1).max(1);
        ui.horizontal(|ui| {
            let mut chosen = kind;
            let response = egui::ComboBox::from_id_salt("storyteller.element_kind").selected_text(kind.label()).show_ui(ui, |ui| {
                for (n, option) in SHORTCUT_KINDS
                    .iter()
                    .chain([ElementKind::Centered, ElementKind::Lyrics, ElementKind::Section, ElementKind::Synopsis].iter())
                    .enumerate()
                {
                    let r = ui.selectable_value(&mut chosen, *option, option.label());
                    self.automation.add(format!("status.element_kind.{}", option.id()), "option", option.label(), &r);
                    let _ = n;
                }
            });
            self.automation.add("status.element_kind", "combobox", kind.label(), &response.response);
            if chosen != kind {
                self.run("element.set_kind", params(&[("index", json!(caret.element)), ("kind", json!(chosen.id()))]));
                self.refocus();
            }
            ui.separator();
            let page_label = ui.label(format!("Page {page} of {pages}"));
            self.automation.add("status.page", "text", format!("Page {page} of {pages}"), &page_label);
            ui.separator();
            // What Tab and Enter will do here, as Final Draft's status bar shows.
            let text = self.session.current().map(|e| e.text.clone()).unwrap_or_default();
            let hint = if self.editor.picker.is_some() {
                "Type a letter, or use the arrows and Enter    Esc: keep it".to_string()
            } else {
                storyteller_engine::flow::hint(kind, &text)
            };
            let hint_label = ui.label(egui::RichText::new(&hint).weak());
            self.automation.add("status.hint", "text", hint, &hint_label);
            ui.separator();
            // The update's line goes in the status once per change, so other messages still show.
            let update = self.updates.status();
            let line = crate::updates::status_line(&update);
            if line != self.update_line {
                if let Some(line) = &line {
                    self.status = line.clone();
                }
                self.update_line = line;
            }
            if update.phase == "ready" {
                let restart = ui.button("Restart to Update");
                self.automation.add("status.restart_to_update", "button", "Restart to Update", &restart);
                if restart.clicked() {
                    self.queued.push("help.restart_to_update".into());
                }
            }
            if let Some(error) = &self.error {
                ui.colored_label(ui.visuals().error_fg_color, error);
            } else {
                ui.weak(&self.status);
            }
        });
    }

    /// "Restart to Update": the script is saved first; an untitled script must be saved by
    /// the writer before the app restarts.
    fn restart_to_update(&mut self) {
        if self.session.dirty {
            if self.session.path.is_none() {
                self.status = "Save the script, then restart to update.".into();
                return;
            }
            if !self.save(false) {
                return;
            }
        }
        let _ = self.prefs.flush();
        match self.updates.install_and_relaunch() {
            Ok(()) => std::process::exit(0),
            Err(error) => self.error = Some(crate::messages::update_install_failed(Some(&error.to_string()))),
        }
    }

    fn title_page_dialog(&mut self, ctx: &egui::Context) {
        let Some(draft) = &mut self.title_dialog else { return };
        let mut done = None;
        theme::centered(egui::Window::new("Title Page").collapsible(false).resizable(false), ctx).show(ctx, |ui| {
            egui::Grid::new("storyteller.title_grid").num_columns(2).spacing([12.0, 6.0]).show(ui, |ui| {
                for (key, value) in &mut draft.fields {
                    ui.label(key.as_str());
                    let multiline = matches!(key.as_str(), "Contact" | "Notes" | "Title" | "Author");
                    let edit = if multiline { egui::TextEdit::multiline(value).desired_rows(2) } else { egui::TextEdit::singleline(value) };
                    let response = ui.add(edit.desired_width(320.0));
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

    fn rename_dialog(&mut self, ctx: &egui::Context) {
        let Some(draft) = &mut self.rename else { return };
        let mut go = false;
        let mut close = ctx.input(|i| i.key_pressed(egui::Key::Escape));
        egui::Window::new("Rename Character").collapsible(false).resizable(false).anchor(egui::Align2::CENTER_TOP, [0.0, 120.0]).show(ctx, |ui| {
            egui::Grid::new("storyteller.rename_grid").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
                ui.label("Character");
                let combo =
                    egui::ComboBox::from_id_salt("storyteller.rename_from").selected_text(draft.from.as_str()).width(240.0).show_ui(ui, |ui| {
                        for name in &draft.names {
                            ui.selectable_value(&mut draft.from, name.clone(), name.as_str());
                        }
                    });
                self.automation.add("dialog.rename.from", "combobox", "Character", &combo.response);
                ui.end_row();
                ui.label("New name");
                let to = ui.add(egui::TextEdit::singleline(&mut draft.to).desired_width(240.0));
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
                        storyteller_engine::CommandError::Invalid { reason, .. } => capitalise(&reason),
                        other => other.to_string(),
                    });
                }
            }
        }
    }

    fn go_to_dialog(&mut self, ctx: &egui::Context) {
        let Some(draft) = &mut self.go_to else { return };
        let mut go = false;
        let mut close = ctx.input(|i| i.key_pressed(egui::Key::Escape));
        egui::Window::new("Go To").collapsible(false).resizable(false).anchor(egui::Align2::CENTER_TOP, [0.0, 120.0]).show(ctx, |ui| {
            ui.horizontal(|ui| {
                let scene = ui.selectable_value(&mut draft.page, false, "Scene");
                self.automation.add("dialog.go_to.scene", "radio", "Scene", &scene);
                let page = ui.selectable_value(&mut draft.page, true, "Page");
                self.automation.add("dialog.go_to.page", "radio", "Page", &page);
            });
            let hint = if draft.page { "Page number" } else { "Scene number, such as 12 or 12A" };
            let response = ui.add(egui::TextEdit::singleline(&mut draft.text).hint_text(hint).desired_width(240.0));
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
        let target = if draft.page {
            match text.parse::<u64>() {
                Ok(page) => params(&[("page", json!(page))]),
                Err(_) => {
                    draft.error = Some("Type a page number.".into());
                    return;
                }
            }
        } else {
            params(&[("scene", json!(text))])
        };
        match self.registry.run("nav.go_to", &mut self.session, &target) {
            Ok(_) => {
                self.go_to = None;
                self.editor.pending_focus = Some((self.session.caret.element, 0));
            }
            Err(error) => {
                if let Some(draft) = &mut self.go_to {
                    draft.error = Some(match error {
                        storyteller_engine::CommandError::Invalid { reason, .. } => capitalise(&reason),
                        other => other.to_string(),
                    });
                }
            }
        }
    }

    fn help_windows(&mut self, ctx: &egui::Context) {
        let rows = if self.help.shortcuts { self.shortcut_rows(ctx) } else { Vec::new() };
        match self.help.show(ctx, &mut self.automation, &rows) {
            Some(HelpAction::Open(url)) => self.open_link(url, "Opened the website in your browser."),
            Some(HelpAction::Licenses) => self.queued.push("help.licenses".into()),
            None => {}
        }
    }

    fn unsaved_prompt(&mut self, ctx: &egui::Context) {
        if self.prompt.is_none() {
            return;
        }
        let mut answer = None;
        egui::Modal::new(egui::Id::new("storyteller.unsaved")).frame(theme::modal_frame(&ctx.global_style())).show(ctx, |ui| {
            ui.heading("Save changes?");
            ui.label(format!("“{}” has changes that are not saved.", self.session.title().trim_end_matches(" — edited")));
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
                    self.session.dirty = false;
                    self.perform(ctx, action);
                }
            }
            Some(_) => self.prompt = None,
            None => {}
        }
    }

    fn draw(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        if self.window_menu() {
            egui::Panel::top("storyteller.menu").show(ui, |ui| self.menu_bar(ui));
        }
        egui::Panel::top("storyteller.top_bar").exact_size(theme::TOP_BAR_HEIGHT).show(ui, |ui| self.top_bar(ui));
        if self.find.is_some() {
            egui::Panel::top("storyteller.find").show(ui, |ui| self.find_bar(ui));
        }
        egui::Panel::bottom("storyteller.status").show(ui, |ui| self.status_bar(ui));
        if self.prefs.get().view.navigator {
            egui::Panel::left("storyteller.navigator").resizable(true).default_size(250.0).show(ui, |ui| self.navigator(ui));
        }
        let zoom = self.prefs.get().zoom();
        let pages = self.prefs.get().view.page_view;
        let actions = egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(ui.visuals().extreme_bg_color))
            .show(ui, |ui| editor::show(ui, &mut self.session, &mut self.editor, &mut self.automation, zoom, pages))
            .inner;
        self.apply(actions);
        self.title_page_dialog(&ctx);
        self.go_to_dialog(&ctx);
        self.rename_dialog(&ctx);
        self.help_windows(&ctx);
        self.unsaved_prompt(&ctx);
        for id in std::mem::take(&mut self.queued) {
            self.run_command(&ctx, &id);
        }
    }
}

fn params(pairs: &[(&str, Value)]) -> Map<String, Value> {
    pairs.iter().map(|(k, v)| (k.to_string(), v.clone())).collect()
}

fn encode_png(image: &egui::ColorImage) -> Result<Vec<u8>, String> {
    let [width, height] = image.size;
    let mut rgba = Vec::new();
    rgba.try_reserve(width.saturating_mul(height).saturating_mul(4)).map_err(|_| "screenshot too large".to_string())?;
    for pixel in &image.pixels {
        rgba.extend_from_slice(&pixel.to_srgba_unmultiplied());
    }
    let mut out = Vec::new();
    {
        let width = u32::try_from(width).map_err(|_| "screenshot too wide")?;
        let height = u32::try_from(height).map_err(|_| "screenshot too tall")?;
        let mut encoder = png::Encoder::new(&mut out, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().map_err(|e| e.to_string())?;
        writer.write_image_data(&rgba).map_err(|e| e.to_string())?;
    }
    Ok(out)
}

/// `ui.*` methods of the control channel.
struct AppHost<'a> {
    automation: &'a mut Automation,
    editor: &'a mut EditorState,
    queued: &'a mut Vec<String>,
    status: &'a str,
    error: Option<&'a str>,
    navigator: bool,
    zoom: f32,
    dialogs: (bool, bool),
    help_windows: Vec<&'static str>,
}

impl UiHost for AppHost<'_> {
    fn ui(&mut self, method: &str, params: &Value) -> Result<Value, String> {
        let text = |name: &str| params.get(name).and_then(Value::as_str).map(str::to_string);
        match method {
            "ui.elements" => Ok(self.automation.list()),
            "ui.click" => self.automation.click(&text("id").ok_or("missing `id`")?),
            "ui.type" => {
                if let Some(id) = text("id") {
                    self.automation.click(&id)?;
                }
                self.automation.type_text(&text("text").ok_or("missing `text`")?);
                Ok(json!({ "queued": "type" }))
            }
            "ui.key" => {
                let modifiers: Vec<String> = params
                    .get("modifiers")
                    .and_then(Value::as_array)
                    .map(|m| m.iter().filter_map(|v| v.as_str().map(str::to_string)).collect())
                    .unwrap_or_default();
                self.automation.key(&text("key").ok_or("missing `key`")?, &modifiers)
            }
            "ui.focus" => {
                let index = params.get("element").and_then(Value::as_u64).and_then(|v| usize::try_from(v).ok()).ok_or("missing `element`")?;
                let offset = params.get("offset").and_then(Value::as_u64).and_then(|v| usize::try_from(v).ok()).unwrap_or(usize::MAX);
                self.editor.pending_focus = Some((index, offset));
                Ok(json!({ "queued": "focus" }))
            }
            "ui.command" => {
                let id = text("id").ok_or("missing `id`")?;
                self.queued.push(id.clone());
                Ok(json!({ "queued": id }))
            }
            "ui.state" => Ok(json!({
                "focused_element": self.editor.focused,
                "completion": if self.editor.completion.dismissed { Vec::new() } else { self.editor.completion.items.clone() },
                "navigator": self.navigator,
                "zoom": self.zoom,
                "title_page_dialog": self.dialogs.0,
                "unsaved_prompt": self.dialogs.1,
                "help_windows": self.help_windows,
                "status": self.status,
                "error": self.error,
            })),
            other => Err(format!("unknown method `{other}`")),
        }
    }
}

impl eframe::App for StorytellerApp {
    fn raw_input_hook(&mut self, ctx: &egui::Context, raw_input: &mut egui::RawInput) {
        if let Some(batch) = self.automation.injected.pop_front() {
            raw_input.events.extend(batch);
            ctx.request_repaint();
        }
    }

    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Edit > Cut and Copy, chosen last frame: the field has its focus back now.
        let input = std::mem::take(&mut self.next_frame_input);
        if !input.is_empty() {
            ctx.input_mut(|i| i.events.extend(input));
            ctx.request_repaint();
        }
        self.handle_screenshot(ctx);
        self.handle_control(ctx);
        self.sync_native_menu();
        if self.prompt.is_none() && self.title_dialog.is_none() && self.go_to.is_none() && self.rename.is_none() {
            for (shortcut, id) in self.shortcuts.clone() {
                if self.menu_owns_shortcut(id) {
                    continue;
                }
                if ctx.input_mut(|i| i.consume_shortcut(&shortcut)) {
                    self.queued.push(id.to_string());
                }
            }
        }
        // ⌘1–⌘7 are in the registry as format.* commands; anything queued runs after drawing.
        self.autosave();
        let tick = self.prefs.tick(Instant::now());
        if let Some(deadline) = tick.next_deadline {
            ctx.request_repaint_after(deadline.saturating_duration_since(Instant::now()));
        }
        let title = format!("{} — {APP_NAME}", self.session.title());
        if title != self.window_title {
            ctx.send_viewport_cmd(ViewportCommand::Title(title.clone()));
            self.window_title = title;
        }
        if ctx.input(|i| i.viewport().close_requested()) && self.session.dirty && !self.discard_confirmed {
            ctx.send_viewport_cmd(ViewportCommand::CancelClose);
            self.prompt = Some(Pending::Close);
        }
        craft_quit::on_close_request(ctx);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.automation.begin_frame();
        if catch_unwind(AssertUnwindSafe(|| self.draw(ui))).is_err() {
            self.error = Some("Internal error while drawing. Your script is intact; please save and report this.".into());
        }
        self.automation.end_frame();
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        let _ = self.prefs.flush();
    }
}

/// `there is no page 9` → `There is no page 9.`
fn capitalise(text: &str) -> String {
    let mut chars = text.chars();
    let first = chars.next().map(|c| c.to_uppercase().collect::<String>()).unwrap_or_default();
    format!("{first}{}.", chars.as_str())
}

#[path = "find_bar.rs"]
mod find_bar;

#[cfg(test)]
#[path = "screens.rs"]
mod screens;

#[cfg(test)]
#[path = "ui_tests.rs"]
mod ui_tests;
