//! Every action is a named command: id, label, menu path, shortcut, typed params, `enabled`
//! and `run`. Menus, keyboard shortcuts, the CLI and the control channel all dispatch by id.

use std::path::PathBuf;

use serde::Serialize;
use serde_json::{Map, Value, json};
use storyteller_core::{Element, ElementKind, Script, autocomplete, inline};

use crate::files::{self, FileError, FileFormat, FileProblem};
use crate::find;
use crate::flow;
use crate::reports;
use crate::scenes_file;
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
    /// A file could not be read or written; `problem` says what kind of trouble it was.
    #[error("{detail}")]
    File { problem: FileProblem, detail: String },
}

impl CommandError {
    /// The kind of file trouble, when this is a file error.
    pub fn file_problem(&self) -> Option<FileProblem> {
        match self {
            CommandError::File { problem, .. } => Some(*problem),
            _ => None,
        }
    }
}

impl From<FileError> for CommandError {
    fn from(error: FileError) -> Self {
        CommandError::File { problem: error.problem(), detail: error.to_string() }
    }
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
        let edits = session.edits();
        let outcome = (command.run)(session, params)?;
        // Only after an edit (not after opening a file, undo or redo): those keep the script
        // as it was.
        if session.edits() != edits {
            session.keep_acts_in_step();
        }
        session.clamp_caret();
        Ok(outcome)
    }
}

/// The new name in the case of the mention it replaces: `NORA` → `EMMA`, `Nora` → `Emma`.
/// A mixed-case new name such as `McCLANE` is kept as typed for a capitalised mention.
fn name_like(found: &str, to: &str) -> std::string::String {
    let all_caps = |t: &str| t.chars().any(char::is_alphabetic) && t.chars().all(|c| !c.is_lowercase());
    if all_caps(found) && found.chars().filter(|c| c.is_alphabetic()).count() > 1 {
        return to.to_uppercase();
    }
    if !all_caps(to) && to.chars().any(char::is_uppercase) {
        return to.to_string();
    }
    let mut chars = to.chars();
    chars.next().map(|c| c.to_uppercase().chain(chars.flat_map(char::to_lowercase)).collect()).unwrap_or_default()
}

fn plural(count: usize, one: &'static str, many: &'static str) -> &'static str {
    if count == 1 { one } else { many }
}

/// Every character name in the script, by number of speeches, most first, then by name.
pub fn character_names(script: &Script) -> Vec<(String, usize)> {
    let mut counts: Vec<(String, usize)> = Vec::new();
    for element in script.elements.iter().filter(|e| e.kind == ElementKind::Character) {
        let name = element.character_name().to_uppercase();
        if name.is_empty() {
            continue;
        }
        match counts.iter_mut().find(|(n, _)| *n == name) {
            Some((_, count)) => *count += 1,
            None => counts.push((name, 1)),
        }
    }
    counts.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    counts
}

/// One row of the character table: a speaking character, with the speeches, scenes and first
/// appearance.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct CharacterRow {
    pub name: String,
    pub speeches: usize,
    /// Scenes the character speaks in.
    pub scenes: usize,
    /// The element of the first cue.
    pub first: usize,
    /// The scene of the first cue, by its printed number or position, if it is in one.
    pub first_scene: Option<String>,
    /// Words spoken, dialogue only.
    pub words: usize,
    /// The scenes the character speaks in, by printed number or position, in script order.
    pub scene_list: Vec<String>,
}

