//! File > Export for Method Schedule… (docs/production-spec.md §4.4): the script's scenes as a
//! script scenes file, `.mscenes`, with this app's own pages and lengths, for Method Schedule's
//! File > Import Script.
//!
//! Version 1 of `method-script-scenes` is frozen once it ships: fields may be added, never
//! removed, renamed or given a new meaning.

use serde::Serialize;
use storyteller_core::ElementKind;
use storyteller_core::inline::plain;

use crate::Session;
use crate::reports::{self, SceneRow};

/// The file's `format`.
pub const FORMAT: &str = "method-script-scenes";
/// Its `version`.
pub const VERSION: u32 = 1;
/// Its extension.
pub const EXTENSION: &str = "mscenes";

/// The file, its keys in the agreed order. An optional value the script does not have is left
/// out, never written as null.
#[derive(Debug, Serialize)]
struct File {
    format: &'static str,
    version: u32,
    title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    episode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    written_by: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    draft: Option<String>,
    generated: String,
    source_hash: String,
    pages: usize,
    eighths: u32,
    scenes: Vec<Scene>,
}

#[derive(Debug, Serialize)]
struct Scene {
    index: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    number: Option<String>,
    heading: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    int_ext: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    set: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    time: Option<String>,
    page: String,
    eighths: u32,
    cast: Vec<String>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    omitted: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    summary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    color: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    act: Option<String>,
}

/// A title-page field as one line of plain text, or nothing when it is empty.
fn field(session: &Session, key: &str) -> Option<String> {
    let value = session.script.title_page.get(key)?;
    let lines: Vec<String> = value.lines().map(plain).map(|l| l.trim().to_string()).filter(|l| !l.is_empty()).collect();
    Some(lines.join(", ")).filter(|v| !v.is_empty())
}

fn text(value: &str) -> Option<String> {
    Some(plain(value).trim().to_string()).filter(|v| !v.is_empty())
}

/// The act each scene heading is in: the last New Act line before it, until its End of Act.
fn acts(session: &Session) -> Vec<(usize, Option<String>)> {
    let mut act = None;
    let mut out = Vec::new();
    for (element, item) in session.script.elements.iter().enumerate() {
        match item.kind {
            ElementKind::NewAct => act = text(&item.text),
            ElementKind::EndOfAct => act = None,
            ElementKind::SceneHeading => out.push((element, act.clone())),
            _ => {}
        }
    }
    out
}

fn scene(session: &Session, row: SceneRow, act: Option<String>) -> Scene {
    let meta = session.script.elements.get(row.element).map(|e| e.scene.clone()).unwrap_or_default();
    // Without INT. or EXT. the heading is not split here: Method Schedule splits it as its
    // Fountain import does.
    let split = row.int_ext.is_some();
    Scene {
        index: row.index,
        number: row.number.filter(|n| !n.trim().is_empty()),
        int_ext: row.int_ext,
        set: Some(row.location).filter(|l| split && !l.is_empty()),
        time: row.time.filter(|_| split),
        page: row.page,
        // An omitted scene is one eighth with nobody in it, as the format says.
        eighths: if row.omitted { 1 } else { row.eighths.max(1) },
        cast: if row.omitted { Vec::new() } else { row.characters },
        omitted: row.omitted,
        title: text(&meta.title),
        summary: text(&meta.summary),
        color: meta.rgb().map(|(r, g, b)| format!("#{r:02X}{g:02X}{b:02X}")),
        act,
        heading: row.heading,
    }
}

/// 64-bit FNV-1a, as 16 hex digits.
fn digest(bytes: &[u8]) -> String {
    let hash = bytes.iter().fold(0xcbf2_9ce4_8422_2325_u64, |hash, &b| (hash ^ u64::from(b)).wrapping_mul(0x0000_0100_0000_01b3));
    format!("{hash:016x}")
}

fn pretty(file: &File) -> String {
    let mut out = serde_json::to_string_pretty(file).unwrap_or_default();
    out.push('\n');
    out
}

/// The session's script as a script scenes file, written on `generated` (`YYYY-MM-DD`).
pub fn write(session: &mut Session, generated: &str) -> String {
    let rows = reports::session_scenes(session);
    let pages = session.paginated().page_count();
    let acts = acts(session);
    let scenes: Vec<Scene> = rows
        .into_iter()
        .map(|row| {
            let act = acts.iter().find(|(element, _)| *element == row.element).and_then(|(_, act)| act.clone());
            scene(session, row, act)
        })
        .collect();
    let mut file = File {
        format: FORMAT,
        version: VERSION,
        title: field(session, "Title").unwrap_or_default(),
        episode: field(session, "Episode"),
        written_by: field(session, "Author"),
        draft: field(session, "Draft").or_else(|| field(session, "Draft date")),
        generated: String::new(),
        source_hash: String::new(),
        pages,
        eighths: scenes.iter().map(|s| s.eighths).sum(),
        scenes,
    };
    // The digest is of the script and of everything the file says but the date it was
    // written, so the same script, paged the same way, always gives the same hash, and any
    // edit, even one that leaves every length alone, gives a new one.
    let mut content = serde_json::to_vec(&session.script).unwrap_or_default();
    content.extend_from_slice(pretty(&file).as_bytes());
    file.source_hash = digest(&content);
    file.generated = generated.to_string();
    pretty(&file)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_digest_is_fnv_1a() {
        assert_eq!(digest(b""), "cbf29ce484222325");
        assert_eq!(digest(b"a"), "af63dc4c8601ec8c");
    }
}
