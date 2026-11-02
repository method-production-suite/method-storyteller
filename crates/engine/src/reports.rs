//! Production > Reports (docs/production-spec.md §4.3): scenes, locations and cast, each as rows
//! for the window, the control protocol and CSV, and the script as plain text.

use serde::Serialize;
use storyteller_core::autocomplete::split_heading;
use storyteller_core::inline::plain;
use storyteller_core::paginate::{Paginated, SceneLength, format_eighths};
use storyteller_core::{ElementKind, Script};

use crate::Session;
use crate::commands::{CharacterRow, character_table};

/// One scene of the scene report.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct SceneRow {
    /// Its heading element.
    pub element: usize,
    /// Its position among the scenes, from 1.
    pub index: usize,
    /// As printed, such as `12A`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub number: Option<String>,
    pub heading: String,
    /// `INT`, `EXT` or `INT/EXT`, when the heading says.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub int_ext: Option<&'static str>,
    pub location: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub time: Option<String>,
    /// The page it starts on, as printed: `12`, or `12A` on a locked page.
    pub page: String,
    /// Its length in eighths of a page; `0` for an omitted scene.
    pub eighths: u32,
    /// Speaking characters, in the order they first speak, without extensions.
    pub characters: Vec<String>,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub title: String,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub omitted: bool,
}

impl SceneRow {
    /// The scene as a report names it: its number, or its position when it has none.
    pub fn label(&self) -> String {
        self.number.clone().unwrap_or_else(|| self.index.to_string())
    }
}

/// One location of the location report. Omitted scenes are not counted.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct LocationRow {
    pub location: String,
    pub scenes: usize,
    /// Pages in eighths, all its scenes together.
    pub eighths: u32,
    /// Scenes by the heading's INT/EXT: interior, exterior, both, and neither said.
    pub int: usize,
    pub ext: usize,
    pub int_ext: usize,
    pub unmarked: usize,
}

/// One character of the cast report: the scenes they speak in, by label.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct CastRow {
    pub name: String,
    pub scenes: Vec<String>,
}

/// INT, EXT or INT/EXT from a heading's prefix. An establishing shot is exterior.
fn int_ext(prefix: Option<&str>) -> Option<&'static str> {
    let prefix = prefix?.to_uppercase();
    match prefix.trim_end_matches('.') {
        "INT" => Some("INT"),
        "EXT" | "EST" => Some("EXT"),
        _ => Some("INT/EXT"),
    }
}

/// A heading with its scene number typed in front, such as `12. INT. HUT - DAY`, from the INT.
/// on; any other heading as it is.
fn after_typed_number(heading: &str) -> &str {
    let rest = heading.trim_start_matches(|c: char| c.is_ascii_digit());
    if rest.len() == heading.len() {
        return heading;
    }
    let rest = rest.trim_start_matches(['.', '-', ' ']);
    if split_heading(rest).0.is_some() { rest } else { heading }
}

/// Every scene, in script order, with its page and length from `lengths`.
pub fn scenes(script: &Script, paginated: &Paginated, lengths: &[SceneLength]) -> Vec<SceneRow> {
    let label = |page: u32| {
        usize::try_from(page)
            .ok()
            .and_then(|p| p.checked_sub(1))
            .and_then(|p| paginated.pages.get(p))
            .map_or_else(|| page.to_string(), storyteller_core::paginate::Page::label)
    };
    let mut rows: Vec<SceneRow> = Vec::new();
    for (element, item) in script.elements.iter().enumerate() {
        match item.kind {
            ElementKind::SceneHeading => {
                let heading = plain(&item.text).trim().to_string();
                // Without INT. or EXT. there is no telling a location from a time of day: the
                // whole heading is the location.
                let (prefix, location, time) = match split_heading(after_typed_number(&heading)) {
                    (None, ..) => (None, heading.as_str(), None),
                    parts => parts,
                };
                let length = lengths.iter().find(|l| l.element == element);
                let page = length.map(|l| l.page).unwrap_or_else(|| paginated.element_pages.get(element).copied().unwrap_or(0));
                rows.push(SceneRow {
                    element,
                    index: rows.len() + 1,
                    number: item.scene_number.clone(),
                    int_ext: int_ext(prefix),
                    // `I/E.` leaves its full stop behind.
                    location: location.trim_start_matches(['.', ' ']).to_uppercase(),
                    time: time.filter(|t| !t.is_empty()).map(str::to_uppercase),
                    page: label(page),
                    eighths: length.map_or(0, |l| l.eighths),
                    characters: Vec::new(),
                    title: item.scene.title.clone(),
                    omitted: item.omitted.is_some(),
                    heading,
                });
            }
            ElementKind::Character => {
                let name = item.character_name().to_uppercase();
                if let Some(row) = rows.last_mut().filter(|r| !r.omitted && !name.is_empty() && !r.characters.contains(&name)) {
                    row.characters.push(name);
                }
            }
            _ => {}
        }
    }
    rows
}

