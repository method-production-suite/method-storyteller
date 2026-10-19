//! The sample script, Field Station, built into the app. The first launch with nothing to open
//! shows it once, as an unsaved copy, so there is a real script to look at. Help > Open Sample Script
//! brings it back at any time. The bundled text is never written: Save asks where to put a copy.

use std::path::PathBuf;

use storyteller_engine::Session;
use storyteller_engine::files::FileFormat;

use super::StorytellerApp;

const FIELD_STATION: &str = include_str!("../../../examples/field-station.osf");

/// The name the sample shows in the window title and offers when it is first saved.
pub(super) const SAMPLE_NAME: &str = "Field Station";

/// The line shown under the page while the sample is open.
pub(super) const SAMPLE_LINE: &str =
    "This is the sample script, Field Station. Change anything; Save keeps a copy wherever you choose. Help > Open Sample Script brings it back.";

/// What the app opens when it starts.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum LaunchOpen {
    File(PathBuf),
    Sample,
    Nothing,
}

/// The launch rule (SUITE-STANDARDS.md §8). A file named on the command line opens; then a file
/// opened from the Finder. macOS hands a launch's files to the app before it finishes
/// launching, so they are already queued when the app is created, and the sample never shows
/// first. Otherwise the sample opens when asked for (`--open-sample`), or once, on a first
/// launch, for a writer with no recent files.
pub(super) fn launch_choice(
    file: Option<PathBuf>,
    from_finder: Option<PathBuf>,
    first_launch: bool,
    has_recent: bool,
    open_sample: bool,
) -> LaunchOpen {
    match file.or(from_finder) {
        Some(path) => LaunchOpen::File(path),
        None if open_sample || (first_launch && !has_recent) => LaunchOpen::Sample,
        None => LaunchOpen::Nothing,
    }
}

/// Command-line requests for the start: `--open-sample` and `--check-for-updates` (used by
/// Method Launcher, SUITE-STANDARDS.md §3).
#[derive(Clone, Copy, Debug, Default)]
pub struct StartFlags {
    pub open_sample: bool,
    pub check_for_updates: bool,
}

impl StartFlags {
    /// The commands the first frame runs for these flags.
    pub(super) fn commands(self) -> Vec<&'static str> {
        if self.check_for_updates { vec!["help.check_updates"] } else { Vec::new() }
    }
}

impl StorytellerApp {
    /// Opens a fresh copy of the sample, unsaved and with no path.
    pub(super) fn open_sample(&mut self) {
        match storyteller_core::osf::parse(FIELD_STATION) {
            Ok(script) => {
                let learned = std::mem::take(&mut self.session.spelling);
                self.session = Session::new(script);
                self.session.format = FileFormat::Osf;
                self.session.spelling = learned;
                self.after_open();
                self.sample = true;
                self.status = SAMPLE_LINE.into();
            }
            Err(error) => self.error = Some(format!("The sample script could not be read: {error}")),
        }
    }

    /// Whether the open script is the sample, not yet saved anywhere.
    pub(super) fn showing_sample(&self) -> bool {
        self.sample && self.session.path.is_none()
    }

    /// The name for the window title: the sample's, or the session's own.
    pub(super) fn display_title(&self) -> String {
        if self.showing_sample() {
            let name = format!("{SAMPLE_NAME} (sample)");
            if self.session.dirty { format!("{name} — edited") } else { name }
        } else {
            self.session.title()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_launch_rule() {
        let file = || Some(PathBuf::from("/scripts/given.osf"));
        let finder = || Some(PathBuf::from("/scripts/double-clicked.fdx"));
        // A file named on the command line, then one from the Finder.
        assert_eq!(launch_choice(file(), finder(), true, false, false), LaunchOpen::File(PathBuf::from("/scripts/given.osf")));
        assert_eq!(
            launch_choice(None, finder(), true, false, false),
            LaunchOpen::File(PathBuf::from("/scripts/double-clicked.fdx")),
            "a first launch from the Finder opens the file, not the sample"
        );
        assert_eq!(
            launch_choice(None, finder(), false, true, true),
            LaunchOpen::File(PathBuf::from("/scripts/double-clicked.fdx")),
            "and wins over --open-sample"
        );
        // The sample: once, on a first launch with nothing to open and no recent files.
        assert_eq!(launch_choice(None, None, true, false, false), LaunchOpen::Sample);
        assert_eq!(launch_choice(None, None, false, false, false), LaunchOpen::Nothing, "never twice");
        assert_eq!(launch_choice(None, None, true, true, false), LaunchOpen::Nothing, "a writer with scripts never sees it unasked");
        // --open-sample asks for it, any launch.
        assert_eq!(launch_choice(None, None, false, true, true), LaunchOpen::Sample);
    }

    #[test]
    fn check_for_updates_runs_the_help_menu_check_at_start() {
        assert_eq!(StartFlags { check_for_updates: true, ..StartFlags::default() }.commands(), ["help.check_updates"]);
        assert!(StartFlags::default().commands().is_empty());
    }
}
