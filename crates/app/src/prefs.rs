//! User preferences, stored with `craft-settings`, and the one-time move to the bundle-id folders.

use std::io;
use std::path::{Path, PathBuf};

use craft_settings::{DocumentMut, MigrateError, Probe, Settings};
use serde::{Deserialize, Serialize};

/// Names the settings and data folders (the bundle id, per the suite standard).
pub const APP_ID: &str = storyteller_engine::BUNDLE_ID;
/// Folder name 0.1.0 used for settings and recovery copies.
const LEGACY_APP_ID: &str = "method-storyteller";
/// Recovery copies, inside the data folder.
pub const RECOVERY_DIR: &str = "Recovery";
const LEGACY_RECOVERY_DIR: &str = "recovery";
/// Recent files kept.
pub const MAX_RECENT: usize = 10;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Prefs {
    pub general: General,
    pub view: View,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct General {
    pub recent: Vec<String>,
    /// The format new scripts are saved in: `fountain` or `osf`.
    pub new_script_format: String,
}

impl Default for General {
    fn default() -> Self {
        General { recent: Vec::new(), new_script_format: "fountain".into() }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct View {
    pub navigator: bool,
    pub scene_numbers: bool,
    pub zoom: f32,
    /// Show the script as separate pages (on) or as one continuous column (off).
    pub page_view: bool,
}

impl Default for View {
    fn default() -> Self {
        View { navigator: true, scene_numbers: true, zoom: 1.0, page_view: true }
    }
}

impl Settings for Prefs {
    const APP_ID: &'static str = APP_ID;
    const SCHEMA_VERSION: u32 = 1;

    fn migrate(_from: u32, _doc: &mut DocumentMut) -> Result<(), MigrateError> {
        Ok(())
    }
}

impl Prefs {
    pub fn push_recent(&mut self, path: &str) {
        self.general.recent.retain(|p| p != path);
        self.general.recent.insert(0, path.to_string());
        self.general.recent.truncate(MAX_RECENT);
    }

    pub fn zoom(&self) -> f32 {
        if self.view.zoom.is_finite() { self.view.zoom.clamp(0.5, 3.0) } else { 1.0 }
    }
}

/// Copies the 0.1.0 `method-storyteller` folders to the bundle-id folders on first launch.
///
/// Copies rather than moves, so an older build still finds its files after a rollback. Does
/// nothing once the new folder exists. Covers the platform folders only: a `<APP>_CONFIG_DIR`
/// or portable folder is the user's own choice and is left alone.
pub fn migrate_legacy_folders() {
    let config = Probe::default().platform_config_dir;
    let data = craft_settings::paths::data_dir(LEGACY_APP_ID).and_then(|d| d.parent().map(Path::to_path_buf));
    let mut bases: Vec<PathBuf> = config.into_iter().chain(data).collect();
    bases.dedup();
    for base in bases {
        if let Err(error) = migrate_folder(&base.join(LEGACY_APP_ID), &base.join(APP_ID)) {
            eprintln!("could not copy settings from {}: {error}", base.join(LEGACY_APP_ID).display());
        }
    }
}

/// Copies `old` to `new` when `new` does not exist yet, renaming `recovery/` to `Recovery/`.
/// Returns whether anything was copied.
fn migrate_folder(old: &Path, new: &Path) -> io::Result<bool> {
    if !old.is_dir() || new.exists() {
        return Ok(false);
    }
    // Copy into a sibling first, so a half-finished copy never looks like a finished one.
    let staging = staging_path(new);
    if staging.exists() {
        std::fs::remove_dir_all(&staging)?;
    }
    copy_tree(old, &staging, true)?;
    std::fs::rename(&staging, new)?;
    Ok(true)
}

fn staging_path(new: &Path) -> PathBuf {
    let mut name = new.file_name().unwrap_or_default().to_os_string();
    name.push(".migrating");
    new.with_file_name(name)
}

fn copy_tree(from: &Path, to: &Path, top: bool) -> io::Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let name = entry.file_name();
        let target = if top && name == LEGACY_RECOVERY_DIR { to.join(RECOVERY_DIR) } else { to.join(&name) };
        let kind = entry.file_type()?;
        if kind.is_dir() {
            copy_tree(&entry.path(), &target, false)?;
        } else if kind.is_file() {
            std::fs::copy(entry.path(), &target)?;
        }
    }
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    fn legacy(base: &Path) -> PathBuf {
        let old = base.join(LEGACY_APP_ID);
        std::fs::create_dir_all(old.join("recovery")).unwrap();
        std::fs::write(old.join("config.toml"), "schema_version = 1\n").unwrap();
        std::fs::write(old.join("recovery/Untitled.recovery.fountain"), "INT. KITCHEN - NIGHT\n").unwrap();
        old
    }

    #[test]
    fn copies_settings_and_renames_recovery() {
        let base = tempfile::tempdir().unwrap();
        let old = legacy(base.path());
        let new = base.path().join(APP_ID);
        assert!(migrate_folder(&old, &new).unwrap());
        assert_eq!(std::fs::read_to_string(new.join("config.toml")).unwrap(), "schema_version = 1\n");
        let names: Vec<_> = std::fs::read_dir(&new).unwrap().map(|e| e.unwrap().file_name()).collect();
        assert!(names.iter().any(|n| n == RECOVERY_DIR), "{names:?}");
        assert!(new.join(RECOVERY_DIR).join("Untitled.recovery.fountain").is_file());
        assert!(old.join("config.toml").is_file(), "the old folder stays for rollbacks");
        assert!(!staging_path(&new).exists());
    }

    #[test]
    fn leaves_an_existing_new_folder_alone() {
        let base = tempfile::tempdir().unwrap();
        let old = legacy(base.path());
        let new = base.path().join(APP_ID);
        std::fs::create_dir_all(&new).unwrap();
        std::fs::write(new.join("config.toml"), "newer").unwrap();
        assert!(!migrate_folder(&old, &new).unwrap());
        assert_eq!(std::fs::read_to_string(new.join("config.toml")).unwrap(), "newer");
    }

    #[test]
    fn does_nothing_without_an_old_folder() {
        let base = tempfile::tempdir().unwrap();
        let new = base.path().join(APP_ID);
        assert!(!migrate_folder(&base.path().join(LEGACY_APP_ID), &new).unwrap());
        assert!(!new.exists());
    }
}
