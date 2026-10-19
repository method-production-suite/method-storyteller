//! An open document: the script, its file, the caret and undo history.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use storyteller_core::layout::Layout;
use storyteller_core::paginate::{Paginated, SceneLength, paginate, scene_lengths};
use storyteller_core::{Element, ElementKind, Script};

use crate::files::FileFormat;

/// Undo steps kept.
pub const MAX_UNDO: usize = 500;

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
    paginated: Option<Paginated>,
    lengths: Option<Vec<SceneLength>>,
    /// The writer's learned words and this session's ignored ones (see [`crate::spell`]).
    pub spelling: crate::spell::Personal,
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
            format: FileFormat::Fountain,
            dirty: false,
            caret: Caret::default(),
            layout,
            undo: Vec::new(),
            redo: Vec::new(),
            last_edit: None,
            paginated: None,
            lengths: None,
            spelling: crate::spell::Personal::default(),
        }
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
        self.dirty = true;
        self.paginated = None;
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
        self.paginated = None;
        self.lengths = None;
        self.clamp_caret();
    }

    /// Forget history (after opening or creating a document).
    pub fn reset_history(&mut self) {
        self.undo.clear();
        self.redo.clear();
        self.last_edit = None;
        self.paginated = None;
        self.lengths = None;
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

    /// Each scene's page and length in eighths of a page, cached like the pagination.
    pub fn scene_lengths(&mut self) -> &[SceneLength] {
        if self.lengths.is_none() {
            self.paginated();
            let (script, layout) = (&self.script, &self.layout);
            self.lengths = self.paginated.as_ref().map(|paginated| scene_lengths(script, paginated, layout));
        }
        self.lengths.as_deref().unwrap_or_default()
    }

    /// Drop the cached pagination (after a layout change).
    pub fn invalidate_layout(&mut self) {
        self.paginated = None;
        self.lengths = None;
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