/// The scene report for a session's script, from its cached pagination.
pub fn session_scenes(session: &mut Session) -> Vec<SceneRow> {
    let lengths = session.scene_lengths().to_vec();
    let paginated = session.paginated().clone();
    scenes(&session.script, &paginated, &lengths)
}

/// A report as CSV by name: `scenes`, `locations`, `characters` or `cast`.
pub fn csv_for(session: &mut Session, report: &str) -> Option<String> {
    Some(match report {
        "scenes" => scenes_csv(&session_scenes(session)),
        "locations" => locations_csv(&locations(&session_scenes(session))),
        "cast" => cast_csv(&cast(&session_scenes(session))),
        "characters" => characters_csv(&character_table(&session.script)),
        _ => return None,
    })
}

/// Every location, by name, from the scene report.
pub fn locations(scenes: &[SceneRow]) -> Vec<LocationRow> {
    let mut rows: Vec<LocationRow> = Vec::new();
    for scene in scenes.iter().filter(|s| !s.omitted) {
        let at = match rows.iter().position(|r| r.location == scene.location) {
            Some(at) => at,
            None => {
                rows.push(LocationRow { location: scene.location.clone(), scenes: 0, eighths: 0, int: 0, ext: 0, int_ext: 0, unmarked: 0 });
                rows.len() - 1
            }
        };
        let Some(row) = rows.get_mut(at) else { continue };
        row.scenes += 1;
        row.eighths += scene.eighths;
        match scene.int_ext {
            Some("INT") => row.int += 1,
            Some("EXT") => row.ext += 1,
            Some(_) => row.int_ext += 1,
            None => row.unmarked += 1,
        }
    }
    rows.sort_by(|a, b| a.location.cmp(&b.location));
    rows
}

/// Every speaking character, in the order they first speak, with the scenes they speak in.
pub fn cast(scenes: &[SceneRow]) -> Vec<CastRow> {
    let mut rows: Vec<CastRow> = Vec::new();
    for scene in scenes {
        for name in &scene.characters {
            match rows.iter_mut().find(|r| r.name == *name) {
                Some(row) => row.scenes.push(scene.label()),
                None => rows.push(CastRow { name: name.clone(), scenes: vec![scene.label()] }),
            }
        }
    }
    rows
}

/// Characters a spreadsheet may read a cell's formula from, when the cell starts with one:
/// `=`, `+`, `-`, `@` (and their full-width forms), tab and carriage return.
const FORMULA_STARTS: [char; 10] = ['=', '+', '-', '@', '\t', '\r', '\u{ff1d}', '\u{ff0b}', '\u{ff0d}', '\u{ff20}'];

/// One CSV field, quoted when it has to be. Script text that starts like a formula gets a
/// single quote in front, so opening the report never runs it.
fn field(text: &str) -> String {
    let text = if text.starts_with(FORMULA_STARTS) { format!("'{text}") } else { text.to_string() };
    if text.contains([',', '"', '\n', '\r']) { format!("\"{}\"", text.replace('"', "\"\"")) } else { text }
}

