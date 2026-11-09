//! An open document: the script, its file, the caret and undo history.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use storyteller_core::layout::Layout;
use storyteller_core::paginate::{Paginated, SceneLength, paginate, scene_lengths};
use storyteller_core::{Element, ElementKind, Script};

use crate::files::FileFormat;

/// Undo steps kept.
pub const MAX_UNDO: usize = 500;

/// Nothing laid out, for [`Session::shown_pages`] when there is nothing to show.
static EMPTY: Paginated = Paginated { title_page: None, pages: Vec::new(), scenes: Vec::new(), element_pages: Vec::new() };

/// Caret position: element index and byte offset into its text (always on a char boundary).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Caret {
    pub element: usize,
    pub offset: usize,
}

#[derive(Clone, Debug)]
struct Snapshot {
    script: Script,
    caret: Caret,
}

/// What kind of edit an undo step records; consecutive typing in one element merges.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditKind {
    Typing(usize),
    Other,
}

#[derive(Debug)]
pub struct Session {
    pub script: Script,
    pub path: Option<PathBuf>,
    pub format: FileFormat,
    pub dirty: bool,
    pub caret: Caret,
    pub layout: Layout,
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
    last_edit: Option<EditKind>,
    /// Counts checkpoints, so a command that edited the script can be told from one that
    /// did not.
    edits: u64,
    paginated: Option<Paginated>,
    lengths: Option<Vec<SceneLength>>,
    /// The pages and scene lengths from before the typing now going on, which the screen keeps
    /// drawing while [`Session::hold_pages`] is set (see [`Session::shown_pages`]).
    held: Option<(Paginated, Vec<SceneLength>)>,
    /// Set by the app while the writer is typing: the screen draws the held pages rather than
    /// laying the whole script out again on every key. Nothing else uses the held pages.
    pub hold_pages: bool,
    /// The writer's learned words (see [`crate::spell::Personal`]); the app fills them from its settings.
    pub spelling: crate::spell::Personal,
    /// View > Show Revisions; the app keeps it in its settings (§3.5).
    pub show_revisions: storyteller_core::model::ShowRevisions,
    /// View > Revised Page Colours: revised pages on screen in their set's page colour.
    pub page_colours: bool,
    /// Tab after text in a character cue opens the extension list (decision 3), rather than
    /// starting a parenthetical. The app keeps it in its settings.
    pub tab_opens_extensions: bool,
    /// View > Show Outline: sections and synopses on screen. They are never printed.
    pub show_outline: bool,
    /// The last range copied, as text and as elements: pasting that same text back gives these.
    copied: Option<(String, Vec<Element>)>,
}

impl Default for Session {
    fn default() -> Self {
        Session::new(Script::blank())
    }
}

impl Session {
    pub fn new(mut script: Script) -> Self {
        if script.elements.is_empty() {
            script.elements.push(Element::new(ElementKind::SceneHeading, ""));
        }
        let layout = Layout::for_script(&script);
        Session {
            script,
            path: None,
            // New scripts are Open Screenplay Format, the standard; Fountain is one choice away.
            format: FileFormat::Osf,
            dirty: false,
            caret: Caret::default(),
            layout,
            undo: Vec::new(),
            redo: Vec::new(),
            last_edit: None,
            edits: 0,
            paginated: None,
            lengths: None,
            held: None,
            hold_pages: false,
            spelling: crate::spell::Personal::default(),
            show_revisions: Default::default(),
            page_colours: false,
            tab_opens_extensions: false,
            show_outline: true,
            copied: None,
        }
    }

    /// A script being saved after edits keeps what it now uses in its lists. An unedited one is
    /// written back exactly as it came.
    pub fn learn_before_saving(&mut self) {
        if self.dirty {
            let mut lists = std::mem::take(&mut self.script.lists);
            lists.learn(&self.script);
            self.script.lists = lists;
        }
    }

    /// Whether element `index` is drawn: an outline paragraph is not while outlines are hidden.
    pub fn is_shown(&self, index: usize) -> bool {
        self.show_outline || !self.script.elements.get(index).is_some_and(|e| matches!(e.kind, ElementKind::Section | ElementKind::Synopsis))
    }

