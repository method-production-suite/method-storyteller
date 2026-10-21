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

const APP_COMMANDS: [AppCommand; 41] = [
    AppCommand { id: "app.close", label: "Close", menu: "File/Close", shortcut: Some("Cmd+W") },
    AppCommand { id: "app.title_page", label: "Title Page…", menu: "File/Title Page…", shortcut: Some("Cmd+Shift+T") },
    AppCommand {
        id: "prefs.new_mstory",
        label: "Save New Scripts as Method Storyteller Packages",
        menu: "File/Save New Scripts as Method Storyteller Packages",
        shortcut: None,
    },
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
    AppCommand { id: "app.spelling", label: "Spelling…", menu: "Edit/Spelling…", shortcut: Some("Cmd+Semicolon") },
    AppCommand { id: "prefs.check_spelling", label: "Check Spelling While Typing", menu: "View/Check Spelling While Typing", shortcut: None },
    AppCommand { id: "prefs.spelling_us", label: "US English Spelling", menu: "Edit/US English Spelling", shortcut: None },
    AppCommand { id: "prefs.spelling_uk", label: "UK English Spelling", menu: "Edit/UK English Spelling", shortcut: None },
    AppCommand { id: "app.find", label: "Find…", menu: "Edit/Find…", shortcut: Some("Cmd+F") },
    AppCommand { id: "app.find_next", label: "Find Next", menu: "Edit/Find Next", shortcut: Some(FIND_NEXT_KEY) },
    AppCommand { id: "app.find_previous", label: "Find Previous", menu: "Edit/Find Previous", shortcut: Some(FIND_PREVIOUS_KEY) },
    AppCommand { id: "app.rename_character", label: "Rename Character…", menu: "Production/Rename Character…", shortcut: None },
    AppCommand { id: "app.characters", label: "Characters", menu: "Production/Characters", shortcut: None },
    AppCommand { id: "app.edit_scene_number", label: "Edit Scene Number…", menu: "Production/Edit Scene Number…", shortcut: None },
    AppCommand { id: "app.revisions", label: "Revisions…", menu: "Production/Revisions…", shortcut: None },
    AppCommand { id: "app.go_to", label: "Go To…", menu: "Edit/Go To…", shortcut: Some(GO_TO_KEY) },
    AppCommand { id: "prefs.smart_quotes", label: "Smart Quotes", menu: "Format/Smart Quotes", shortcut: None },
    AppCommand { id: "app.bold", label: "Bold", menu: "Format/Bold", shortcut: Some("Cmd+B") },
    AppCommand { id: "app.italic", label: "Italic", menu: "Format/Italic", shortcut: Some("Cmd+I") },
    AppCommand { id: "app.underline", label: "Underline", menu: "Format/Underline", shortcut: Some("Cmd+U") },
    AppCommand { id: "view.navigator", label: "Scene Navigator", menu: "View/Scene Navigator", shortcut: Some("Cmd+Shift+N") },
    AppCommand { id: "view.page_view", label: "Page View", menu: "View/Page View", shortcut: Some("Cmd+Shift+P") },
    AppCommand { id: "view.zoom_in", label: "Zoom In", menu: "View/Zoom In", shortcut: Some("Cmd+Equals") },
    AppCommand { id: "view.zoom_out", label: "Zoom Out", menu: "View/Zoom Out", shortcut: Some("Cmd+Minus") },
    AppCommand { id: "view.zoom_reset", label: "Actual Size", menu: "View/Actual Size", shortcut: Some("Cmd+0") },
    AppCommand { id: "help.about", label: "About Method Storyteller", menu: "Help/About Method Storyteller", shortcut: None },
    AppCommand { id: "help.user_guide", label: "User Guide", menu: "Help/User Guide", shortcut: None },
    AppCommand { id: "help.shortcuts", label: "Keyboard Shortcuts", menu: "Help/Keyboard Shortcuts", shortcut: None },
    AppCommand { id: "help.whats_new", label: "What's New", menu: "Help/What's New", shortcut: None },
    AppCommand { id: "help.open_sample", label: "Open Sample Script", menu: "Help/Open Sample Script", shortcut: None },
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

/// The keys the text fields handle themselves for Edit > Cut, Copy, Paste and Select All:
/// listed in Keyboard Shortcuts and shown beside the menu drawn in the window, but never bound
/// as shortcuts (a binding would take them from the text fields).
const TEXT_FIELD_KEYS: [(&str, &str); 4] = [("app.cut", "Cmd+X"), ("app.copy", "Cmd+C"), ("app.paste", "Cmd+V"), ("app.select_all", "Cmd+A")];

fn text_field_key(id: &str) -> Option<&'static str> {
    TEXT_FIELD_KEYS.iter().find(|(command, _)| *command == id).map(|(_, keys)| *keys)
}

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
    Sample,
    NewTv(storyteller_core::model::TvShape),
}

struct TitleDraft {
    fields: Vec<(String, String)>,
}