/// A CSV table: a header row, then the rows, each line ending in CRLF.
pub fn csv(header: &[&str], rows: impl IntoIterator<Item = Vec<String>>) -> String {
    let mut out = String::new();
    let mut line = |fields: Vec<String>| {
        out.push_str(&fields.iter().map(|f| field(f)).collect::<Vec<_>>().join(","));
        out.push_str("\r\n");
    };
    line(header.iter().map(|h| (*h).to_string()).collect());
    for row in rows {
        line(row);
    }
    out
}

pub fn scenes_csv(rows: &[SceneRow]) -> String {
    let header = ["Scene", "Heading", "INT/EXT", "Location", "Time", "Page", "Length", "Eighths", "Characters", "Title", "Omitted"];
    csv(
        &header,
        rows.iter().map(|r| {
            vec![
                r.label(),
                r.heading.clone(),
                r.int_ext.unwrap_or_default().to_string(),
                r.location.clone(),
                r.time.clone().unwrap_or_default(),
                r.page.clone(),
                format_eighths(r.eighths),
                r.eighths.to_string(),
                r.characters.join(", "),
                r.title.clone(),
                if r.omitted { "yes".into() } else { String::new() },
            ]
        }),
    )
}

pub fn locations_csv(rows: &[LocationRow]) -> String {
    let header = ["Location", "Scenes", "Pages", "Eighths", "INT", "EXT", "INT/EXT", "Unmarked"];
    csv(
        &header,
        rows.iter().map(|r| {
            let counts = [r.scenes, r.int, r.ext, r.int_ext, r.unmarked].map(|n| n.to_string());
            let [scenes, int, ext, both, unmarked] = counts;
            vec![r.location.clone(), scenes, format_eighths(r.eighths), r.eighths.to_string(), int, ext, both, unmarked]
        }),
    )
}

pub fn cast_csv(rows: &[CastRow]) -> String {
    csv(&["Character", "Scenes", "Scene list"], rows.iter().map(|r| vec![r.name.clone(), r.scenes.len().to_string(), r.scenes.join(", ")]))
}

pub fn characters_csv(rows: &[CharacterRow]) -> String {
    let header = ["Character", "Speeches", "Scenes", "Words", "First scene", "Scene list"];
    csv(
        &header,
        rows.iter().map(|r| {
            vec![
                r.name.clone(),
                r.speeches.to_string(),
                r.scenes.to_string(),
                r.words.to_string(),
                r.first_scene.clone().unwrap_or_default(),
                r.scene_list.join(", "),
            ]
        }),
    )
}

/// The script as it reads on the page, with no markup: the title page and every page, the text
/// where it sits on the page less the common left margin, pages separated by a form feed.
pub fn plain_text(paginated: &Paginated) -> String {
    let pages: Vec<&storyteller_core::paginate::Page> = paginated.title_page.iter().chain(paginated.pages.iter()).collect();
    let margin = pages.iter().flat_map(|p| &p.lines).filter(|l| !l.text().trim().is_empty()).map(|l| l.col).min().unwrap_or(0);
    let mut out = String::new();
    for (number, page) in pages.iter().enumerate() {
        if number > 0 {
            out.push('\u{c}');
        }
        let mut grid: Vec<String> = Vec::new();
        for line in &page.lines {
            let text = line.text();
            if text.trim().is_empty() {
                continue;
            }
            let row = usize::from(line.row).min(500);
            if grid.len() <= row {
                grid.resize(row + 1, String::new());
            }
            let Some(cells) = grid.get_mut(row) else { continue };
            let col = usize::from(line.col.saturating_sub(margin)).min(500);
            let mut chars: Vec<char> = cells.chars().collect();
            if chars.len() < col {
                chars.resize(col, ' ');
            }
            for (at, c) in (col..).zip(text.chars()) {
                match chars.get_mut(at) {
                    Some(cell) => *cell = c,
                    None => chars.push(c),
                }
            }
            *cells = chars.into_iter().collect();
        }
        let first = grid.iter().position(|r| !r.trim().is_empty()).unwrap_or(grid.len());
        for row in grid.iter().skip(first) {
            out.push_str(row.trim_end());
            out.push('\n');
        }
    }
    out
}
