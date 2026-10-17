//! Every action is a named command: id, label, menu path, shortcut, typed params, `enabled`
//! and `run`. Menus, keyboard shortcuts, the CLI and the control channel all dispatch by id.

use std::path::PathBuf;

use serde::Serialize;
use serde_json::{Map, Value, json};
use storyteller_core::{Element, ElementKind, Script, autocomplete};

use crate::files::{self, FileFormat};
use crate::flow;
use crate::session::{Caret, EditKind, Session, floor_char_boundary};

/// Command parameters, as JSON.
pub type Params = Map<String, Value>;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum CommandError {
    #[error("unknown command `{0}`")]
    Unknown(String),
    #[error("`{0}` is not available right now")]
    Disabled(String),
    #[error("missing parameter `{0}`")]
    Missing(&'static str),
    #[error("parameter `{name}` is invalid: {reason}")]
    Invalid { name: &'static str, reason: String },
    #[error("{0}")]
    Failed(String),
}

/// What a command reports back.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct Outcome {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Value::is_null")]
    pub value: Value,
}

impl Outcome {
    fn done() -> Self {
        Outcome::default()
    }

    fn message(text: impl Into<String>) -> Self {
        Outcome { message: Some(text.into()), value: Value::Null }
    }

    fn value(value: Value) -> Self {
        Outcome { message: None, value }
    }
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ParamType {
    String,
    Integer,
    Bool,
    /// An element kind id such as `scene_heading`.
    Kind,
    Path,
}

#[derive(Clone, Copy, Debug, Serialize)]
pub struct ParamSpec {
    pub name: &'static str,
    #[serde(rename = "type")]
    pub kind: ParamType,
    pub required: bool,
    pub doc: &'static str,
}

const fn param(name: &'static str, kind: ParamType, required: bool, doc: &'static str) -> ParamSpec {
    ParamSpec { name, kind, required, doc }
}

type RunFn = fn(&mut Session, &Params) -> Result<Outcome, CommandError>;
type EnabledFn = fn(&Session) -> bool;

/// A named action.
#[derive(Clone, Copy)]
pub struct Command {
    pub id: &'static str,
    pub label: &'static str,
    /// Menu placement, `Menu/Item` (empty for commands with no menu item).
    pub menu: &'static str,
    /// Shortcut in `Cmd+Shift+S` form; `Cmd` is Ctrl on Windows and Linux.
    pub shortcut: Option<&'static str>,
    pub params: &'static [ParamSpec],
    pub enabled: EnabledFn,
    pub run: RunFn,
}

impl std::fmt::Debug for Command {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Command").field("id", &self.id).finish()
    }
}

/// Command description for `command.list`.
#[derive(Serialize)]
pub struct CommandInfo {
    pub id: &'static str,
    pub label: &'static str,
    pub menu: &'static str,
    pub shortcut: Option<&'static str>,
    pub params: &'static [ParamSpec],
    pub enabled: bool,
}

/// All commands, in menu order.
pub struct Registry {
    commands: Vec<Command>,
}

impl Default for Registry {
    fn default() -> Self {
        Registry::new()
    }
}

impl Registry {
    pub fn new() -> Self {
        Registry { commands: all_commands() }
    }

    pub fn get(&self, id: &str) -> Option<&Command> {
        self.commands.iter().find(|c| c.id == id)
    }

    pub fn iter(&self) -> impl Iterator<Item = &Command> {
        self.commands.iter()
    }

    pub fn list(&self, session: &Session) -> Vec<CommandInfo> {
        self.commands
            .iter()
            .map(|c| CommandInfo { id: c.id, label: c.label, menu: c.menu, shortcut: c.shortcut, params: c.params, enabled: (c.enabled)(session) })
            .collect()
    }

    /// Run a command by id.
    pub fn run(&self, id: &str, session: &mut Session, params: &Params) -> Result<Outcome, CommandError> {
        let command = self.get(id).ok_or_else(|| CommandError::Unknown(id.to_string()))?;
        if !(command.enabled)(session) {
            return Err(CommandError::Disabled(id.to_string()));
        }
        let outcome = (command.run)(session, params)?;
        session.clamp_caret();
        Ok(outcome)
    }
}

// ---------------------------------------------------------------------------------------------
// Parameter helpers

fn string(params: &Params, name: &'static str) -> Result<String, CommandError> {
    opt_string(params, name)?.ok_or(CommandError::Missing(name))
}

fn opt_string(params: &Params, name: &'static str) -> Result<Option<String>, CommandError> {
    match params.get(name) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) => Ok(Some(s.clone())),
        Some(other) => Err(CommandError::Invalid { name, reason: format!("expected a string, got {other}") }),
    }
}

