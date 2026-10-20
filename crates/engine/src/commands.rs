//! Every action is a named command: id, label, menu path, shortcut, typed params, `enabled`
//! and `run`. Menus, keyboard shortcuts, the CLI and the control channel all dispatch by id.

use std::path::PathBuf;

use serde::Serialize;
use serde_json::{Map, Value, json};
use storyteller_core::{Element, ElementKind, Script, autocomplete, inline};

use crate::files::{self, FileError, FileFormat, FileProblem};
use crate::find;
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
    // Capitals for the kinds that always take them, and for a type the script's settings print
    // in capitals (a multi-camera script's action).
    if element.kind.is_caps() || session.layout.style(element.kind).caps {
        text = text.to_uppercase();
    }
    if element.text == text {
        return Ok(());
    }
    // Typing "int." or "ext." at the start of action makes it a scene heading, as in Final Draft,
    // however many keys arrive at once.
    let becomes_heading = typing && element.kind == ElementKind::Action && flow::becomes_heading(&element.text, &text);
    session.checkpoint(if typing { EditKind::Typing(index) } else { EditKind::Other });
    if let Some(element) = session.script.elements.get_mut(index) {
        element.set_text(text);
    }
    if becomes_heading {
        // Its own undo step, so one Undo turns it back into the action just typed.
        session.checkpoint(EditKind::Other);
        if let Some(element) = session.script.elements.get_mut(index) {
            element.kind = ElementKind::SceneHeading;
            element.normalize();
        }
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
        element.set_text(text);
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

/// The elements of the scene `index` is in: from its heading (or the start of the script) up to
/// the next heading.
pub fn scene_range(script: &Script, index: usize) -> std::ops::Range<usize> {
    let elements = &script.elements;
    let index = index.min(elements.len().saturating_sub(1));
    let start = elements.get(..=index).unwrap_or_default().iter().rposition(|e| e.kind == ElementKind::SceneHeading).unwrap_or(0);
    let end = elements
        .get(start + 1..)
        .unwrap_or_default()
        .iter()
        .position(|e| e.kind == ElementKind::SceneHeading)
        .map_or(elements.len(), |n| start + 1 + n);
    start..end
}

/// Whether pasted text is screenplay text in several paragraphs (a blank line between two), so
/// that pasting it makes elements rather than one long paragraph.
pub fn is_screenplay_paste(text: &str) -> bool {
    text.replace("\r\n", "\n").trim().split("\n\n").filter(|part| !part.trim().is_empty()).nth(1).is_some()
}

/// Pastes screenplay text at `start..end` of element `index`, read as Fountain, as one Undo
/// step: the text before the paste stays in the element, the pasted elements follow, and the
/// text after the paste becomes an element of its own. Returns how many elements were pasted.
fn paste_screenplay(s: &mut Session, index: usize, start: usize, end: usize, text: &str) -> Result<usize, CommandError> {
    let Some(element) = s.script.elements.get(index) else {
        return Err(CommandError::Invalid { name: "index", reason: "no such element".into() });
    };
    let start = floor_char_boundary(&element.text, start);
    let end = floor_char_boundary(&element.text, end.max(start));
    let kind = element.kind;
    let before = element.text.get(..start).unwrap_or_default().to_string();
    let after = element.text.get(end..).unwrap_or_default().to_string();
    // The text alone, never a title page: a pasted `Title: …` line is action.
    let parsed = storyteller_core::fountain::parse(&format!("\n{}", text.replace("\r\n", "\n").trim()))
        .map_err(|e| CommandError::Failed(format!("the pasted text could not be read: {e}")))?;
    let mut pasted: Vec<Element> = parsed.elements.into_iter().filter(|e| e.kind != ElementKind::PageBreak || !e.text.is_empty()).collect();
    if pasted.is_empty() {
        return Ok(0);
    }
    if s.script.elements.len().saturating_add(pasted.len()) > storyteller_core::MAX_ELEMENTS {
        return Err(CommandError::Failed("the script has too many elements".into()));
    }
    s.checkpoint(EditKind::Other);
    let count = pasted.len();
    let mut at = index;
    if before.trim().is_empty() {
        // An empty line (or a paste at its start) takes the first pasted element in its place.
        s.script.elements.remove(index);
    } else {
        if let Some(element) = s.script.elements.get_mut(index) {
            element.set_text(before.trim_end().to_string());
        }
        at = index + 1;
    }
    if !after.trim().is_empty() {
        let mut rest = Element::new(kind, after.trim_start());
        rest.normalize();
        pasted.push(rest);
    }
    let last = at + count - 1;
    let caret = pasted.get(count - 1).map_or(0, |e| e.text.len());
    for (n, element) in pasted.into_iter().enumerate() {
        s.script.elements.insert(at + n, element);
    }
    s.caret = Caret { element: last, offset: caret };
    Ok(count)
}

fn new_element(kind: ElementKind) -> (Element, usize) {
    if kind == ElementKind::Parenthetical { (Element::new(kind, "()"), 1) } else { (Element::new(kind, ""), 0) }
}

/// What Enter did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Entered {
    /// Ended the element and started the next one.
    Next,
    /// The element is empty: the app offers the element picker (Final Draft's behaviour).
    Picker,
}

