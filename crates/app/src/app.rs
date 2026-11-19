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
/// How long typing pauses before the screen lays the script out again. Until then a key in a
/// long script does not wait on the whole script's pagination.
const SETTLE: f64 = 0.5;
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

const APP_COMMANDS: [AppCommand; 51] = [
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
    AppCommand { id: "app.lists", label: "Autocomplete Lists…", menu: "Production/Autocomplete Lists…", shortcut: None },
    AppCommand {
        id: "prefs.tab_extensions",
        label: "Tab After a Name Opens Extensions",
        menu: "Production/Tab After a Name Opens Extensions",
        shortcut: None,
    },
    AppCommand { id: "app.reports", label: "Reports…", menu: "Production/Reports…", shortcut: None },
    AppCommand { id: "app.edit_scene_number", label: "Edit Scene Number…", menu: "Production/Edit Scene Number…", shortcut: None },
    AppCommand { id: "app.revisions", label: "Revisions…", menu: "Production/Revisions…", shortcut: None },
    AppCommand { id: "app.mark_revised", label: "Mark Revised", menu: "Production/Mark Revised", shortcut: Some("Cmd+CloseBracket") },
    AppCommand { id: "app.clear_revised", label: "Clear Revised", menu: "Production/Clear Revised", shortcut: Some("Cmd+OpenBracket") },
    AppCommand { id: "app.go_to", label: "Go To…", menu: "Edit/Go To…", shortcut: Some(GO_TO_KEY) },
    AppCommand { id: "app.add_bookmark", label: "Add Bookmark…", menu: "Edit/Add Bookmark…", shortcut: None },
    AppCommand { id: "prefs.smart_quotes", label: "Smart Quotes", menu: "Format/Smart Quotes", shortcut: None },
    AppCommand { id: "app.bold", label: "Bold", menu: "Format/Bold", shortcut: Some("Cmd+B") },
    AppCommand { id: "app.italic", label: "Italic", menu: "Format/Italic", shortcut: Some("Cmd+I") },
    AppCommand { id: "app.underline", label: "Underline", menu: "Format/Underline", shortcut: Some("Cmd+U") },
    AppCommand { id: "view.navigator", label: "Scene Navigator", menu: "View/Scene Navigator", shortcut: Some("Cmd+Shift+N") },
    AppCommand { id: "view.page_view", label: "Page View", menu: "View/Page View", shortcut: Some("Cmd+Shift+P") },
    AppCommand { id: "view.formatting_marks", label: "Show Formatting Marks", menu: "View/Show Formatting Marks", shortcut: None },
    AppCommand { id: "view.revisions_all", label: "Show All Revision Sets", menu: "View/Show All Revision Sets", shortcut: None },
    AppCommand { id: "view.revisions_active", label: "Show Active Revision Set", menu: "View/Show Active Revision Set", shortcut: None },
    AppCommand { id: "view.revisions_none", label: "Hide Revisions", menu: "View/Hide Revisions", shortcut: None },
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

/// Production > Lock Pages and Unlock All Pages ask first (docs/production-spec.md §4.2).
enum PagesConfirm {
    /// Lock the pages, turning on revision mode with them when asked to.
    Lock {
        revision_mode: bool,
    },
    UnlockAll,
}

/// Production > Number Scenes….
struct NumberingDraft {
    start: String,
    keep_existing: bool,
    from_caret: bool,
    skip_io: bool,
    /// Print the numbers in the left and the right margin (a script setting).
    left: bool,
    right: bool,
    error: Option<String>,
}

/// File > Export PDF…: which pages to issue, and how (docs/production-spec.md §3.6).
#[derive(Default)]
struct ExportDraft {
    which: ExportPages,
    pages: String,
    scenes: String,
    clean: bool,
    colour: bool,
    error: Option<String>,
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum ExportPages {
    #[default]
    All,
    Revised,
    Pages,
    Scenes,
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
    Bookmark,
}

/// Production > Autocomplete Lists…: the list shown and the entry being typed.
#[derive(Default)]
struct ListsDraft {
    tab: usize,
    entry: String,
}

/// Edit > Add Bookmark…: the name being typed.
struct BookmarkDraft {
    name: String,
    focused: bool,
    error: Option<String>,
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
    /// File > Open Recent, kept in `recent.json` for Method Launcher.
    recent: crate::recent::Recent,
    editor: EditorState,
    automation: Automation,
    control: Option<Receiver<ControlRequest>>,
    pending_screenshot: Option<(Value, Sender<String>, PathBuf)>,
    /// Where the window's screenshot lands when it is ready (egui hands it to a callback).
    screenshot: std::sync::Arc<std::sync::Mutex<Option<std::sync::Arc<egui::ColorImage>>>>,
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
    bookmark: Option<BookmarkDraft>,
    lists: Option<ListsDraft>,
    /// File > New TV Script: the shape chosen so far.
    new_tv: Option<storyteller_core::model::TvShape>,
    rename: Option<RenameDraft>,
    scene_dialog: Option<SceneDraft>,
    /// Production > Characters: the character table is open.
    characters: bool,
    /// Production > Reports…: the report the window shows, while it is open.
    reports: Option<&'static str>,
    numbering: Option<NumberingDraft>,
    pages_confirm: Option<PagesConfirm>,
    export_pdf: Option<ExportDraft>,
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
    /// The scene (its heading) being dragged in the navigator.
    dragging_scene: Option<usize>,
    /// A caret to give the keyboard to on the next frame (after a click outside the editor).
    focus_next_frame: Option<(usize, usize)>,
    /// The session's edit count when this frame last saw it change, and the time it did: the
    /// screen keeps its pages while edits come within [`SETTLE`] of each other.
    last_edit_seen: (u64, f64),
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
            let region = if in_memory { None } else { storyteller_platform::region() };
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
            recent: crate::recent::Recent::open(
                if in_memory { None } else { craft_settings::paths::data_dir(APP_ID) }.as_deref(),
                &prefs.get().general.recent,
            ),
            prefs,
            editor: EditorState::default(),
            automation: Automation::default(),
            control: None,
            pending_screenshot: None,
            screenshot: std::sync::Arc::default(),
            queued: Vec::new(),
            prompt: None,
            discard_confirmed: false,
            recovery_dir: if in_memory { None } else { craft_settings::paths::data_dir(APP_ID).map(|d| d.join(RECOVERY_DIR)) },
            recovered: None,
            recovery_name: String::new(),
            title_dialog: None,
            go_to: None,
            bookmark: None,
            lists: None,
            new_tv: None,
            rename: None,
            scene_dialog: None,
            characters: false,
            reports: None,
            numbering: None,
            pages_confirm: None,
            export_pdf: None,
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
            dragging_scene: None,
            focus_next_frame: None,
            last_edit_seen: (0, 0.0),
            #[cfg(test)]
            pictures: false,
        };
        let last_seen = app.prefs.get().general.last_seen_version.clone();
        app.help.whats_new = help::whats_new_due(&last_seen, help::VERSION);
        if help::record_version(&last_seen, help::VERSION) {
            app.prefs.update(|p| p.general.last_seen_version = help::VERSION.to_string());
        }
        app.view_into_session();
        let ctx = cc.egui_ctx.clone();
        storyteller_platform::set_wake(move || ctx.request_repaint());
        // What to open (sample::launch_choice). The first launch retires the sample whatever
        // opens: a writer who starts with their own script never sees it unasked.
        let from_finder = app.first_from_finder();
        let general = &app.prefs.get().general;
        let first_launch = !general.sample_seen;
        let has_recent = !app.recent.is_empty();
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
            "file.export_pdf" => self.export_pdf = Some(ExportDraft::default()),
            "file.export_fdx" => self.export(FileFormat::Fdx),
            "file.export_fountain" => self.export(FileFormat::Fountain),
            "file.export_osf" => self.export(FileFormat::Osf),
            "file.export_text" => self.export_text(),
            "file.export_scenes" => self.export_scenes(),
            "app.title_page" => {
                let page = &self.session.script.title_page;
                let fields =
                    storyteller_core::TitlePage::STANDARD_KEYS.iter().map(|k| (k.to_string(), page.get(k).unwrap_or("").to_string())).collect();
                self.title_dialog = Some(TitleDraft { fields });
            }
            "app.go_to" => self.go_to = Some(GoToDraft::default()),
            "app.add_bookmark" => {
                self.session.clamp_caret();
                let name = self.session.element(self.session.caret.element).and_then(|e| e.bookmark.clone()).unwrap_or_default();
                self.bookmark = Some(BookmarkDraft { name, focused: false, error: None });
            }
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
            "view.formatting_marks" => {
                self.prefs.update(|p| p.view.formatting_marks = !p.view.formatting_marks);
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
            "view.outline" => {
                self.run(id, Map::new());
                let show = self.session.show_outline;
                self.prefs.update(|p| p.view.outline = show);
                self.refocus();
            }
            "view.page_colours" => {
                self.run(id, Map::new());
                let show = self.session.page_colours;
                self.prefs.update(|p| p.view.page_colours = show);
            }
            "view.revisions_all" | "view.revisions_active" | "view.revisions_none" => {
                let show = id.trim_start_matches("view.revisions_");
                let mut params = Map::new();
                params.insert("show".into(), json!(show));
                self.run("view.show_revisions", params);
                let show = self.session.show_revisions;
                self.prefs.update(|p| p.view.revisions = show);
            }
            "edit.copy_scene" => self.copy_scene(ctx, None),
            // File > Close: the script closes and an empty one takes its place (decision 15).
            "app.close" => self.guard_unsaved(ctx, Pending::New),
            "app.characters" => self.characters = !self.characters,
            "app.lists" => self.lists = Some(ListsDraft::default()),
            "prefs.tab_extensions" => {
                self.prefs.update(|p| p.general.tab_opens_extensions = !p.general.tab_opens_extensions);
                self.session.tab_opens_extensions = self.prefs.get().general.tab_opens_extensions;
            }
            "app.reports" => self.reports = Some(self.reports.unwrap_or("scenes")),
            "app.revisions" => self.revisions = Some(RevisionsDraft::default()),
            "app.mark_revised" => self.mark_selection(ctx, "revisions.mark"),
            "app.clear_revised" => self.mark_selection(ctx, "revisions.clear"),
            // A locked script is a production draft: offer revision mode with the locks.
            "pages.lock" => self.pages_confirm = Some(PagesConfirm::Lock { revision_mode: !self.session.script.revisions.mode }),
            "pages.unlock_all" => self.pages_confirm = Some(PagesConfirm::UnlockAll),
            "scene.number" => {
                let numbered = self.session.script.scene_indices().iter().any(|&h| self.session.element(h).is_some_and(|e| e.scene_number.is_some()));
                self.numbering = Some(NumberingDraft {
                    start: "1".into(),
                    keep_existing: numbered,
                    from_caret: false,
                    skip_io: self.prefs.get().general.skip_io_letters,
                    left: self.session.script.scene_number_margins().0,
                    right: self.session.script.scene_number_margins().1,
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
                self.recent.clear();
                self.status = "Cleared the list of recent scripts.".into();
            }
            recent if recent.starts_with(RECENT_PREFIX) => {
                let path =
                    recent.strip_prefix(RECENT_PREFIX).and_then(|n| n.parse::<usize>().ok()).and_then(|n| self.recent.get(n).map(str::to_string));
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
        let id = egui::Id::unique(("storyteller.element", index));
        if self.editor.cursor.is_none_or(|c| c.index != index) {
            self.editor.cursor = Some(editor::Cursor::at(index, self.session.caret.offset));
        }
        ctx.memory_mut(|m| m.request_focus(id));
        id
    }

    /// Format > Bold, Italic and Underline: on the selection in the caret's paragraph, or on the
    /// word the caret is in; off again when all of it has the style already. The text stays
    /// selected.
    fn style_selection(&mut self, ctx: &egui::Context, style: &str) {
        if self.editor.span.is_some() {
            self.status = crate::messages::WITHIN_ONE_PARAGRAPH.to_string();
            return;
        }
        self.session.clamp_caret();
        let index = self.session.caret.element;
        let Some(text) = self.session.element(index).map(|e| e.text.clone()) else { return };
        let chars: Vec<char> = text.chars().collect();
        let (mut from, mut to) = match self.editor.cursor.filter(|c| c.index == index) {
            Some(cursor) => {
                let (a, b) = cursor.ordered();
                (crate::editor::byte_to_char(&text, a), crate::editor::byte_to_char(&text, b))
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
                let _ = &text;
                self.editor.cursor = Some(editor::Cursor { index, anchor: byte("start"), head: byte("end") });
                self.focus_caret_field(ctx);
                ctx.request_repaint();
            }
            Err(error) => {
                self.status = match error {
                    storyteller_engine::CommandError::Invalid { reason, .. } => sentence(&reason),
                    other => other.to_string(),
                };
                self.refocus();
            }
        }
    }

    /// Production > Mark Revised and Clear Revised (`command`): the selection in the caret's
    /// paragraph, or the whole paragraph when nothing is selected.
    fn mark_selection(&mut self, ctx: &egui::Context, command: &str) {
        if self.editor.span.is_some() {
            self.status = crate::messages::WITHIN_ONE_PARAGRAPH.to_string();
            return;
        }
        self.session.clamp_caret();
        let index = self.session.caret.element;
        let Some(text) = self.session.element(index).map(|e| e.text.clone()) else { return };
        let mut target = vec![("index", json!(index))];
        if let Some(cursor) = self.editor.cursor.filter(|c| c.index == index && c.anchor != c.head) {
            let (a, b) = cursor.ordered();
            target.push(("start", json!(a.min(text.len()))));
            target.push(("end", json!(b.min(text.len()))));
        }
        if let Err(error) = self.registry.run(command, &mut self.session, &params(&target)) {
            self.status = match error {
                storyteller_engine::CommandError::Invalid { reason, .. } => sentence(&reason),
                other => other.to_string(),
            };
        }
        self.focus_caret_field(ctx);
        ctx.request_repaint();
    }

    /// Edit > Select All: the whole script, as ⌘A selects it.
    fn select_all(&mut self, ctx: &egui::Context) {
        self.focus_caret_field(ctx);
        if self.editor.select_whole_script(&self.session) {
            return;
        }
        let index = self.session.caret.element;
        let Some(text) = self.session.element(index).map(|e| e.text.clone()) else { return };
        self.editor.cursor = Some(editor::Cursor { index, anchor: 0, head: text.len() });
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
                    rfd::FileDialog::new()
                        .add_filter("Screenplay", &["mstory", "fountain", "spmd", "txt", "osf", "fadein", "fdx", "highland"])
                        .pick_file()
                });
                if let Some(path) = path {
                    self.open_path(&path);
                }
            }
        }
    }

    fn apply(&mut self, ctx: &egui::Context, actions: Vec<EditAction>) {
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
                EditAction::CopyRange { from, to } => match self.registry.run("edit.copy_range", &mut self.session, &range_params(from, to)) {
                    Ok(outcome) => {
                        if let Some(text) = outcome.value.get("text").and_then(Value::as_str) {
                            ctx.copy_text(text.to_string());
                        }
                        self.error = None;
                    }
                    Err(error) => self.error = Some(error.to_string()),
                },
                EditAction::ReplaceRange { from, to, text } => {
                    let mut request = range_params(from, to);
                    request.insert("text".into(), json!(text));
                    self.run("edit.replace_range", request);
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

    // -----------------------------------------------------------------------------------------
    // Drawing

    /// The bar along the top: on macOS it is the window's title bar (drag to move, double-click
    /// to zoom), beside the traffic lights; elsewhere a toolbar under the menus.
    fn top_bar(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let rect = ui.max_rect();
        let title_bar = cfg!(target_os = "macos");
        if title_bar {
            let background = ui.interact(rect, egui::Id::unique("storyteller.title_bar"), egui::Sense::click_and_drag());
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

    /// While edits come close together, the screen keeps the pages it has (the session holds
    /// them only through typing in one paragraph); once they pause for [`SETTLE`], it lays the
    /// script out again.
    fn hold_pages_while_typing(&mut self, ctx: &egui::Context) {
        let now = ctx.input(|i| i.time);
        let edits = self.session.edits();
        if edits != self.last_edit_seen.0 {
            self.last_edit_seen = (edits, now);
        }
        let left = SETTLE - (now - self.last_edit_seen.1);
        self.session.hold_pages = left > 0.0;
        if left > 0.0 {
            ctx.request_repaint_after(Duration::from_secs_f64(left));
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
        let pages = self.session.shown_pages().page_count().max(1);
        let page = self.session.shown_pages().element_pages.get(caret.element).copied().unwrap_or(1).max(1);
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
            // Nothing visible names the picker, so a screen reader is told what it chooses.
            let _ = response.response.clone().accessible_name("Element type");
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

    fn draw(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        self.hold_pages_while_typing(&ctx);
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
            .show(ui, |ui| {
                self.editor.show_markup = self.prefs.get().view.formatting_marks;
                editor::show(ui, &mut self.session, &mut self.editor, &mut self.automation, zoom, pages)
            })
            .inner;
        self.apply(ui.ctx(), actions);
        self.title_page_dialog(&ctx);
        self.go_to_dialog(&ctx);
        self.bookmark_dialog(&ctx);
        self.lists_window(&ctx);
        self.new_tv_dialog(&ctx);
        self.spelling_dialog(&ctx);
        self.rename_dialog(&ctx);
        self.scene_properties_dialog(&ctx);
        self.characters_window(&ctx);
        self.reports_window(&ctx);
        self.numbering_dialog(&ctx);
        self.pages_confirm_dialog(&ctx);
        self.export_pdf_dialog(&ctx);
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

/// A message from the engine as a sentence: `there is no page 9` reads `There is no page 9.`
fn sentence(text: &str) -> String {
    let mut chars = text.chars();
    let first: String = chars.next().map(|c| c.to_uppercase().collect()).unwrap_or_default();
    format!("{first}{}.", chars.as_str())
}

fn params(pairs: &[(&str, Value)]) -> Map<String, Value> {
    pairs.iter().map(|(k, v)| (k.to_string(), v.clone())).collect()
}

fn range_params(from: Caret, to: Caret) -> Map<String, Value> {
    params(&[
        ("from_index", json!(from.element)),
        ("from_offset", json!(from.offset)),
        ("to_index", json!(to.element)),
        ("to_offset", json!(to.offset)),
    ])
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

#[path = "control_host.rs"]
mod control_host;
#[path = "dialogs.rs"]
mod dialogs;
#[path = "documents.rs"]
mod documents;
#[path = "find_bar.rs"]
mod find_bar;
#[path = "menus.rs"]
mod menus;
#[path = "navigator.rs"]
mod navigator;
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