fn opt_index(params: &Params, name: &'static str) -> Result<Option<usize>, CommandError> {
    match params.get(name) {
        None | Some(Value::Null) => Ok(None),
        Some(value) => value
            .as_u64()
            .and_then(|v| usize::try_from(v).ok())
            .map(Some)
            .ok_or_else(|| CommandError::Invalid { name, reason: format!("expected a non-negative integer, got {value}") }),
    }
}

fn opt_bool(params: &Params, name: &'static str) -> Result<Option<bool>, CommandError> {
    match params.get(name) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Bool(b)) => Ok(Some(*b)),
        Some(other) => Err(CommandError::Invalid { name, reason: format!("expected true or false, got {other}") }),
    }
}

fn kind(params: &Params, name: &'static str) -> Result<ElementKind, CommandError> {
    let id = string(params, name)?;
    ElementKind::from_id(&id).ok_or_else(|| CommandError::Invalid { name, reason: format!("unknown element type `{id}`") })
}

fn path(params: &Params, name: &'static str) -> Result<PathBuf, CommandError> {
    let text = string(params, name)?;
    if text.trim().is_empty() {
        return Err(CommandError::Invalid { name, reason: "empty path".into() });
    }
    Ok(PathBuf::from(text))
}

/// Element index from `index` (default: the caret's element), checked.
fn element_index(session: &Session, params: &Params) -> Result<usize, CommandError> {
    let index = opt_index(params, "index")?.unwrap_or(session.caret.element);
    if index >= session.script.elements.len() {
        return Err(CommandError::Invalid { name: "index", reason: format!("there are {} elements", session.script.elements.len()) });
    }
    Ok(index)
}

/// Byte offset from `offset` (default: the caret if it is in that element, else the end).
fn text_offset(session: &Session, params: &Params, index: usize) -> Result<usize, CommandError> {
    let text = session.element(index).map_or("", |e| e.text.as_str());
    let default = if session.caret.element == index { session.caret.offset } else { text.len() };
    let offset = opt_index(params, "offset")?.unwrap_or(default);
    Ok(floor_char_boundary(text, offset))
}

fn always(_: &Session) -> bool {
    true
}

fn has_path(session: &Session) -> bool {
    session.path.is_some()
}

// ---------------------------------------------------------------------------------------------
// Edit operations, shared by commands

/// Replace an element's text, applying the kind's capitalisation.
pub fn set_text(session: &mut Session, index: usize, text: &str, typing: bool) -> Result<(), CommandError> {
    let Some(element) = session.script.elements.get(index) else {
        return Err(CommandError::Invalid { name: "index", reason: "no such element".into() });
    };
    let mut text = if element.kind.is_single_line() { text.replace(['\n', '\r'], " ") } else { text.replace('\r', "") };
    if element.kind.is_caps() {
        text = text.to_uppercase();
    }
    if element.text == text {
        return Ok(());
    }
    session.checkpoint(if typing { EditKind::Typing(index) } else { EditKind::Other });
    if let Some(element) = session.script.elements.get_mut(index) {
        element.text = text;
    }
    Ok(())
}

fn set_kind(session: &mut Session, index: usize, to: ElementKind) -> Result<(), CommandError> {
    let Some(element) = session.script.elements.get(index) else {
        return Err(CommandError::Invalid { name: "index", reason: "no such element".into() });
    };
    let from = element.kind;
    if from == to {
        return Ok(());
    }
    let (text, shift) = flow::convert_text(&element.text, from, to);
    session.checkpoint(EditKind::Other);
    if let Some(element) = session.script.elements.get_mut(index) {
        element.kind = to;
        element.text = text;
        element.normalize();
    }
    if session.caret.element == index {
        session.caret.offset = session.caret.offset.saturating_add_signed(shift);
    }
    Ok(())
}

fn insert(session: &mut Session, at: usize, element: Element) -> Result<(), CommandError> {
    if session.script.elements.len() >= storyteller_core::MAX_ELEMENTS {
        return Err(CommandError::Failed("the script has too many elements".into()));
    }
    let at = at.min(session.script.elements.len());
    session.script.elements.insert(at, element);
    Ok(())
}