/// Enter at the caret: Final Draft element flow.
pub fn enter(session: &mut Session, index: usize, offset: usize) -> Result<Entered, CommandError> {
    let Some(element) = session.script.elements.get(index).cloned() else {
        return Err(CommandError::Invalid { name: "index", reason: "no such element".into() });
    };
    let rule = flow::rule_for(element.kind);
    if flow::is_empty(element.kind, &element.text) {
        return Ok(Entered::Picker);
    }
    session.checkpoint(EditKind::Other);
    let offset = floor_char_boundary(&element.text, offset);
    if offset == 0 {
        // At the start of a paragraph: open an empty one of the same type above it.
        insert(session, index, Element::new(element.kind, ""))?;
        session.caret = Caret { element: index + 1, offset: 0 };
        return Ok(Entered::Next);
    }
    let mut head = element.text.get(..offset).unwrap_or("").to_string();
    let mut tail = element.text.get(offset..).unwrap_or("").trim_start().to_string();
    // A caret just inside a parenthetical's closing bracket ends the parenthetical.
    if element.kind == ElementKind::Parenthetical && tail.trim() == ")" {
        head.push(')');
        tail.clear();
    }
    let next_kind = rule.enter;
    let (mut fresh, mut caret) = new_element(next_kind);
    if let Some(current) = session.script.elements.get_mut(index) {
        // Revised text and notes after the caret go with the text to the new paragraph.
        // The new paragraph starts after the spaces Enter drops.
        let dropped = element.text.get(offset..).unwrap_or("").chars().take_while(|c| c.is_whitespace()).count();
        let at = u32::try_from(inline::plain(element.text.get(..offset).unwrap_or("")).chars().count() + dropped).unwrap_or(u32::MAX);
        current.split_anchors(at, &mut fresh);
        current.set_text(head.trim_end().to_string());
    }
    if !tail.is_empty() {
        let (text, _) = flow::convert_text(&tail, element.kind, next_kind);
        fresh.text = text;
        caret = usize::from(next_kind == ElementKind::Parenthetical);
    }
    insert(session, index + 1, fresh)?;
    session.caret = Caret { element: index + 1, offset: caret };
    Ok(Entered::Next)
}

/// Start an element of `kind` right after `index`, with the caret in it.
fn start_after(session: &mut Session, index: usize, kind: ElementKind) -> Result<(), CommandError> {
    session.checkpoint(EditKind::Other);
    let (fresh, caret) = new_element(kind);
    insert(session, index + 1, fresh)?;
    session.caret = Caret { element: index + 1, offset: caret };
    Ok(())
}

/// Tab: an empty element changes type; after a non-empty one the next element starts
/// (a parenthetical after a cue or a speech, a cue after action).
pub fn tab(session: &mut Session, index: usize) -> Result<(), CommandError> {
    let Some(element) = session.script.elements.get(index) else {
        return Err(CommandError::Invalid { name: "index", reason: "no such element".into() });
    };
    let rule = flow::rule_for(element.kind);
    if flow::is_empty(element.kind, &element.text) {
        set_kind(session, index, rule.tab_empty)?;
        let (fresh, caret) = new_element(rule.tab_empty);
        if let Some(element) = session.script.elements.get_mut(index) {
            element.set_text(fresh.text);
        }
        session.caret = Caret { element: index, offset: caret };
        return Ok(());
    }
    start_after(session, index, rule.tab_next)
}