/// Production > Revisions…: the names and marks being edited, by set id, and a delete waiting
/// for its answer.
#[derive(Default)]
struct RevisionsDraft {
    names: std::collections::HashMap<u32, String>,
    marks: std::collections::HashMap<u32, String>,
    confirm_delete: Option<u32>,
    error: Option<String>,
}

/// Production > Number Scenes….
struct NumberingDraft {
    start: String,
    keep_existing: bool,
    from_caret: bool,
    skip_io: bool,
    error: Option<String>,
}

/// Production > Edit Scene Number….
struct SceneNumberDraft {
    heading: usize,
    number: String,
    error: Option<String>,
    focused: bool,
}

/// The Scene Properties dialog: a scene's title, summary and colour.
struct SceneDraft {
    heading: usize,
    title: String,
    summary: String,
    color: Option<String>,
    focused: bool,
}

/// The colours a scene can take, as the Scene Properties dialog offers them: name and #RRGGBB.
const SCENE_COLORS: [(&str, &str); 8] = [
    ("Red", "#E06666"),
    ("Orange", "#F6B26B"),
    ("Yellow", "#FFD966"),
    ("Green", "#93C47D"),
    ("Teal", "#76A5AF"),
    ("Blue", "#6FA8DC"),
    ("Purple", "#8E7CC3"),
    ("Pink", "#C27BA0"),
];

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

/// What the Go To dialog looks for.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum GoToMode {
    #[default]
    Scene,
    Page,
    Act,
}

/// The Go To dialog: a scene number, a page or an act.
#[derive(Default)]
struct GoToDraft {
    text: String,
    mode: GoToMode,
    error: Option<String>,
    focused: bool,
    /// A tab was clicked: give the number field the keyboard on the next frame (egui drops it at
    /// the end of a frame whose click landed outside a text field).
    refocus: bool,
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
    /// Where recovery copies go (`None` with settings in memory, as in tests, unless set).
    recovery_dir: Option<PathBuf>,
    /// A recovery copy found at launch, waiting for the writer's answer.
    recovered: Option<PathBuf>,
    /// The name the open script's recovery copy goes under: its file's name, made unique among
    /// copies still waiting, or a recovered copy's own name until it is saved.
    recovery_name: String,
    title_dialog: Option<TitleDraft>,
    go_to: Option<GoToDraft>,
    /// File > New TV Script: the shape chosen so far.
    new_tv: Option<storyteller_core::model::TvShape>,
    rename: Option<RenameDraft>,
    scene_dialog: Option<SceneDraft>,
    /// Production > Characters: the character table is open.
    characters: bool,
    numbering: Option<NumberingDraft>,
    revisions: Option<RevisionsDraft>,
    scene_number: Option<SceneNumberDraft>,
    find: Option<find_bar::FindBar>,
    spelling: Option<spelling::SpellDialog>,
    spell_cache: spelling::SpellCache,
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
    /// The open script is a fresh copy of the sample (see `sample.rs`).
    sample: bool,
    /// Input for the next frame: Edit > Cut and Copy, after the text field has its focus back.
    next_frame_input: Vec<Event>,
    /// A caret to give the keyboard to on the next frame (after a click outside the editor).
    focus_next_frame: Option<(usize, usize)>,
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

/// File > Open Recent: the submenu's place among the File entries, and its items' id prefix
/// (`app.open_recent.0` is the most recent script).
const OPEN_RECENT: &str = "app.open_recent";
const RECENT_PREFIX: &str = "app.open_recent.";

/// A recent script's menu label: its file name, and its folder after a dash, so two
/// `Pilot.mstory` files stay apart.
fn recent_label(path: &str) -> String {
    let path = Path::new(path);
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| path.display().to_string());
    match path.parent().and_then(|p| p.file_name()) {
        Some(folder) => format!("{name} – {}", folder.to_string_lossy()),
        None => name,
    }
}

impl StorytellerApp {
    pub fn new(cc: &eframe::CreationContext<'_>, file: Option<PathBuf>, control_port: Option<u16>, flags: sample::StartFlags) -> Self {
        let mut app = Self::without_menu(cc, file, control_port, Location::Auto, flags);
        app.install_menu(&cc.egui_ctx);
        app
    }