fn new_element(kind: ElementKind) -> (Element, usize) {
    if kind == ElementKind::Parenthetical { (Element::new(kind, "()"), 1) } else { (Element::new(kind, ""), 0) }
}

/// Enter at the caret: Final Draft element flow.
pub fn enter(session: &mut Session, index: usize, offset: usize) -> Result<(), CommandError> {
    let Some(element) = session.script.elements.get(index).cloned() else {
        return Err(CommandError::Invalid { name: "index", reason: "no such element".into() });
    };
    let rule = flow::rule_for(element.kind);
    let empty = element.text.trim().is_empty() || (element.kind == ElementKind::Parenthetical && element.text.trim() == "()");
    if empty {
        set_kind(session, index, rule.enter_empty)?;
        if let Some(element) = session.script.elements.get_mut(index) {
            element.text.clear();
        }
        let (fresh, caret) = new_element(rule.enter_empty);
        if let Some(element) = session.script.elements.get_mut(index) {
            element.text = fresh.text;
        }
        session.caret = Caret { element: index, offset: caret };
        return Ok(());
    }
    session.checkpoint(EditKind::Other);
    let offset = floor_char_boundary(&element.text, offset);
    if offset == 0 {
        // At the start of a paragraph: open an empty one of the same type above it.
        insert(session, index, Element::new(element.kind, ""))?;
        session.caret = Caret { element: index + 1, offset: 0 };
        return Ok(());
    }
    let mut head = element.text.get(..offset).unwrap_or("").to_string();
    let mut tail = element.text.get(offset..).unwrap_or("").trim_start().to_string();
    // A caret just inside a parenthetical's closing bracket ends the parenthetical.
    if element.kind == ElementKind::Parenthetical && tail.trim() == ")" {
        head.push(')');
        tail.clear();
    }
    if let Some(current) = session.script.elements.get_mut(index) {
        current.text = head.trim_end().to_string();
    }
    let next_kind = rule.enter;
    let (mut fresh, mut caret) = new_element(next_kind);
    if !tail.is_empty() {
        let (text, _) = flow::convert_text(&tail, element.kind, next_kind);
        fresh.text = text;
        caret = usize::from(next_kind == ElementKind::Parenthetical);
    }
    insert(session, index + 1, fresh)?;
    session.caret = Caret { element: index + 1, offset: caret };
    Ok(())
}

/// Backspace at the start of an element: merge it into the previous one.
pub fn merge_previous(session: &mut Session, index: usize) -> Result<bool, CommandError> {
    if index == 0 || index >= session.script.elements.len() {
        return Ok(false);
    }
    session.checkpoint(EditKind::Other);
    let current = session.script.elements.remove(index);
    let previous_index = index - 1;
    let Some(previous) = session.script.elements.get_mut(previous_index) else { return Ok(false) };
    if previous.kind == ElementKind::PageBreak {
        session.script.elements.remove(previous_index);
        session.script.elements.insert(previous_index, current);
        session.caret = Caret { element: previous_index, offset: 0 };
        return Ok(true);
    }
    let mut extra = current.text;
    if previous.kind == ElementKind::Parenthetical || current.kind == ElementKind::Parenthetical {
        extra = extra.trim_matches(['(', ')']).to_string();
    }
    let join_at = previous.text.trim_end_matches(')').len().min(previous.text.len());
    let at = if previous.kind == ElementKind::Parenthetical { join_at } else { previous.text.len() };
    let mut text = previous.text.clone();
    if previous.kind.is_caps() {
        extra = extra.to_uppercase();
    }
    text.insert_str(floor_char_boundary(&text, at), &extra);
    previous.text = if previous.kind.is_single_line() { text.replace('\n', " ") } else { text };
    session.caret = Caret { element: previous_index, offset: at };
    Ok(true)
}

// ---------------------------------------------------------------------------------------------
// Commands

/// A `format.*` command that sets the element type (⌘1–⌘7).
macro_rules! kind_command {
    ($id:expr, $label:expr, $menu:expr, $shortcut:expr, $kind:expr) => {
        Command {
            id: $id,
            label: $label,
            menu: $menu,
            shortcut: Some($shortcut),
            params: const { &[param("index", ParamType::Integer, false, "Element index (default: the caret's element)")] },
            enabled: always,
            run: |s, p| {
                let index = element_index(s, p)?;
                set_kind(s, index, $kind)?;
                Ok(Outcome::done())
            },
        }
    };
}