/// ⌘1–⌘7: an empty element becomes that type; otherwise an element of that type starts after it.
pub fn element_shortcut(session: &mut Session, index: usize, kind: ElementKind) -> Result<(), CommandError> {
    let Some(element) = session.script.elements.get(index) else {
        return Err(CommandError::Invalid { name: "index", reason: "no such element".into() });
    };
    if flow::is_empty(element.kind, &element.text) {
        set_kind(session, index, kind)?;
        if kind == ElementKind::Parenthetical && session.element(index).is_some_and(|e| e.text.trim().is_empty()) {
            if let Some(element) = session.script.elements.get_mut(index) {
                element.set_text("()");
            }
            session.caret = Caret { element: index, offset: 1 };
        }
        return Ok(());
    }
    start_after(session, index, kind)
}

/// Backspace at the start of an element: merge it into the previous one.
pub fn merge_previous(session: &mut Session, index: usize) -> Result<bool, CommandError> {
    if index == 0 || index >= session.script.elements.len() {
        return Ok(false);
    }
    session.checkpoint(EditKind::Other);
    let mut current = session.script.elements.remove(index);
    let previous_index = index - 1;
    let Some(previous) = session.script.elements.get_mut(previous_index) else { return Ok(false) };
    if previous.kind == ElementKind::PageBreak {
        session.script.elements.remove(previous_index);
        session.script.elements.insert(previous_index, current);
        session.caret = Caret { element: previous_index, offset: 0 };
        return Ok(true);
    }
    let mut extra = std::mem::take(&mut current.text);
    if previous.kind == ElementKind::Parenthetical || current.kind == ElementKind::Parenthetical {
        extra = extra.trim_matches(['(', ')']).to_string();
    }
    let join_at = previous.text.trim_end_matches(')').len().min(previous.text.len());
    let at = if previous.kind == ElementKind::Parenthetical { join_at } else { previous.text.len() };
    let mut text = previous.text.clone();
    if previous.kind.is_caps() {
        extra = extra.to_uppercase();
    }
    let at = floor_char_boundary(&text, at);
    text.insert_str(at, &extra);
    previous.set_text(if previous.kind.is_single_line() { text.replace('\n', " ") } else { text });
    // The joined paragraph's revised text and notes come along, where its text now sits.
    let joined_at = u32::try_from(inline::plain(previous.text.get(..at).unwrap_or_default()).chars().count()).unwrap_or(u32::MAX);
    previous.absorb_anchors(&mut current, joined_at);
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

fn all_commands() -> Vec<Command> {
    use ParamType::*;
    const INDEX: ParamSpec = param("index", Integer, false, "Element index (default: the caret's element)");
    const OFFSET: ParamSpec = param("offset", Integer, false, "Byte offset in the element text (default: the caret)");
    const FIND_TEXT: ParamSpec = param("text", String, true, "Text to find");
    const FIND_CASE: ParamSpec = param("case", Bool, false, "Match case exactly");
    const FIND_WORD: ParamSpec = param("whole_word", Bool, false, "Whole words only");
    const FIND_KIND: ParamSpec = param("kind", Kind, false, "Only in elements of this type");
    const FIND_REPLACEMENT: ParamSpec = param("replacement", String, true, "Replacement text");
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
            id: "file.new_tv",
            label: "New TV Script…",
            menu: "File/New TV Script…",
            shortcut: None,
            params: const { &[param("shape", String, false, "one_hour (Teaser, Acts One to Five, Tag) or half_hour (Cold Open, Acts One to Three, Tag)")] },
            enabled: always,
            run: |s, p| {
                let shape = match opt_string(p, "shape")? {
                    Some(id) => storyteller_core::model::TvShape::from_id(&id)
                        .ok_or_else(|| CommandError::Invalid { name: "shape", reason: "one_hour or half_hour".into() })?,
                    None => storyteller_core::model::TvShape::default(),
                };
                *s = Session::new(Script::tv(shape));
                // The caret in the first act's scene heading, ready to write.
                s.caret = Caret { element: 1, offset: 0 };
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
                let (script, format) = files::open(&path)?;
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
                files::save(&s.script, &path, s.format, &s.layout)?;
                s.dirty = false;
                s.break_typing();
                Ok(Outcome::message(files::saved_message("Saved", &path, &s.script, s.format)))
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
                files::save(&s.script, &path, format, &s.layout)?;
                s.path = Some(path.clone());
                s.format = format;
                s.dirty = false;
                s.break_typing();
                Ok(Outcome::message(files::saved_message("Saved", &path, &s.script, format)))
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
                files::save(&s.script, &path, format, &s.layout)?;
                Ok(Outcome::message(files::saved_message("Exported", &path, &s.script, format)))
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
            id: "nav.go_to",
            label: "Go To",
            menu: "",
            shortcut: None,
            params: const {
                &[
                    param("scene", String, false, "Scene number as printed (12, 12A), or its position when scenes are unnumbered"),
                    param("page", Integer, false, "Page number"),
                    param("act", String, false, "Act: its position counted from 1, or its name (TAG, ACT TWO, two)"),
                ]
            },
            enabled: always,
            run: |s, p| {
                let index = match opt_string(p, "act")? {
                    Some(act) => s
                        .script
                        .find_act(&act)
                        .ok_or_else(|| CommandError::Invalid { name: "act", reason: format!("there is no act {}", act.trim()) })?,
                    None => go_to(s, opt_string(p, "scene")?, opt_index(p, "page")?)?,
                };
                s.caret = Caret { element: index, offset: 0 };
                s.break_typing();
                Ok(Outcome::value(json!({ "element": index })))
            },
        },
        Command {
            id: "find.next",
            label: "Find Next",
            menu: "",
            shortcut: None,
            params: const {
                &[FIND_TEXT, FIND_CASE, FIND_WORD, FIND_KIND, param("backwards", Bool, false, "Find the previous match instead"), INDEX, OFFSET]
            },
            enabled: always,
            run: |s, p| {
                let query = find_query(p)?;
                let hits = find::find_all(&s.script, &query);
                let element = opt_index(p, "index")?.unwrap_or(s.caret.element);
                let offset = opt_index(p, "offset")?.unwrap_or(s.caret.offset);
                let backwards = opt_bool(p, "backwards")?.unwrap_or(false);
                let Some((number, hit)) = find::find_next(&hits, element, offset, backwards) else {
                    return Ok(Outcome { message: Some(format!("“{}” is not in the script", query.text)), value: json!({ "total": 0 }) });
                };
                // Forward, the caret goes after the match; backwards, before it, so repeating moves on.
                s.caret = Caret { element: hit.element, offset: if backwards { hit.start } else { hit.end } };
                s.break_typing();
                Ok(Outcome::value(json!({ "element": hit.element, "start": hit.start, "end": hit.end, "number": number + 1, "total": hits.len() })))
            },
        },
        Command {
            id: "find.replace",
            label: "Replace",
            menu: "",
            shortcut: None,
            params: const {
                &[
                    FIND_TEXT,
                    FIND_CASE,
                    FIND_WORD,
                    FIND_KIND,
                    FIND_REPLACEMENT,
                    INDEX,
                    param("start", Integer, true, "Byte offset where the match to replace starts"),
                ]
            },
            enabled: always,
            run: |s, p| {
                let query = find_query(p)?;
                let replacement = string(p, "replacement")?;
                let index = element_index(s, p)?;
                let start = opt_index(p, "start")?.ok_or(CommandError::Missing("start"))?;
                let hits = find::find_all(&s.script, &query);
                let Some(hit) = hits.iter().find(|h| h.element == index && h.start == start) else {
                    return Ok(Outcome { message: Some("Nothing to replace there".into()), value: json!({ "replaced": false }) });
                };
                let text = s.script.elements.get(index).map(|e| e.text.clone()).unwrap_or_default();
                let new_text = format!("{}{replacement}{}", text.get(..hit.start).unwrap_or(""), text.get(hit.end..).unwrap_or(""));
                set_text(s, index, &new_text, false)?;
                s.caret =
                    Caret { element: index, offset: (hit.start + replacement.len()).min(s.script.elements.get(index).map_or(0, |e| e.text.len())) };
                Ok(Outcome::value(json!({ "replaced": true })))
            },
        },
        Command {
            id: "find.replace_all",
            label: "Replace All",
            menu: "",
            shortcut: None,
            params: const { &[FIND_TEXT, FIND_CASE, FIND_WORD, FIND_KIND, FIND_REPLACEMENT] },
            enabled: always,
            run: |s, p| {
                let query = find_query(p)?;
                let replacement = string(p, "replacement")?;
                let hits = find::find_all(&s.script, &query);
                if hits.is_empty() {
                    return Ok(Outcome { message: Some(format!("“{}” is not in the script", query.text)), value: json!({ "count": 0 }) });
                }
                // Work out every new text first: a replacement that changes nothing is no edit.
                let mut changed: Vec<(usize, std::string::String)> = Vec::new();
                for (index, element) in s.script.elements.iter().enumerate() {
                    let ranges: Vec<&find::Hit> = hits.iter().filter(|h| h.element == index).collect();
                    if ranges.is_empty() {
                        continue;
                    }
                    let mut text = element.text.clone();
                    for hit in ranges.iter().rev() {
                        text.replace_range(hit.start..hit.end, &replacement);
                    }
                    if element.kind.is_single_line() {
                        text = text.replace(['\n', '\r'], " ");
                    }
                    if element.kind.is_caps() {
                        text = text.to_uppercase();
                    }
                    if text != element.text {
                        changed.push((index, text));
                    }
                }
                if changed.is_empty() {
                    return Ok(Outcome { message: Some("Nothing to change".into()), value: json!({ "count": 0 }) });
                }
                // One undo step for the lot.
                s.checkpoint(EditKind::Other);
                for (index, text) in changed {
                    if let Some(element) = s.script.elements.get_mut(index) {
                        element.set_text(text);
                    }
                }
                s.clamp_caret();
                let count = hits.len();
                Ok(Outcome {
                    message: Some(format!("Replaced {count} {}", if count == 1 { "match" } else { "matches" })),
                    value: json!({ "count": count }),
                })
            },
        },
        Command {
            id: "character.rename",
            label: "Rename Character",
            menu: "",
            shortcut: None,
            params: const {
                &[
                    param("from", String, true, "The character's current name"),
                    param("to", String, true, "The new name"),
                    param("everywhere", Bool, false, "Also rename capitalised whole-word mentions in action, dialogue and other text"),
                    param("merge", Bool, false, "Allow renaming onto a name another character already has, merging the two"),
                ]
            },
            enabled: always,
            run: |s, p| {
                let from = string(p, "from")?.trim().to_uppercase();
                let to = string(p, "to")?.trim().to_string();
                if to.is_empty() {
                    return Err(CommandError::Invalid { name: "to", reason: "the new name is empty".into() });
                }
                if to.contains(['(', ')', '^']) {
                    return Err(CommandError::Invalid {
                        name: "to",
                        reason: "a name cannot contain ( ) or ^; extensions such as (V.O.) are kept as they are".into(),
                    });
                }
                let new_name = to.to_uppercase();
                let merge = opt_bool(p, "merge")?.unwrap_or(false);
                if new_name != from && !merge && character_names(&s.script).iter().any(|(n, _)| *n == new_name) {
                    return Err(CommandError::Invalid {
                        name: "to",
                        reason: format!("{new_name} is already a character; renaming would merge the two"),
                    });
                }
                let everywhere = opt_bool(p, "everywhere")?.unwrap_or(false);
                let is_cue = |e: &Element| e.kind == ElementKind::Character && e.character_name().to_uppercase() == from;
                if !s.script.elements.iter().any(is_cue) {
                    return Err(CommandError::Invalid { name: "from", reason: format!("no character is called {from}") });
                }
                let mention = find::Query { text: from.clone(), case: false, whole_word: true, kind: None };
                // One undo step for every cue and mention.
                s.checkpoint(EditKind::Other);
                let (mut cues, mut mentions) = (0, 0);
                for element in &mut s.script.elements {
                    if is_cue(element) {
                        // Keep the extensions: `NORA (V.O.)` becomes `EMMA (V.O.)`.
                        let name_len = element.character_name().len();
                        let start = element.text.len() - element.text.trim_start().len();
                        let rest = element.text.get(start + name_len..).unwrap_or("").to_string();
                        element.set_text(format!("{new_name}{rest}"));
                        cues += 1;
                    } else if everywhere && element.kind != ElementKind::Character {
                        // Names in prose are capitalised, so `will` is not WILL and `may` is not MAY.
                        let hits: Vec<(usize, usize)> = find::find_in(&element.text, &mention)
                            .into_iter()
                            .filter(|&(start, _)| element.text.get(start..).and_then(|t| t.chars().next()).is_some_and(char::is_uppercase))
                            .collect();
                        let mut text = element.text.clone();
                        for &(start, end) in hits.iter().rev() {
                            let found = text.get(start..end).unwrap_or("");
                            let name = name_like(found, &to);
                            text.replace_range(start..end, &name);
                        }
                        if element.kind.is_caps() {
                            text = text.to_uppercase();
                        }
                        element.set_text(text);
                        mentions += hits.len();
                    }
                }
                s.clamp_caret();
                let message = if everywhere {
                    format!("Renamed {from} in {cues} {} and {mentions} {}", plural(cues, "cue", "cues"), plural(mentions, "mention", "mentions"))
                } else {
                    format!("Renamed {from} in {cues} {}", plural(cues, "cue", "cues"))
                };
                Ok(Outcome { message: Some(message), value: json!({ "cues": cues, "mentions": mentions }) })
            },
        },
        Command {
            id: "character.list",
            label: "List Characters",
            menu: "",
            shortcut: None,
            params: &[],
            enabled: always,
            run: |s, _| Ok(Outcome::value(json!(character_names(&s.script)))),
        },
        Command {
            id: "spell.next",
            label: "Next Misspelling",
            menu: "",
            shortcut: None,
            params: const { &[param("backwards", Bool, false, "Find the previous one instead"), INDEX, OFFSET] },
            enabled: always,
            run: |s, p| {
                let misses = crate::spell::misspelled(&s.script, &s.spelling);
                let element = opt_index(p, "index")?.unwrap_or(s.caret.element);
                let offset = opt_index(p, "offset")?.unwrap_or(s.caret.offset);
                let backwards = opt_bool(p, "backwards")?.unwrap_or(false);
                let hits: Vec<find::Hit> = misses.iter().map(|m| find::Hit { element: m.element, start: m.start, end: m.end }).collect();
                let Some((number, hit)) = find::find_next(&hits, element, offset, backwards) else {
                    return Ok(Outcome { message: Some("No misspellings".into()), value: json!({ "total": 0 }) });
                };
                let word = misses.get(number).map(|m| m.word.clone()).unwrap_or_default();
                s.caret = Caret { element: hit.element, offset: if backwards { hit.start } else { hit.end } };
                s.break_typing();
                Ok(Outcome::value(json!({
                    "element": hit.element, "start": hit.start, "end": hit.end, "word": word,
                    "suggestions": crate::spell::suggestions(&word, 6), "number": number + 1, "total": hits.len(),
                })))
            },
        },
        Command {
            id: "spell.ignore",
            label: "Ignore Spelling",
            menu: "",
            shortcut: None,
            params: const { &[param("word", String, true, "The word to accept in this script (kept with the script)")] },
            enabled: always,
            run: |s, p| {
                let word = string(p, "word")?;
                let word = word.trim();
                let lower = word.to_lowercase();
                // Kept with the script as written, so it stays ignored when the file is saved and
                // reopened; a word already there in any case is not added again.
                if !s.script.spelling_ignored.iter().any(|w| w.to_lowercase() == lower) {
                    s.script.spelling_ignored.push(word.to_string());
                    s.dirty = true;
                }
                Ok(Outcome::message(format!("Ignoring “{word}” in this script")))
            },
        },
        Command {
            id: "spell.learn",
            label: "Learn Spelling",
            menu: "",
            shortcut: None,
            params: const { &[param("word", String, true, "The word to add to the writer's dictionary")] },
            enabled: always,
            run: |s, p| {
                let word = string(p, "word")?;
                s.spelling.learned.insert(word.to_lowercase());
                Ok(Outcome { message: Some(format!("Added “{word}” to your dictionary")), value: json!({ "learned": word.to_lowercase() }) })
            },
        },
        Command {
            id: "spell.change",
            label: "Change Spelling",
            menu: "",
            shortcut: None,
            params: const {
                &[
                    INDEX,
                    param("start", Integer, true, "Byte offset where the word starts"),
                    param("word", String, true, "The word as it is now (nothing changes if the text there differs)"),
                    param("replacement", String, true, "The new spelling"),
                ]
            },
            enabled: always,
            run: |s, p| {
                let index = element_index(s, p)?;
                let start = opt_index(p, "start")?.ok_or(CommandError::Missing("start"))?;
                let word = string(p, "word")?;
                let replacement = string(p, "replacement")?;
                let text = s.script.elements.get(index).map(|e| e.text.clone()).unwrap_or_default();
                let end = start.checked_add(word.len()).ok_or(CommandError::Invalid { name: "start", reason: "out of range".into() })?;
                if text.get(start..end) != Some(word.as_str()) {
                    return Ok(Outcome { message: Some("The word has changed since".into()), value: json!({ "changed": false }) });
                }
                let new_text = format!("{}{replacement}{}", text.get(..start).unwrap_or(""), text.get(end..).unwrap_or(""));
                set_text(s, index, &new_text, false)?;
                let caret = start.saturating_add(replacement.len());
                s.caret = Caret { element: index, offset: caret.min(s.script.elements.get(index).map_or(0, |e| e.text.len())) };
                Ok(Outcome::value(json!({ "changed": true })))
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
            id: "edit.paste_screenplay",
            label: "Paste Screenplay Text",
            menu: "",
            shortcut: None,
            params: const {
                &[
                    INDEX,
                    param("start", Integer, false, "Byte offset where the paste begins (default: the caret)"),
                    param("end", Integer, false, "Byte offset where the replaced selection ends (default: start)"),
                    param("text", String, true, "Screenplay text, read as Fountain"),
                ]
            },
            enabled: always,
            run: |s, p| {
                let index = element_index(s, p)?;
                let start = text_offset(s, p, index).and_then(|caret| Ok(opt_index(p, "start")?.unwrap_or(caret)))?;
                let end = opt_index(p, "end")?.unwrap_or(start);
                let pasted = paste_screenplay(s, index, start, end, &string(p, "text")?)?;
                Ok(Outcome::value(json!({ "elements": pasted })))
            },
        },
        Command {
            id: "edit.copy_scene",
            label: "Copy Scene",
            menu: "Edit/Copy Scene",
            shortcut: None,
            params: const { &[INDEX] },
            enabled: always,
            run: |s, p| {
                let index = element_index(s, p)?;
                let range = scene_range(&s.script, index);
                let count = range.len();
                let elements = s.script.elements.get(range).unwrap_or_default().to_vec();
                let text = storyteller_core::fountain::write(&Script { elements, ..Script::default() });
                Ok(Outcome { message: Some(format!("Copied the scene ({count} elements)")), value: json!({ "text": text }) })
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
        convert_command!(
            "convert.scene_heading",
            "Change to Scene Heading",
            "Format/Change to Scene Heading",
            CONVERT_KEYS[0],
            ElementKind::SceneHeading
        ),
        convert_command!("convert.action", "Change to Action", "Format/Change to Action", CONVERT_KEYS[1], ElementKind::Action),
        convert_command!("convert.character", "Change to Character", "Format/Change to Character", CONVERT_KEYS[2], ElementKind::Character),
        convert_command!(
            "convert.parenthetical",
            "Change to Parenthetical",
            "Format/Change to Parenthetical",
            CONVERT_KEYS[3],
            ElementKind::Parenthetical
        ),
        convert_command!("convert.dialogue", "Change to Dialogue", "Format/Change to Dialogue", CONVERT_KEYS[4], ElementKind::Dialogue),
        convert_command!("convert.transition", "Change to Transition", "Format/Change to Transition", CONVERT_KEYS[5], ElementKind::Transition),
        convert_command!("convert.shot", "Change to Shot", "Format/Change to Shot", CONVERT_KEYS[6], ElementKind::Shot),
        Command {
            id: "convert.new_act",
            label: "Change to New Act",
            menu: "Format/Change to New Act",
            shortcut: None,
            params: const { &[INDEX] },
            enabled: always,
            run: |s, p| {
                let index = element_index(s, p)?;
                set_kind(s, index, ElementKind::NewAct)?;
                Ok(Outcome::done())
            },
        },
        Command {
            id: "convert.end_of_act",
            label: "Change to End of Act",
            menu: "Format/Change to End of Act",
            shortcut: None,
            params: const { &[INDEX] },
            enabled: always,
            run: |s, p| {
                let index = element_index(s, p)?;
                set_kind(s, index, ElementKind::EndOfAct)?;
                Ok(Outcome::done())
            },
        },
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
                Ok(match enter(s, index, offset)? {
                    Entered::Next => Outcome::done(),
                    Entered::Picker => Outcome { message: Some("Choose an element".into()), value: serde_json::json!({ "picker": true }) },
                })
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
                tab(s, index)?;
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
    files::save(&session.script, &path, format, &session.layout)?;
    Ok(Outcome::message(files::saved_message("Exported", &path, &session.script, format)))
}
