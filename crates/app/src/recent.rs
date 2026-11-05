//! The recent scripts, kept in `recent.json` in the data folder, as every app in the suite keeps
//! them, so Method Launcher can list them (SUITE-STANDARDS.md §2):
//! `[{"path": "/absolute/path", "opened": "<RFC 3339 time>"}]`, newest first.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// The file's name, inside the data folder.
pub const FILE: &str = "recent.json";
/// Recent scripts kept.
pub const MAX: usize = 10;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct Entry {
    path: String,
    opened: String,
    /// Keys another version or app added, kept as they were.
    #[serde(flatten)]
    other: Map<String, Value>,
}

/// The list, and where it is saved (nowhere in tests).
#[derive(Debug, Default)]
pub struct Recent {
    file: Option<PathBuf>,
    entries: Vec<Entry>,
}

impl Recent {
    /// Reads `recent.json` from `data`. When there is none yet, the list older versions kept in
    /// the settings (`legacy`, newest first) becomes the first one, written straight away.
    pub fn open(data: Option<&Path>, legacy: &[String]) -> Self {
        let file = data.map(|d| d.join(FILE));
        let read = file.as_deref().and_then(|f| std::fs::read_to_string(f).ok());
        let mut recent = Recent { file, entries: Vec::new() };
        match read {
            // A file that does not parse is started again rather than failing the launch.
            Some(text) => recent.entries = serde_json::from_str::<Vec<Entry>>(&text).unwrap_or_default(),
            None if !legacy.is_empty() => {
                let now = storyteller_engine::commands::now_rfc3339();
                recent.entries = legacy.iter().take(MAX).map(|path| Entry { path: absolute(path), opened: now.clone(), other: Map::new() }).collect();
                recent.save();
            }
            None => {}
        }
        recent.entries.truncate(MAX);
        recent
    }

    /// The paths, newest first.
    pub fn paths(&self) -> impl Iterator<Item = &str> {
        self.entries.iter().map(|e| e.path.as_str())
    }

    pub fn get(&self, n: usize) -> Option<&str> {
        self.entries.get(n).map(|e| e.path.as_str())
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// `path` opened or saved now: first in the list, once.
    pub fn push(&mut self, path: &str) {
        let path = absolute(path);
        let other = self.entries.iter().position(|e| e.path == path).map(|n| self.entries.remove(n).other).unwrap_or_default();
        self.entries.insert(0, Entry { path, opened: storyteller_engine::commands::now_rfc3339(), other });
        self.entries.truncate(MAX);
        self.save();
    }

    pub fn clear(&mut self) {
        self.entries.clear();
        self.save();
    }

    fn save(&self) {
        let Some(file) = &self.file else { return };
        let text = serde_json::to_string_pretty(&self.entries).unwrap_or_else(|_| "[]".into()) + "\n";
        if let Some(dir) = file.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Err(error) = storyteller_engine::files::save_text(file, &text) {
            eprintln!("could not save the recent scripts: {error}");
        }
    }
}

/// The path as an absolute one, as the format asks.
fn absolute(path: &str) -> String {
    std::path::absolute(path).map_or_else(|_| path.to_string(), |p| p.display().to_string())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;

    fn read(dir: &Path) -> Vec<Value> {
        serde_json::from_str(&std::fs::read_to_string(dir.join(FILE)).unwrap()).unwrap()
    }

    #[test]
    fn newest_first_once_each_in_the_suite_format() {
        let dir = tempfile::tempdir().unwrap();
        let mut recent = Recent::open(Some(dir.path()), &[]);
        assert!(recent.is_empty());
        assert!(!dir.path().join(FILE).exists(), "nothing written until there is something to keep");
        recent.push("/scripts/a.mstory");
        recent.push("/scripts/b.fountain");
        recent.push("/scripts/a.mstory");
        assert_eq!(recent.paths().collect::<Vec<_>>(), ["/scripts/a.mstory", "/scripts/b.fountain"]);
        let file = read(dir.path());
        assert_eq!(file.len(), 2);
        assert_eq!(file[0]["path"], "/scripts/a.mstory");
        let opened = file[0]["opened"].as_str().unwrap();
        assert_eq!(opened.len(), 20, "{opened}");
        assert!(opened.ends_with('Z') && opened.as_bytes()[10] == b'T', "{opened}");
        assert_eq!(file[0].as_object().unwrap().len(), 2, "only path and opened");
        for n in 0..12 {
            recent.push(&format!("/scripts/{n}.fountain"));
        }
        assert_eq!(read(dir.path()).len(), MAX);
        recent.clear();
        assert_eq!(read(dir.path()).len(), 0);
    }

    #[test]
    fn the_settings_list_moves_over_once() {
        let dir = tempfile::tempdir().unwrap();
        let legacy = ["/scripts/new.mstory".to_string(), "/scripts/old.fdx".to_string()];
        let recent = Recent::open(Some(dir.path()), &legacy);
        assert_eq!(recent.paths().collect::<Vec<_>>(), ["/scripts/new.mstory", "/scripts/old.fdx"]);
        assert_eq!(read(dir.path()).len(), 2, "written straight away");
        // Once the file exists, the settings list is not read again.
        let mut again = Recent::open(Some(dir.path()), &["/scripts/stale.fountain".to_string()]);
        assert_eq!(again.paths().count(), 2);
        again.clear();
        assert!(Recent::open(Some(dir.path()), &legacy).is_empty(), "a cleared list stays cleared");
    }

    #[test]
    fn keys_from_elsewhere_are_kept_and_paths_made_absolute() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(FILE), r#"[{"path": "/scripts/a.fdx", "opened": "2026-11-01T09:00:00Z", "bookmark": "abc"}]"#).unwrap();
        let mut recent = Recent::open(Some(dir.path()), &[]);
        recent.push("/scripts/a.fdx");
        recent.push("relative.fountain");
        let file = read(dir.path());
        assert_eq!(file[1]["bookmark"], "abc");
        assert_ne!(file[1]["opened"], "2026-11-01T09:00:00Z", "opened again now");
        assert!(Path::new(file[0]["path"].as_str().unwrap()).is_absolute());
        // A damaged file is started again.
        std::fs::write(dir.path().join(FILE), "not json").unwrap();
        assert!(Recent::open(Some(dir.path()), &[]).is_empty());
    }
}