/// Every speaking character, most speeches first (then by name).
pub fn character_table(script: &Script) -> Vec<CharacterRow> {
    // A scene by its printed number, or its position when it has none.
    let scene_label = |scene: Option<usize>, count: usize| {
        scene.map(|h| script.elements.get(h).and_then(|e| e.scene_number.clone()).unwrap_or_else(|| count.to_string()))
    };
    let mut rows: Vec<CharacterRow> = Vec::new();
    let mut scene: Option<usize> = None;
    let mut scene_count = 0usize;
    let mut seen_in: Vec<(String, usize)> = Vec::new();
    let mut speaker: Option<usize> = None;
    for (index, element) in script.elements.iter().enumerate() {
        match element.kind {
            ElementKind::SceneHeading => {
                scene_count += 1;
                scene = Some(index);
                speaker = None;
            }
            ElementKind::Character => {
                let name = element.character_name().to_uppercase();
                if name.is_empty() {
                    speaker = None;
                    continue;
                }
                let at = match rows.iter().position(|r| r.name == name) {
                    Some(at) => at,
                    None => {
                        rows.push(CharacterRow {
                            name: name.clone(),
                            speeches: 0,
                            scenes: 0,
                            first: index,
                            first_scene: scene_label(scene, scene_count),
                            words: 0,
                            scene_list: Vec::new(),
                        });
                        rows.len() - 1
                    }
                };
                if let Some(row) = rows.get_mut(at) {
                    row.speeches += 1;
                    let key = (name, scene.unwrap_or(usize::MAX));
                    if !seen_in.contains(&key) {
                        seen_in.push(key);
                        row.scenes += 1;
                        row.scene_list.extend(scene_label(scene, scene_count));
                    }
                }
                speaker = Some(at);
            }
            ElementKind::Dialogue | ElementKind::Lyrics => {
                if let Some(row) = speaker.and_then(|at| rows.get_mut(at)) {
                    row.words += storyteller_core::inline::plain(&element.text).split_whitespace().count();
                }
            }
            ElementKind::Parenthetical => {}
            _ => speaker = None,
        }
    }
    rows.sort_by(|a, b| b.speeches.cmp(&a.speeches).then_with(|| a.name.cmp(&b.name)));
    rows
}

/// The find parameters shared by `find.*`.
fn find_query(params: &Params) -> Result<find::Query, CommandError> {
    let text = string(params, "text")?;
    if text.is_empty() {
        return Err(CommandError::Invalid { name: "text", reason: "nothing to find".into() });
    }
    let kind = match opt_string(params, "kind")? {
        Some(_) => Some(kind(params, "kind")?),
        None => None,
    };
    Ok(find::Query { text, case: opt_bool(params, "case")?.unwrap_or(false), whole_word: opt_bool(params, "whole_word")?.unwrap_or(false), kind })
}

/// The element `nav.go_to` lands on: a scene heading, or the first element on a page.
fn go_to(session: &mut Session, scene: Option<String>, page: Option<usize>) -> Result<usize, CommandError> {
    let paginated = session.paginated();
    if let Some(scene) = scene {
        let wanted = scene.trim();
        // The printed scene number (12, 12A). Only a script with no scene numbers at all goes by
        // position, so a missing number is never mistaken for another scene.
        let numbered = paginated.scenes.iter().any(|s| s.number.is_some());
        let found = if numbered {
            paginated.scenes.iter().find(|s| s.number.as_deref().is_some_and(|n| n.eq_ignore_ascii_case(wanted)))
        } else {
            wanted.parse::<usize>().ok().and_then(|n| n.checked_sub(1)).and_then(|n| paginated.scenes.get(n))
        };
        return found.map(|s| s.element).ok_or_else(|| CommandError::Invalid { name: "scene", reason: format!("there is no scene {wanted}") });
    }
    let page = page.ok_or(CommandError::Missing("scene"))?;
    let count = paginated.page_count();
    let wanted = u32::try_from(page).unwrap_or(u32::MAX);
    if page == 0 || page > count {
        return Err(CommandError::Invalid { name: "page", reason: format!("there is no page {page} (the script has {count})") });
    }
    // The first element that starts on the page, or the one running onto it from the page before.
    let pages = &paginated.element_pages;
    pages
        .iter()
        .position(|&p| p == wanted)
        .or_else(|| pages.iter().rposition(|&p| p > 0 && p < wanted))
        .ok_or_else(|| CommandError::Invalid { name: "page", reason: format!("page {page} has no script text") })
}

/// The paragraph a bookmark's name, or the start of one, points to.
fn find_bookmark(script: &Script, wanted: &str) -> Result<usize, CommandError> {
    let wanted = wanted.trim().to_lowercase();
    let named: Vec<(usize, String)> =
        script.elements.iter().enumerate().filter_map(|(i, e)| e.bookmark.as_ref().map(|name| (i, name.to_lowercase()))).collect();
    if let Some((index, _)) = named.iter().find(|(_, name)| *name == wanted) {
        return Ok(*index);
    }
    let starts: Vec<usize> = named.iter().filter(|(_, name)| !wanted.is_empty() && name.starts_with(&wanted)).map(|(i, _)| *i).collect();
    match starts.as_slice() {
        [only] => Ok(*only),
        [] => Err(CommandError::Invalid { name: "bookmark", reason: format!("there is no bookmark {}", wanted) }),
        _ => Err(CommandError::Invalid { name: "bookmark", reason: format!("more than one bookmark starts with {}", wanted) }),
    }
}