    /// The nearest element drawn from `index`, looking forward first, or back if none follows.
    pub fn nearest_shown(&self, index: usize) -> usize {
        let count = self.script.elements.len();
        (index..count).find(|&i| self.is_shown(i)).or_else(|| (0..index.min(count)).rfind(|&i| self.is_shown(i))).unwrap_or(index)
    }

    /// Remembers a range copied to the clipboard as `text`.
    pub fn remember_copied(&mut self, text: String, elements: Vec<Element>) {
        self.copied = Some((text, elements));
    }

    /// The elements of the last range copied, when `text` is what it put on the clipboard.
    pub fn copied_elements(&self, text: &str) -> Option<&[Element]> {
        self.copied.as_ref().filter(|(copied, _)| copied.replace("\r\n", "\n") == text.replace("\r\n", "\n")).map(|(_, elements)| elements.as_slice())
    }

    /// Whether pasting `text` makes elements rather than text in one paragraph.
    pub fn pastes_as_elements(&self, text: &str) -> bool {
        crate::commands::is_screenplay_paste(text) || self.copied_elements(text).is_some_and(|elements| elements.len() > 1)
    }

    /// Record an undo step before an edit. Typing in the same element merges into one step.
    pub fn checkpoint(&mut self, kind: EditKind) {
        let merge = matches!(kind, EditKind::Typing(_)) && self.last_edit == Some(kind);
        if !merge {
            self.undo.push(Snapshot { script: self.script.clone(), caret: self.caret });
            if self.undo.len() > MAX_UNDO {
                self.undo.remove(0);
            }
        }
        self.redo.clear();
        self.last_edit = Some(kind);
        self.edits += 1;
        self.dirty = true;
        // Typing keeps the pages it started from for the screen; any other edit drops them.
        // This runs before the edit, so the pages still match the script.
        match (kind, self.paginated.take()) {
            (EditKind::Typing(_), Some(pages)) => {
                let lengths = self.lengths.take().unwrap_or_else(|| scene_lengths(&self.script, &pages, &self.layout));
                self.held = Some((pages, lengths));
            }
            (EditKind::Typing(_), None) => {}
            (EditKind::Other, _) => self.held = None,
        }
        self.lengths = None;
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    pub fn undo(&mut self) -> bool {
        let Some(snapshot) = self.undo.pop() else { return false };
        self.redo.push(Snapshot { script: std::mem::replace(&mut self.script, snapshot.script), caret: self.caret });
        self.caret = snapshot.caret;
        self.after_history();
        true
    }

    pub fn redo(&mut self) -> bool {
        let Some(snapshot) = self.redo.pop() else { return false };
        self.undo.push(Snapshot { script: std::mem::replace(&mut self.script, snapshot.script), caret: self.caret });
        self.caret = snapshot.caret;
        self.after_history();
        true
    }

    fn after_history(&mut self) {
        self.last_edit = None;
        self.dirty = true;
        self.invalidate_layout();
        self.clamp_caret();
    }

    /// Forget history (after opening or creating a document).
    pub fn reset_history(&mut self) {
        self.undo.clear();
        self.redo.clear();
        self.last_edit = None;
        self.invalidate_layout();
    }

    /// Mark the end of a typing run so the next keystroke starts a new undo step.
    pub fn break_typing(&mut self) {
        self.last_edit = None;
    }

    /// Keep the caret on an existing element and a char boundary.
    pub fn clamp_caret(&mut self) {
        if self.script.elements.is_empty() {
            self.script.elements.push(Element::new(ElementKind::SceneHeading, ""));
        }
        let last = self.script.elements.len() - 1;
        self.caret.element = self.caret.element.min(last);
        let text = self.script.elements.get(self.caret.element).map_or("", |e| e.text.as_str());
        self.caret.offset = floor_char_boundary(text, self.caret.offset);
    }

    pub fn element(&self, index: usize) -> Option<&Element> {
        self.script.elements.get(index)
    }

    pub fn current(&self) -> Option<&Element> {
        self.element(self.caret.element)
    }

    /// Pagination of the current script (cached until the next edit).
    pub fn paginated(&mut self) -> &Paginated {
        let script = &self.script;
        let layout = &self.layout;
        self.paginated.get_or_insert_with(|| paginate(script, layout))
    }

    /// The script with its cached pagination and scene lengths, borrowed together.
    pub(crate) fn with_pages(&mut self) -> (&Script, &Paginated, &[SceneLength]) {
        self.scene_lengths();
        let paginated = self.paginated.get_or_insert_with(|| paginate(&self.script, &self.layout));
        (&self.script, paginated, self.lengths.as_deref().unwrap_or_default())
    }

    /// Each scene's page and length in eighths of a page, cached like the pagination.
    pub fn scene_lengths(&mut self) -> &[SceneLength] {
        if self.lengths.is_none() {
            self.paginated();
            let (script, layout) = (&self.script, &self.layout);
            self.lengths = self.paginated.as_ref().map(|paginated| scene_lengths(script, paginated, layout));
        }
        self.lengths.as_deref().unwrap_or_default()
    }

    /// Whether the screen draws the held pages: the writer is typing, the script has not been
    /// laid out since, and the held pages still have a place for every paragraph.
    fn showing_held(&self) -> bool {
        self.hold_pages
            && self.paginated.is_none()
            && self.held.as_ref().is_some_and(|(pages, _)| pages.element_pages.len() == self.script.elements.len())
    }

    /// The pages to draw on screen. While the writer types in a paragraph ([`Session::hold_pages`]),
    /// these are the pages from before the typing began, so a key does not lay the whole script
    /// out again; the app clears `hold_pages` after a pause and the pages catch up. Saving,
    /// exporting, reports and Lock Pages use [`Session::paginated`], which is always current.
    pub fn shown_pages(&mut self) -> &Paginated {
        if !self.showing_held() {
            self.paginated();
        }
        self.paginated.as_ref().or(self.held.as_ref().map(|(pages, _)| pages)).unwrap_or(&EMPTY)
    }

    /// The scene lengths to draw on screen, held like [`Session::shown_pages`].
    pub fn shown_scene_lengths(&mut self) -> &[SceneLength] {
        if self.showing_held() {
            return self.held.as_ref().map_or(&[], |(_, lengths)| lengths.as_slice());
        }
        self.scene_lengths()
    }

    /// The set a change is marked in now: the active one, in revision mode (kept in the script,
    /// so a file saved in revision mode opens in it).
    pub fn marking_set(&self) -> Option<u32> {
        if self.script.revisions.mode { self.script.revisions.active } else { None }
    }

    /// How many edits the session has recorded (see [`Session::checkpoint`]).
    pub fn edits(&self) -> u64 {
        self.edits
    }

    /// After an edit: End of Act lines follow their acts' names, an act whose line the caret
    /// is on gets its End of Act line when it has none, and a lettered script's scenes keep
    /// their letters in order (docs/spec.md §3).
    pub fn keep_acts_in_step(&mut self) {
        let on_act = self.current().is_some_and(|e| e.kind == ElementKind::NewAct).then_some(self.caret.element);
        let (acts_changed, added) = self.script.keep_acts_in_step(on_act);
        // A lettered script's scenes keep their letters in order.
        let changed = self.script.keep_letters_in_step() || acts_changed;
        if let Some(at) = added
            && self.caret.element >= at
        {
            self.caret.element += 1;
        }
        if changed {
            self.invalidate_layout();
        }
    }

    /// Drop the cached pagination (after a layout change).
    pub fn invalidate_layout(&mut self) {
        self.paginated = None;
        self.lengths = None;
        self.held = None;
    }

    /// Window or tab title: file name and an unsaved marker.
    pub fn title(&self) -> String {
        let name = self.path.as_ref().and_then(|p| p.file_name()).map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "Untitled".into());
        if self.dirty { format!("{name} — edited") } else { name }
    }
}

/// The largest char boundary at or below `offset`.
pub fn floor_char_boundary(text: &str, offset: usize) -> usize {
    let mut offset = offset.min(text.len());
    while offset > 0 && !text.is_char_boundary(offset) {
        offset -= 1;
    }
    offset
}