fn all_commands() -> Vec<Command> {
    use ParamType::*;
    const INDEX: ParamSpec = param("index", Integer, false, "Element index (default: the caret's element)");
    const OFFSET: ParamSpec = param("offset", Integer, false, "Byte offset in the element text (default: the caret)");
    vec![
        Command {
            id: "file.new",
            label: "New",
            menu: "File/New",
            shortcut: Some("Cmd+N"),
            params: &[],
            enabled: always,
            run: |s, _| {
                *s = Session::new(Script::blank());
                Ok(Outcome::done())
            },
        },
        Command {
            id: "file.open",
            label: "Open…",
            menu: "File/Open…",
            shortcut: Some("Cmd+O"),
            params: const { &[param("path", Path, true, "Fountain (.fountain) or Final Draft (.fdx) file")] },
            enabled: always,
            run: |s, p| {
                let path = path(p, "path")?;
                let (script, format) = files::open(&path).map_err(|e| CommandError::Failed(e.to_string()))?;
                let layout = s.layout.clone();
                *s = Session::new(script);
                s.layout = layout;
                s.format = format;
                s.path = Some(path);
                Ok(Outcome::message(format!("Opened {} elements", s.script.elements.len())))
            },
        },
        Command {
            id: "file.save",
            label: "Save",
            menu: "File/Save",
            shortcut: Some("Cmd+S"),
            params: &[],
            enabled: has_path,
            run: |s, _| {
                let path = s.path.clone().ok_or(CommandError::Missing("path"))?;
                files::save(&s.script, &path, s.format, &s.layout).map_err(|e| CommandError::Failed(e.to_string()))?;
                s.dirty = false;
                s.break_typing();
                Ok(Outcome::message(format!("Saved {}", path.display())))
            },
        },
        Command {
            id: "file.save_as",
            label: "Save As…",
            menu: "File/Save As…",
            shortcut: Some("Cmd+Shift+S"),
            params: const { &[param("path", Path, true, "Destination; .fdx saves FDX, .osf or .fadein Open Screenplay Format, anything else Fountain")] },
            enabled: always,
            run: |s, p| {
                let path = path(p, "path")?;
                let format = match FileFormat::from_path(&path) {
                    FileFormat::Pdf => return Err(CommandError::Invalid { name: "path", reason: "use file.export_pdf for PDF".into() }),
                    other => other,
                };
                files::save(&s.script, &path, format, &s.layout).map_err(|e| CommandError::Failed(e.to_string()))?;
                s.path = Some(path.clone());
                s.format = format;
                s.dirty = false;
                s.break_typing();
                Ok(Outcome::message(format!("Saved {}", path.display())))
            },
        },
        Command {
            id: "file.export_pdf",
            label: "Export PDF…",
            menu: "File/Export PDF…",
            shortcut: Some("Cmd+E"),
            params: const { &[param("path", Path, true, "Destination .pdf")] },
            enabled: always,
            run: |s, p| export(s, p, FileFormat::Pdf),
        },
        Command {
            id: "file.export_fdx",
            label: "Export FDX…",
            menu: "File/Export FDX (.fdx)…",
            shortcut: None,
            params: const { &[param("path", Path, true, "Destination .fdx")] },
            enabled: always,
            run: |s, p| export(s, p, FileFormat::Fdx),
        },
        Command {
            id: "file.export_osf",
            label: "Export Open Screenplay Format…",
            menu: "File/Export Open Screenplay Format (.osf)…",
            shortcut: None,
            params: const { &[param("path", Path, true, "Destination .osf, or .fadein for the ZIP container")] },
            enabled: always,
            run: |s, p| {
                let path = path(p, "path")?;
                let format = if FileFormat::from_path(&path) == FileFormat::FadeIn { FileFormat::FadeIn } else { FileFormat::Osf };
                files::save(&s.script, &path, format, &s.layout).map_err(|e| CommandError::Failed(e.to_string()))?;
                Ok(Outcome::message(format!("Exported {}", path.display())))
            },
        },
        Command {
            id: "file.export_fountain",
            label: "Export Fountain…",
            menu: "File/Export Fountain…",
            shortcut: None,
            params: const { &[param("path", Path, true, "Destination .fountain")] },
            enabled: always,
            run: |s, p| export(s, p, FileFormat::Fountain),
        },
        Command {
            id: "edit.undo",
            label: "Undo",
            menu: "Edit/Undo",
            shortcut: Some("Cmd+Z"),
            params: &[],
            enabled: Session::can_undo,
            run: |s, _| {
                s.undo();
                Ok(Outcome::done())
            },
        },
        Command {
            id: "edit.redo",
            label: "Redo",
            menu: "Edit/Redo",
            shortcut: Some("Cmd+Shift+Z"),
            params: &[],
            enabled: Session::can_redo,
            run: |s, _| {
                s.redo();
                Ok(Outcome::done())
            },
        },
        Command {
            id: "caret.set",
            label: "Move Caret",
            menu: "",
            shortcut: None,
            params: const { &[INDEX, OFFSET] },
            enabled: always,
            run: |s, p| {
                let index = element_index(s, p)?;
                let offset = text_offset(s, p, index)?;
                s.caret = Caret { element: index, offset };
                s.break_typing();
                Ok(Outcome::done())
            },
        },
        Command {
            id: "element.set_text",
            label: "Set Text",
            menu: "",
            shortcut: None,
            params: const {
                &[
                    INDEX,
                    param("text", String, true, "New text, with Fountain inline markup"),
                    param("typing", Bool, false, "Merge with the previous typing step"),
                ]
            },
            enabled: always,
            run: |s, p| {
                let index = element_index(s, p)?;
                let text = string(p, "text")?;
                set_text(s, index, &text, opt_bool(p, "typing")?.unwrap_or(false))?;
                if s.caret.element == index {
                    s.caret.offset = s.caret.offset.min(s.script.elements.get(index).map_or(0, |e| e.text.len()));
                }
                Ok(Outcome::done())
            },
        },
        Command {
            id: "element.insert",
            label: "Insert Element",
            menu: "",
            shortcut: None,
            params: const {
                &[
                    param("index", Integer, false, "Insert after this element (default: the caret's)"),
                    param("kind", Kind, true, "Element type id"),
                    param("text", String, false, "Text"),
                ]
            },
            enabled: always,
            run: |s, p| {
                let after = element_index(s, p)?;
                let kind = kind(p, "kind")?;
                let mut element = Element::new(kind, opt_string(p, "text")?.unwrap_or_default());
                element.normalize();
                s.checkpoint(EditKind::Other);
                let len = element.text.len();
                insert(s, after + 1, element)?;
                s.caret = Caret { element: after + 1, offset: len };
                Ok(Outcome::value(json!({ "index": after + 1 })))
            },
        },
        Command {
            id: "element.delete",
            label: "Delete Element",
            menu: "",
            shortcut: None,
            params: const { &[INDEX] },
            enabled: always,
            run: |s, p| {
                let index = element_index(s, p)?;
                s.checkpoint(EditKind::Other);
                s.script.elements.remove(index);
                if s.script.elements.is_empty() {
                    s.script.elements.push(Element::new(ElementKind::SceneHeading, ""));
                }
                s.caret = Caret { element: index.saturating_sub(usize::from(index >= s.script.elements.len())), offset: 0 };
                Ok(Outcome::done())
            },
        },
        Command {
            id: "element.set_kind",
            label: "Set Element Type",
            menu: "",
            shortcut: None,
            params: const { &[INDEX, param("kind", Kind, true, "Element type id")] },
            enabled: always,
            run: |s, p| {
                let index = element_index(s, p)?;
                set_kind(s, index, kind(p, "kind")?)?;
                Ok(Outcome::done())
            },
        },
        kind_command!("format.scene_heading", "Scene Heading", "Format/Scene Heading", "Cmd+1", ElementKind::SceneHeading),
        kind_command!("format.action", "Action", "Format/Action", "Cmd+2", ElementKind::Action),
        kind_command!("format.character", "Character", "Format/Character", "Cmd+3", ElementKind::Character),
        kind_command!("format.parenthetical", "Parenthetical", "Format/Parenthetical", "Cmd+4", ElementKind::Parenthetical),
        kind_command!("format.dialogue", "Dialogue", "Format/Dialogue", "Cmd+5", ElementKind::Dialogue),
        kind_command!("format.transition", "Transition", "Format/Transition", "Cmd+6", ElementKind::Transition),
        kind_command!("format.shot", "Shot", "Format/Shot", "Cmd+7", ElementKind::Shot),
        Command {
            id: "flow.enter",
            label: "New Element (Enter)",
            menu: "",
            shortcut: Some("Enter"),
            params: const { &[INDEX, OFFSET] },
            enabled: always,
            run: |s, p| {
                let index = element_index(s, p)?;
                let offset = text_offset(s, p, index)?;
                enter(s, index, offset)?;
                Ok(Outcome::done())
            },
        },
        Command {
            id: "flow.tab",
            label: "Next Element Type (Tab)",
            menu: "",
            shortcut: Some("Tab"),
            params: const { &[INDEX] },
            enabled: always,
            run: |s, p| {
                let index = element_index(s, p)?;
                let to = s.element(index).map(|e| flow::rule_for(e.kind).tab).unwrap_or(ElementKind::Action);
                set_kind(s, index, to)?;
                Ok(Outcome::done())
            },
        },
        Command {
            id: "flow.shift_tab",
            label: "Previous Element Type (Shift-Tab)",
            menu: "",
            shortcut: Some("Shift+Tab"),
            params: const { &[INDEX] },
            enabled: always,
            run: |s, p| {
                let index = element_index(s, p)?;
                let to = s.element(index).map(|e| flow::rule_for(e.kind).shift_tab).unwrap_or(ElementKind::Action);
                set_kind(s, index, to)?;
                Ok(Outcome::done())
            },
        },
        Command {
            id: "element.merge_previous",
            label: "Join With Previous (Backspace at Start)",
            menu: "",
            shortcut: None,
            params: const { &[INDEX] },
            enabled: always,
            run: |s, p| {
                let index = element_index(s, p)?;
                let merged = merge_previous(s, index)?;
                Ok(Outcome::value(json!({ "merged": merged })))
            },
        },
        Command {
            id: "scene.number",
            label: "Number Scenes",
            menu: "Production/Number Scenes",
            shortcut: None,
            params: const { &[param("keep_existing", Bool, false, "Keep numbers that are already set")] },
            enabled: always,
            run: |s, p| {
                s.checkpoint(EditKind::Other);
                s.script.number_scenes(opt_bool(p, "keep_existing")?.unwrap_or(false));
                Ok(Outcome::message(format!("Numbered {} scenes", s.script.scene_indices().len())))
            },
        },
        Command {
            id: "scene.clear_numbers",
            label: "Remove Scene Numbers",
            menu: "Production/Remove Scene Numbers",
            shortcut: None,
            params: &[],
            enabled: always,
            run: |s, _| {
                s.checkpoint(EditKind::Other);
                s.script.clear_scene_numbers();
                Ok(Outcome::done())
            },
        },
        Command {
            id: "view.scene_numbers",
            label: "Show Scene Numbers",
            menu: "View/Show Scene Numbers",
            shortcut: None,
            params: const { &[param("show", Bool, false, "Show (true) or hide (false); toggles when omitted")] },
            enabled: always,
            run: |s, p| {
                let show = opt_bool(p, "show")?.unwrap_or(!s.layout.show_scene_numbers);
                s.layout.show_scene_numbers = show;
                s.invalidate_layout();
                Ok(Outcome::value(json!({ "show": show })))
            },
        },
        Command {
            id: "title_page.set",
            label: "Set Title Page Field",
            menu: "",
            shortcut: None,
            params: const {
                &[
                    param("key", String, true, "Title, Credit, Author, Source, Draft date, Contact, Copyright or Notes"),
                    param("value", String, true, "Value (empty removes it)"),
                ]
            },
            enabled: always,
            run: |s, p| {
                let key = string(p, "key")?;
                let value = string(p, "value")?;
                if key.trim().is_empty() || key.contains([':', '\n']) {
                    return Err(CommandError::Invalid { name: "key", reason: "keys are one line without colons".into() });
                }
                s.checkpoint(EditKind::Other);
                s.script.title_page.set(key.trim(), &value);
                Ok(Outcome::done())
            },
        },
        Command {
            id: "autocomplete.suggest",
            label: "Autocomplete Suggestions",
            menu: "",
            shortcut: None,
            params: const { &[INDEX] },
            enabled: always,
            run: |s, p| {
                let index = element_index(s, p)?;
                let suggestions = s.element(index).map(|e| autocomplete::suggest(&s.script, e.kind, &e.text)).unwrap_or_default();
                Ok(Outcome::value(json!(suggestions)))
            },
        },
    ]
}

fn export(session: &mut Session, params: &Params, format: FileFormat) -> Result<Outcome, CommandError> {
    let path = path(params, "path")?;
    files::save(&session.script, &path, format, &session.layout).map_err(|e| CommandError::Failed(e.to_string()))?;
    Ok(Outcome::message(format!("Exported {}", path.display())))
}