/// The list a command names.
fn list_name(s: &Session, p: &Params) -> Result<std::string::String, CommandError> {
    let name = string(p, "list")?;
    if s.script.lists.list(&name).is_none() {
        return Err(CommandError::Invalid { name: "list", reason: format!("there is no list {name}") });
    }
    Ok(name)
}

fn has_bookmark(s: &Session) -> bool {
    s.script.elements.get(s.caret.element).is_some_and(|e| e.bookmark.is_some())
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

fn show_revisions(name: &'static str, text: &str) -> Result<storyteller_core::model::ShowRevisions, CommandError> {
    use storyteller_core::model::ShowRevisions;
    match text {
        "active" => Ok(ShowRevisions::Active),
        "all" => Ok(ShowRevisions::All),
        "none" => Ok(ShowRevisions::None),
        other => Err(CommandError::Invalid { name, reason: format!("expected active, all or none, got {other}") }),
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
                element_shortcut(s, index, $kind)?;
                Ok(Outcome::done())
            },
        }
    };
}

/// The change-to shortcuts: ⌘⌥1–7 on macOS; Ctrl+Shift+1–7 elsewhere, where Ctrl+Alt is AltGr
/// and would swallow characters typed on many keyboard layouts.
const CONVERT_KEYS: [&str; 7] = if cfg!(target_os = "macos") {
    ["Cmd+Alt+1", "Cmd+Alt+2", "Cmd+Alt+3", "Cmd+Alt+4", "Cmd+Alt+5", "Cmd+Alt+6", "Cmd+Alt+7"]
} else {
    ["Cmd+Shift+1", "Cmd+Shift+2", "Cmd+Shift+3", "Cmd+Shift+4", "Cmd+Shift+5", "Cmd+Shift+6", "Cmd+Shift+7"]
};

/// ⌘⌥1–⌘⌥7: change the current element's type in place.
macro_rules! convert_command {
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

/// Every command, in menu order: each area's list in turn.
fn all_commands() -> Vec<Command> {
    [file_commands(), editing_commands(), element_commands(), list_commands(), production_commands(), view_commands()].concat()
}

/// The parameters several commands share.
const INDEX: ParamSpec = param("index", ParamType::Integer, false, "Element index (default: the caret's element)");
const OFFSET: ParamSpec = param("offset", ParamType::Integer, false, "Byte offset in the element text (default: the caret)");
const RANGE: [ParamSpec; 4] = [
    param("from_index", ParamType::Integer, true, "Element the range starts in"),
    param("from_offset", ParamType::Integer, true, "Byte offset where it starts"),
    param("to_index", ParamType::Integer, true, "Element the range ends in"),
    param("to_offset", ParamType::Integer, true, "Byte offset where it ends"),
];
const LIST_NAME: ParamSpec = param("list", ParamType::String, true, "characters, extensions, intros, locations, times or transitions");
const FIND_TEXT: ParamSpec = param("text", ParamType::String, true, "Text to find");
const FIND_CASE: ParamSpec = param("case", ParamType::Bool, false, "Match case exactly");
const FIND_WORD: ParamSpec = param("whole_word", ParamType::Bool, false, "Whole words only");
const FIND_KIND: ParamSpec = param("kind", ParamType::Kind, false, "Only in elements of this type");
const FIND_REPLACEMENT: ParamSpec = param("replacement", ParamType::String, true, "Replacement text");

#[path = "edit.rs"]
mod edit;
#[path = "editing.rs"]
mod editing;
#[path = "elements.rs"]
mod elements;
#[path = "file_commands.rs"]
mod file_commands;
#[path = "lists.rs"]
mod lists;
#[path = "production.rs"]
mod production;
#[path = "view.rs"]
mod view;

pub use edit::*;
use editing::*;
use elements::*;
use file_commands::*;
use lists::*;
use production::*;
use view::*;