    /// The app without the macOS menu bar, which offscreen tests and pictures leave out.
    /// Tests pass [`Location::Memory`] so they never touch real settings.
    fn without_menu(
        cc: &eframe::CreationContext<'_>,
        file: Option<PathBuf>,
        control_port: Option<u16>,
        settings: Location,
        flags: sample::StartFlags,
    ) -> Self {
        editor::install_fonts(&cc.egui_ctx);
        theme::apply(&cc.egui_ctx);
        // Settings in memory (tests) keep the updater's switch in memory too.
        let in_memory = matches!(settings, Location::Memory);
        let (prefs, report) = SettingsStore::<Prefs>::open(settings);
        let prefs_automatic = prefs.get().updates.check_automatically;
        // The first run picks the spelling dictionary from the system's region (decision 16);
        // settings in memory (tests) stay with US English, whatever the machine's region.
        let mut prefs = prefs;
        if prefs.get().general.spelling_language.is_empty() {
            let region = if in_memory { None } else { storyteller_macos_open::region() };
            let language = region.map_or(storyteller_engine::spell::Language::Us, |r| storyteller_engine::spell::Language::for_region(&r));
            prefs.update(|p| p.general.spelling_language = language.id().into());
        }
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
            recovery_dir: if in_memory { None } else { craft_settings::paths::data_dir(APP_ID).map(|d| d.join(RECOVERY_DIR)) },
            recovered: None,
            recovery_name: String::new(),
            title_dialog: None,
            go_to: None,
            new_tv: None,
            rename: None,
            scene_dialog: None,
            characters: false,
            numbering: None,
            revisions: None,
            scene_number: None,
            find: None,
            spelling: None,
            spell_cache: spelling::SpellCache::default(),
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
            sample: false,
            next_frame_input: Vec::new(),
            focus_next_frame: None,
            #[cfg(test)]
            pictures: false,
        };
        let last_seen = app.prefs.get().general.last_seen_version.clone();
        app.help.whats_new = help::whats_new_due(&last_seen, help::VERSION);
        if help::record_version(&last_seen, help::VERSION) {
            app.prefs.update(|p| p.general.last_seen_version = help::VERSION.to_string());
        }
        app.session.layout.show_scene_numbers = app.prefs.get().view.scene_numbers;
        let ctx = cc.egui_ctx.clone();
        storyteller_macos_open::set_wake(move || ctx.request_repaint());
        // What to open (sample::launch_choice). The first launch retires the sample whatever
        // opens: a writer who starts with their own script never sees it unasked.
        let from_finder = app.first_from_finder();
        let general = &app.prefs.get().general;
        let first_launch = !general.sample_seen;
        let has_recent = !general.recent.is_empty();
        if first_launch {
            app.prefs.update(|p| p.general.sample_seen = true);
        }
        // Unsaved work from a launch that ended without saving comes first: offered, and the
        // sample stays away so the question is the first thing seen.
        let first_launch = !app.find_recovery() && first_launch;
        app.recovery_name = app.fresh_recovery_name();
        match sample::launch_choice(file, from_finder, first_launch, has_recent, flags.open_sample) {
            sample::LaunchOpen::File(path) => app.open_path(&path),
            sample::LaunchOpen::Sample => app.open_sample(),
            sample::LaunchOpen::Nothing => {}
        }
        app.queued.extend(flags.commands().into_iter().map(String::from));
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
        if let Err(error) = self.try_run(id, params) {
            self.error = Some(error.to_string());
        }
    }

    /// Runs a command, shows its message, and hands back the error for the caller to word.
    fn try_run(&mut self, id: &str, params: Map<String, Value>) -> Result<(), storyteller_engine::CommandError> {
        let outcome = self.registry.run(id, &mut self.session, &params)?;
        if let Some(message) = outcome.message {
            self.status = message;
        }
        self.error = None;
        Ok(())
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
            "app.bold" => self.style_selection(ctx, "bold"),
            "app.italic" => self.style_selection(ctx, "italic"),
            "app.underline" => self.style_selection(ctx, "underline"),
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
            "app.spelling" => self.open_spelling(),
            "prefs.check_spelling" => {
                self.prefs.update(|p| p.view.check_spelling = !p.view.check_spelling);
            }
            "app.find" => match &mut self.find {
                Some(bar) => bar.focus(),
                None => {
                    self.spelling = None;
                    self.find = Some(find_bar::FindBar::open());
                }
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
            "prefs.spelling_us" | "prefs.spelling_uk" => {
                let language =
                    if id == "prefs.spelling_uk" { storyteller_engine::spell::Language::Uk } else { storyteller_engine::spell::Language::Us };
                self.prefs.update(|p| p.general.spelling_language = language.id().into());
                self.status = format!("Spelling is checked in {}.", language.label());
            }
            "prefs.new_mstory" => {
                self.prefs.update(|p| p.general.new_script_format = "mstory".into());
                self.status = "New scripts will be saved as Method Storyteller packages (.mstory).".into();
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
            "help.open_sample" => self.guard_unsaved(ctx, Pending::Sample),
            // The menu asks the shape first; the control channel can run file.new_tv with one.
            "file.new_tv" => self.new_tv = Some(self.prefs.get().general.tv_shape),
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
            "edit.copy_scene" => self.copy_scene(ctx, None),
            // File > Close: the script closes and an empty one takes its place (decision 15).
            "app.close" => self.guard_unsaved(ctx, Pending::New),
            "app.characters" => self.characters = !self.characters,
            "app.revisions" => self.revisions = Some(RevisionsDraft::default()),
            "scene.number" => {
                let numbered = self.session.script.scene_indices().iter().any(|&h| self.session.element(h).is_some_and(|e| e.scene_number.is_some()));
                self.numbering = Some(NumberingDraft {
                    start: "1".into(),
                    keep_existing: numbered,
                    from_caret: false,
                    skip_io: self.prefs.get().general.skip_io_letters,
                    error: None,
                });
            }
            "app.edit_scene_number" => {
                let caret = self.session.caret.element;
                match self.session.script.elements.iter().take(caret + 1).rposition(|e| e.kind == ElementKind::SceneHeading) {
                    Some(heading) => {
                        let number = self.session.element(heading).and_then(|e| e.scene_number.clone()).unwrap_or_default();
                        self.scene_number = Some(SceneNumberDraft { heading, number, error: None, focused: false });
                    }
                    None => self.status = "Put the caret in a scene to edit its number.".into(),
                }
            }
            "app.clear_recent" => {
                self.prefs.update(|p| p.general.recent.clear());
                self.status = "Cleared the list of recent scripts.".into();
            }
            recent if recent.starts_with(RECENT_PREFIX) => {
                let path = recent
                    .strip_prefix(RECENT_PREFIX)
                    .and_then(|n| n.parse::<usize>().ok())
                    .and_then(|n| self.prefs.get().general.recent.get(n).cloned());
                if let Some(path) = path {
                    self.guard_unsaved(ctx, Pending::Open(Some(PathBuf::from(path))));
                }
            }
            _ => {
                self.run(id, Map::new());
                self.refocus();
            }
        }
    }

    /// Edit > Copy Scene, or Copy Scene on a navigator row: the scene as Fountain text, which
    /// pastes back into Storyteller as the same elements and into anything else as a screenplay.
    fn copy_scene(&mut self, ctx: &egui::Context, heading: Option<usize>) {
        let index = heading.unwrap_or(self.session.caret.element);
        match self.registry.run("edit.copy_scene", &mut self.session, &params(&[("index", json!(index))])) {
            Ok(outcome) => {
                if let Some(text) = outcome.value.get("text").and_then(Value::as_str) {
                    ctx.copy_text(text.to_string());
                }
                self.status = outcome.message.unwrap_or_default();
                self.error = None;
            }
            Err(error) => self.error = Some(error.to_string()),
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
        let commands = self
            .registry
            .iter()
            .map(|c| (c.menu, c.label, c.shortcut))
            .chain(APP_COMMANDS.iter().map(|c| (c.menu, c.label, c.shortcut.or(text_field_key(c.id)))));
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

    /// Format > Bold, Italic and Underline: on the selection in the caret's paragraph, or on the
    /// word the caret is in; off again when all of it has the style already. The text stays
    /// selected.
    fn style_selection(&mut self, ctx: &egui::Context, style: &str) {
        self.session.clamp_caret();
        let index = self.session.caret.element;
        let Some(text) = self.session.element(index).map(|e| e.text.clone()) else { return };
        let chars: Vec<char> = text.chars().collect();
        let (mut from, mut to) = match self.editor.selection.filter(|(i, _)| *i == index) {
            Some((_, range)) => {
                let (a, b): (usize, usize) = (range.primary.index.into(), range.secondary.index.into());
                (a.min(b), a.max(b))
            }
            None => {
                let at = crate::editor::byte_to_char(&text, self.session.caret.offset);
                (at, at)
            }
        };
        if from == to {
            // The word the caret is in.
            let word = |c: &char| c.is_alphanumeric() || matches!(c, '\'' | '’' | '-');
            while from > 0 && chars.get(from - 1).is_some_and(word) {
                from -= 1;
            }
            while chars.get(to).is_some_and(word) {
                to += 1;
            }
        }
        if from == to {
            self.status = "Select the words to style first.".into();
            self.refocus();
            return;
        }
        let (start, end) = (crate::editor::char_to_byte(&text, from), crate::editor::char_to_byte(&text, to));
        let target = params(&[("index", json!(index)), ("start", json!(start)), ("end", json!(end)), ("style", json!(style))]);
        match self.registry.run("format.style", &mut self.session, &target) {
            Ok(outcome) => {
                let text = self.session.element(index).map(|e| e.text.clone()).unwrap_or_default();
                let byte = |key: &str| outcome.value.get(key).and_then(Value::as_u64).and_then(|v| usize::try_from(v).ok()).unwrap_or(0);
                let (a, b) = (crate::editor::byte_to_char(&text, byte("start")), crate::editor::byte_to_char(&text, byte("end")));
                self.editor.selection = Some((index, egui::text::CCursorRange::two(egui::text::CCursor::new(a), egui::text::CCursor::new(b))));
                self.focus_caret_field(ctx);
                ctx.request_repaint();
            }
            Err(error) => {
                self.status = match error {
                    storyteller_engine::CommandError::Invalid { reason, .. } => capitalise(&reason),
                    other => other.to_string(),
                };
                self.refocus();
            }
        }
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
            Pending::Sample => self.open_sample(),
            Pending::NewTv(shape) => {
                self.run("file.new_tv", params(&[("shape", json!(shape.id()))]));
                let caret = self.session.caret;
                self.after_open();
                self.session.caret = caret;
                self.editor.pending_focus = Some((caret.element, 0));
            }
            Pending::Open(path) => {
                let path = path.or_else(|| {
                    rfd::FileDialog::new().add_filter("Screenplay", &["mstory", "fountain", "spmd", "txt", "osf", "fadein", "fdx"]).pick_file()
                });
                if let Some(path) = path {
                    self.open_path(&path);
                }
            }
        }
    }

    fn open_path(&mut self, path: &Path) {
        let mut params = Map::new();
        params.insert("path".into(), json!(path.display().to_string()));
        if let Err(error) = self.try_run("file.open", params) {
            self.error = Some(crate::messages::open_failed(path, error.file_problem(), &error.to_string()));
        } else {
            let shown = path.display().to_string();
            self.prefs.update(|p| p.push_recent(&shown));
        }
        self.after_open();
    }

    fn after_open(&mut self) {
        self.recovery_name = self.fresh_recovery_name();
        self.session.layout.show_scene_numbers = self.prefs.get().view.scene_numbers;
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
    fn save(&mut self, as_new: bool) -> bool {
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
            self.prefs.update(|p| p.push_recent(&shown));
        }
        saved
    }

    /// A file opened from the Finder while the app runs, after asking about unsaved changes.
    /// Waits while a question is already on screen.
    fn open_from_finder(&mut self, ctx: &egui::Context) {
        if self.prompt.is_some() {
            return;
        }
        if let Some(path) = self.first_from_finder() {
            self.guard_unsaved(ctx, Pending::Open(Some(path)));
        }
    }

    /// The first of the files opened from the Finder. The window holds one script, so any
    /// others are named in the status line rather than dropped silently.
    fn first_from_finder(&mut self) -> Option<PathBuf> {
        let mut paths = storyteller_macos_open::take().into_iter();
        let first = paths.next()?;
        let others: Vec<String> =
            paths.map(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| p.display().to_string())).collect();
        if !others.is_empty() {
            self.status = format!("One script at a time: opened the first. Not opened: {}.", others.join(", "));
        }
        Some(first)
    }

    /// The extension new scripts are saved with: the writer's choice of working format.
    fn preferred_extension(&self) -> &'static str {
        match self.prefs.get().general.new_script_format.as_str() {
            "osf" => "osf",
            "fountain" => "fountain",
            _ => "mstory",
        }
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
                FileFormat::Osf | FileFormat::FadeIn | FileFormat::Mstory => "file.export_osf",
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
                    let edits = self.session.edits();
                    let _ = storyteller_engine::commands::set_text(&mut self.session, index, &text, true);
                    self.session.caret = Caret { element: index, offset: caret };
                    // Typing outside the command registry: End of Act lines follow here too.
                    if self.session.edits() != edits {
                        self.session.keep_acts_in_step();
                    }
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
                EditAction::PasteScreenplay { index, start, end, text } => {
                    let request = params(&[("index", json!(index)), ("start", json!(start)), ("end", json!(end)), ("text", json!(text))]);
                    match self.registry.run("edit.paste_screenplay", &mut self.session, &request) {
                        Ok(outcome) => {
                            let n = outcome.value.get("elements").and_then(Value::as_u64).unwrap_or(0);
                            self.status = format!("Pasted {n} elements");
                            self.error = None;
                        }
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
        let Some(dir) = self.recovery_dir.clone() else { return };
        if let Ok(bytes) = files::encode(&self.session.script, FileFormat::Fountain, &self.session.layout) {
            let _ = crate::recovery::write(&dir, &self.recovery_name, &bytes);
        }
    }

    /// Looks for a recovery copy to offer; whether there is one.
    fn find_recovery(&mut self) -> bool {
        self.recovered = self.recovery_dir.as_deref().and_then(|dir| crate::recovery::waiting(dir).into_iter().next());
        self.recovered.is_some()
    }

    /// A recovery name for the open script: its file's name, or Untitled, never one a copy
    /// still waiting to be answered has.
    fn fresh_recovery_name(&self) -> String {
        let stem =
            self.session.path.as_ref().and_then(|p| p.file_stem()).map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "Untitled".into());
        match &self.recovery_dir {
            Some(dir) => crate::recovery::free_name(dir, &stem),
            None => stem,
        }
    }

    /// The open script's changes are deliberately thrown away: its copy goes.
    fn forget_recovery(&self) {
        if let Some(dir) = &self.recovery_dir {
            crate::recovery::forget(dir, &self.recovery_name);
        }
    }

    /// At launch, a recovery copy left by a launch that ended without saving: open it as an
    /// unsaved script, throw it away, or leave it for later.
    fn recovery_prompt(&mut self, ctx: &egui::Context) {
        let Some(copy) = self.recovered.clone() else { return };
        if self.prompt.is_some() {
            return;
        }
        let name = crate::recovery::name_of(&copy);
        let mut answer = None;
        egui::Modal::new(egui::Id::new("storyteller.recovery")).frame(theme::modal_frame(&ctx.global_style())).show(ctx, |ui| {
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
        // Open Recent, a submenu, straight after Open….
        if let Some(open) = items.iter().position(|e| e.id == "file.open") {
            let enabled = !self.prefs.get().general.recent.is_empty();
            items.insert(open + 1, MenuEntry { id: OPEN_RECENT.into(), label: "Open Recent".into(), shortcut: None, enabled });
        }
        items
    }

    /// File > Open Recent's items: (command id, label), most recent first.
    fn recent_items(&self) -> Vec<(String, String)> {
        self.prefs.get().general.recent.iter().enumerate().map(|(n, path)| (format!("{RECENT_PREFIX}{n}"), recent_label(path))).collect()
    }

    /// The checked state of a toggle command, or `None` for an ordinary one.
    fn checked(&self, id: &str) -> Option<bool> {
        match id {
            "view.navigator" => Some(self.prefs.get().view.navigator),
            "view.page_view" => Some(self.prefs.get().view.page_view),
            "view.scene_numbers" => Some(self.session.layout.show_scene_numbers),
            "prefs.new_mstory" => Some(self.preferred_extension() == "mstory"),
            "prefs.new_fountain" => Some(self.preferred_extension() == "fountain"),
            "prefs.new_osf" => Some(self.preferred_extension() == "osf"),
            "prefs.smart_quotes" => Some(self.prefs.get().general.smart_quotes),
            "app.characters" => Some(self.characters),
            "prefs.spelling_us" => Some(self.prefs.get().general.spelling_language != "en_GB"),
            "prefs.spelling_uk" => Some(self.prefs.get().general.spelling_language == "en_GB"),
            "prefs.check_spelling" => Some(self.prefs.get().view.check_spelling),
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
            native.set_recent(&self.recent_items());
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
                let recent = self.recent_items();
                let response = ui.menu_button(menu, |ui| {
                    for MenuEntry { id, label, shortcut, enabled } in &items {
                        if id == OPEN_RECENT {
                            let submenu = ui.add_enabled_ui(*enabled, |ui| {
                                ui.menu_button(label.as_str(), |ui| {
                                    for (id, label) in &recent {
                                        let item = ui.button(label.as_str());
                                        self.automation.add(format!("menu.item.{id}"), "menuitem", label.clone(), &item);
                                        if item.clicked() {
                                            clicked = Some(id.clone());
                                            ui.close();
                                        }
                                    }
                                    ui.separator();
                                    let clear = ui.button("Clear Menu");
                                    self.automation.add("menu.item.app.clear_recent", "menuitem", "Clear Menu", &clear);
                                    if clear.clicked() {
                                        clicked = Some("app.clear_recent".into());
                                        ui.close();
                                    }
                                })
                            });
                            self.automation.add(format!("menu.item.{id}"), "menu", label.clone(), &submenu.inner.response);
                            continue;
                        }
                        let mut button = egui::Button::new(label.as_str());
                        if let Some(shortcut) = shortcut.or(text_field_key(id)).and_then(parse_shortcut) {
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
        let title = self.display_title();
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
            let strong = ui.visuals().strong_text_color();
            let act_row = |ui: &mut egui::Ui, automation: &mut Automation, k: usize, name: &str| {
                let name = if name.is_empty() { "(untitled act)" } else { name };
                let (rect, response) = ui.allocate_exact_size(egui::vec2(ui.available_width(), theme::NAV_ROW_HEIGHT), egui::Sense::click());
                if response.hovered() {
                    ui.painter().rect_filled(rect, 5.0, hover_fill);
                }
                // Small capitals in the strong colour, like the SCENES label above.
                let font = egui::FontId::proportional(theme::TYPE_SMALL);
                ui.painter().with_clip_rect(rect).text(
                    rect.left_center() + egui::vec2(8.0, 0.0),
                    egui::Align2::LEFT_CENTER,
                    name.to_uppercase(),
                    font,
                    strong,
                );
                automation.add(format!("navigator.act.{k}"), "listitem", name.to_string(), &response);
                response.clicked()
            };
            for (n, scene) in scenes.iter().enumerate().filter(|_| !blank) {
                while let Some((at, name)) = acts.get(next_act) {
                    if *at > scene.element {
                        break;
                    }
                    if act_row(ui, &mut self.automation, next_act, name) {
                        act_clicked = Some(*at);
                    }
                    next_act += 1;
                }
                let number = scene.number.as_deref().unwrap_or("");
                let heading = if scene.heading.trim().is_empty() { "(untitled scene)" } else { scene.heading.as_str() };
                // A scene's own title stands in for its heading; its colour marks the row.
                let meta = self.session.script.elements.get(scene.element).map(|e| e.scene.clone()).unwrap_or_default();
                let shown = if meta.title.is_empty() { heading.to_string() } else { meta.title.clone() };
                // An omitted scene's row is muted: it is there, but not shot.
                let omitted = self.session.script.elements.get(scene.element).is_some_and(storyteller_core::Element::is_omitted);
                let text = if omitted { weak } else { text };
                let (rect, response) = ui.allocate_exact_size(egui::vec2(ui.available_width(), theme::NAV_ROW_HEIGHT), egui::Sense::click());
                let selected = n + 1 == current;
                if selected || response.hovered() {
                    ui.painter().rect_filled(rect, 5.0, if selected { selected_fill } else { hover_fill });
                }
                let painter = ui.painter().with_clip_rect(rect);
                if let Some((r, g, b)) = meta.rgb() {
                    let stripe = egui::Rect::from_min_size(rect.left_top() + egui::vec2(2.0, 4.0), egui::vec2(4.0, rect.height() - 8.0));
                    painter.rect_filled(stripe, 2.0, egui::Color32::from_rgb(r, g, b));
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
                if act_row(ui, &mut self.automation, k, name) {
                    act_clicked = Some(*at);
                }
            }
        });
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

    /// The scene the caret is in, counted from 1 (0 before the first heading): the row the
    /// navigator highlights.
    fn current_scene(&self) -> usize {
        self.session.script.elements.iter().take(self.session.caret.element + 1).filter(|e| e.kind == ElementKind::SceneHeading).count()
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
                    .chain(
                        [
                            ElementKind::Centered,
                            ElementKind::Lyrics,
                            ElementKind::NewAct,
                            ElementKind::EndOfAct,
                            ElementKind::Section,
                            ElementKind::Synopsis,
                        ]
                        .iter(),
                    )
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

    /// Production > Characters: every speaking character with their speeches, scenes, words
    /// and first scene. A name takes the caret to the character's first cue.
    fn characters_window(&mut self, ctx: &egui::Context) {
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

    /// Production > Number Scenes…: start number, keep existing (letters for new scenes), from
    /// the caret's scene on, and whether I and O are left out.
    fn numbering_dialog(&mut self, ctx: &egui::Context) {
        let Some(draft) = &mut self.numbering else { return };
        let mut ok = false;
        let mut close = ctx.input(|i| i.key_pressed(egui::Key::Escape));
        egui::Window::new("Number Scenes").collapsible(false).resizable(false).anchor(egui::Align2::CENTER_TOP, [0.0, 120.0]).show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label("Start at");
                let start = ui.add_enabled(!draft.keep_existing, egui::TextEdit::singleline(&mut draft.start).desired_width(60.0));
                self.automation.add("dialog.numbering.start", "textbox", "Start at", &start);
            });
            let keep = ui.checkbox(&mut draft.keep_existing, "Keep existing numbers (new scenes get letters, such as 12A)");
            self.automation.add("dialog.numbering.keep", "checkbox", "Keep existing numbers", &keep);
            let from = ui.checkbox(&mut draft.from_caret, "From the current scene on");
            self.automation.add("dialog.numbering.from_caret", "checkbox", "From the current scene on", &from);
            let skip = ui.checkbox(&mut draft.skip_io, "Leave out the letters I and O");
            self.automation.add("dialog.numbering.skip_io", "checkbox", "Leave out the letters I and O", &skip);
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
        self.prefs.update(|p| p.general.skip_io_letters = skip_io);
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

    /// Production > Revisions…: every set with its name, mark and page colour, the active one
    /// chosen, the next in the standard order added, and a set deleted (asking first when it
    /// marks anything). Each change is one Undo.
    fn revisions_window(&mut self, ctx: &egui::Context) {
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
                        let field = ui.add(egui::TextEdit::singleline(name).desired_width(220.0));
                        self.automation.add(format!("revisions.name.{}", set.id), "textbox", "Name", &field);
                        if field.lost_focus() && *name != set.name {
                            run.push(("revisions.update", vec![("id", json!(set.id)), ("name", json!(name.clone()))]));
                        }
                        let mark = draft.marks.entry(set.id).or_insert_with(|| set.mark.clone());
                        let field = ui.add(egui::TextEdit::singleline(mark).desired_width(28.0).char_limit(2));
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
                    storyteller_engine::CommandError::Invalid { reason, .. } => capitalise(&reason),
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
    fn scene_number_dialog(&mut self, ctx: &egui::Context) {
        let Some(draft) = &mut self.scene_number else { return };
        let heading = self.session.element(draft.heading).map(|e| e.text.clone()).unwrap_or_default();
        let mut ok = false;
        let mut close = ctx.input(|i| i.key_pressed(egui::Key::Escape));
        egui::Window::new("Edit Scene Number").collapsible(false).resizable(false).anchor(egui::Align2::CENTER_TOP, [0.0, 120.0]).show(ctx, |ui| {
            ui.label(egui::RichText::new(heading.as_str()).strong());
            let number = ui.add(egui::TextEdit::singleline(&mut draft.number).hint_text("Such as 12 or 12A; empty for none").desired_width(200.0));
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
                        storyteller_engine::CommandError::Invalid { reason, .. } => capitalise(&reason),
                        other => other.to_string(),
                    });
                }
            }
        }
    }

    /// Opens Scene Properties for the scene headed at `heading`.
    fn open_scene_dialog(&mut self, heading: usize) {
        let meta = self.session.script.elements.get(heading).map(|e| e.scene.clone()).unwrap_or_default();
        // A colour from a file in another shade is shown as the nearest of ours only when it is one.
        let color = meta.rgb().map(|(r, g, b)| format!("#{r:02X}{g:02X}{b:02X}"));
        self.scene_dialog = Some(SceneDraft { heading, title: meta.title, summary: meta.summary, color, focused: false });
    }

    /// Scene Properties: the scene's title, summary and colour, one Undo.
    fn scene_properties_dialog(&mut self, ctx: &egui::Context) {
        let Some(draft) = &mut self.scene_dialog else { return };
        let heading = self.session.script.elements.get(draft.heading).map(|e| e.text.clone()).unwrap_or_default();
        let mut ok = false;
        let mut close = ctx.input(|i| i.key_pressed(egui::Key::Escape));
        egui::Window::new("Scene Properties").collapsible(false).resizable(false).anchor(egui::Align2::CENTER_TOP, [0.0, 120.0]).show(ctx, |ui| {
            ui.label(egui::RichText::new(if heading.trim().is_empty() { "(untitled scene)" } else { heading.as_str() }).strong());
            ui.add_space(theme::SPACE_2);
            ui.label("Title");
            let title = ui.add(egui::TextEdit::singleline(&mut draft.title).hint_text("A few words for the navigator").desired_width(320.0));
            self.automation.add("dialog.scene.title", "textbox", "Title", &title);
            if !draft.focused {
                title.request_focus();
                draft.focused = true;
            }
            ui.label("Summary");
            let summary =
                ui.add(egui::TextEdit::multiline(&mut draft.summary).hint_text("What happens in the scene").desired_rows(3).desired_width(320.0));
            self.automation.add("dialog.scene.summary", "textbox", "Summary", &summary);
            ui.label("Colour");
            ui.horizontal(|ui| {
                let none = ui.selectable_label(draft.color.is_none(), "None");
                self.automation.add("dialog.scene.color.none", "radio", "None", &none);
                if none.clicked() {
                    draft.color = None;
                }
                for (name, hex) in SCENE_COLORS {
                    let fill = storyteller_core::model::SceneMeta { color: Some(hex.to_string()), ..Default::default() }
                        .rgb()
                        .map(|(r, g, b)| egui::Color32::from_rgb(r, g, b))
                        .unwrap_or_default();
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

    /// File > New TV Script: one-hour, half-hour or multi-camera, the last choice first.
    fn new_tv_dialog(&mut self, ctx: &egui::Context) {
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

    fn go_to_dialog(&mut self, ctx: &egui::Context) {
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
                if scene.clicked() || page.clicked() || act.clicked() {
                    draft.refocus = true;
                    ui.ctx().request_repaint();
                }
            });
            let hint = match draft.mode {
                GoToMode::Scene => "Scene number, such as 12 or 12A",
                GoToMode::Page => "Page number",
                GoToMode::Act => "Act, such as 2, TAG or ACT TWO",
            };
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
        self.update_spelling_marks();
        let actions = egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(ui.visuals().extreme_bg_color))
            .show(ui, |ui| editor::show(ui, &mut self.session, &mut self.editor, &mut self.automation, zoom, pages))
            .inner;
        self.apply(actions);
        self.title_page_dialog(&ctx);
        self.go_to_dialog(&ctx);
        self.new_tv_dialog(&ctx);
        self.spelling_dialog(&ctx);
        self.rename_dialog(&ctx);
        self.scene_properties_dialog(&ctx);
        self.characters_window(&ctx);
        self.numbering_dialog(&ctx);
        self.revisions_window(&ctx);
        self.scene_number_dialog(&ctx);
        self.help_windows(&ctx);
        self.unsaved_prompt(&ctx);
        self.recovery_prompt(&ctx);
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

impl eframe::App for StorytellerApp {
    fn raw_input_hook(&mut self, ctx: &egui::Context, raw_input: &mut egui::RawInput) {
        if let Some(batch) = self.automation.injected.pop_front() {
            raw_input.events.extend(batch);
            ctx.request_repaint();
        }
    }

    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if let Some(focus) = self.focus_next_frame.take() {
            self.editor.pending_focus = Some(focus);
            ctx.request_repaint();
        }
        // Edit > Cut and Copy, chosen last frame: the field has its focus back now.
        let input = std::mem::take(&mut self.next_frame_input);
        if !input.is_empty() {
            ctx.input_mut(|i| i.events.extend(input));
            ctx.request_repaint();
        }
        self.handle_screenshot(ctx);
        self.handle_control(ctx);
        self.sync_native_menu();
        self.open_from_finder(ctx);
        if self.prompt.is_none()
            && self.title_dialog.is_none()
            && self.go_to.is_none()
            && self.rename.is_none()
            && self.spelling.is_none()
            && self.new_tv.is_none()
        {
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
        let title = format!("{} — {APP_NAME}", self.display_title());
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
#[path = "sample.rs"]
mod sample;
pub use sample::StartFlags;

#[path = "spelling.rs"]
mod spelling;

#[cfg(test)]
#[path = "screens.rs"]
mod screens;

#[cfg(test)]
#[path = "ui_tests.rs"]
mod ui_tests;
