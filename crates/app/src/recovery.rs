//! Recovery copies: while a script has unsaved changes, a copy goes into the app's Recovery
//! folder every minute, as Fountain, named for the script (`Pilot.recovery.fountain`). Saving
//! the script, or choosing Don't Save, takes its copy away; a copy still there at the next
//! launch means the app closed without either, and the writer is offered it.

use std::path::{Path, PathBuf};

const SUFFIX: &str = ".recovery.fountain";

/// The copy for a script named `stem` (`Untitled` for a script never saved).
pub fn copy_path(dir: &Path, stem: &str) -> PathBuf {
    dir.join(format!("{stem}{SUFFIX}"))
}

/// The script name a copy belongs to.
pub fn name_of(copy: &Path) -> String {
    copy.file_name().map(|n| n.to_string_lossy().trim_end_matches(SUFFIX).to_string()).unwrap_or_default()
}

/// Writes the copy for `stem`.
pub fn write(dir: &Path, stem: &str, bytes: &[u8]) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    craft_atomic::atomic_write(&copy_path(dir, stem), bytes)
}

/// Removes the copy for `stem`, if there is one.
pub fn forget(dir: &Path, stem: &str) {
    let _ = std::fs::remove_file(copy_path(dir, stem));
}

/// A name for a script's copy that no copy waiting in `dir` has: `stem`, or `stem 2`, `stem 3` …
/// so a new script's copies never write over one still waiting to be answered.
pub fn free_name(dir: &Path, stem: &str) -> String {
    if !copy_path(dir, stem).exists() {
        return stem.to_string();
    }
    (2..).map(|n| format!("{stem} {n}")).find(|name| !copy_path(dir, name).exists()).unwrap_or_else(|| stem.to_string())
}

/// The copies waiting in `dir`, newest first.
pub fn waiting(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut copies: Vec<(std::time::SystemTime, PathBuf)> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.file_name().is_some_and(|n| n.to_string_lossy().ends_with(SUFFIX)) && p.is_file())
        .map(|p| (std::fs::metadata(&p).and_then(|m| m.modified()).unwrap_or(std::time::UNIX_EPOCH), p))
        .collect();
    copies.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
    copies.into_iter().map(|(_, p)| p).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copies_are_written_found_and_forgotten() {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path().join("Recovery");
        assert!(waiting(&folder).is_empty(), "no folder, nothing waiting");
        write(&folder, "Pilot", b"INT. HUT - DAY\n").unwrap();
        std::fs::write(folder.join("notes.txt"), "not a copy").unwrap();
        let found = waiting(&folder);
        assert_eq!(found.len(), 1);
        assert_eq!(name_of(&found[0]), "Pilot");
        assert_eq!(free_name(&folder, "Pilot"), "Pilot 2", "the waiting copy's name is taken");
        assert_eq!(free_name(&folder, "Untitled"), "Untitled");
        forget(&folder, "Pilot");
        assert!(waiting(&folder).is_empty());
        assert_eq!(free_name(&folder, "Pilot"), "Pilot");
        forget(&folder, "Pilot");
    }
}
