//! The sample script, Field Station, built into the app. With nothing to open, the app shows it
//! as an unsaved copy, so a first launch has a real script to look at. Help > Open Sample Script
//! brings it back at any time. The bundled text is never written: Save asks where to put a copy.

use storyteller_engine::Session;
use storyteller_engine::files::FileFormat;

use super::StorytellerApp;

const FIELD_STATION: &str = include_str!("../../../examples/field-station.osf");

/// The name the sample shows in the window title and offers when it is first saved.
pub(super) const SAMPLE_NAME: &str = "Field Station";

/// The line shown under the page while the sample is open.
pub(super) const SAMPLE_LINE: &str =
    "This is the sample script, Field Station. Change anything; Save keeps a copy wherever you choose. Help > Show Sample at Launch turns this off.";

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
