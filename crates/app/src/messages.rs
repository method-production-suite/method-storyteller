//! What the app says when something is empty or goes wrong, in the family voice (docs/BRAND.md):
//! plain, specific, what happened and what to do next. No exclamation marks, no em dashes.

use std::path::Path;

/// The navigator with no scenes in the script.
pub const NO_SCENES_TITLE: &str = "No scenes yet";
pub const NO_SCENES_BODY: &str = "Start a line with INT. or EXT. and it appears here as a scene.";

/// The first line of an empty script, where the page would otherwise show only "Scene Heading".
pub const EMPTY_SCRIPT_HINT: &str = "Scene Heading: start with INT. or EXT.";

/// The find bar when nothing matches.
pub fn nothing_found(text: &str) -> String {
    format!("No matches for “{text}”")
}

/// A file that did not open. `detail` is the engine's error text.
pub fn open_failed(path: &Path, detail: &str) -> String {
    let name = file_name(path);
    let lower = detail.to_lowercase();
    let reason = if lower.contains("no such file") || lower.contains("cannot find") {
        "The file is not there any more; it may have been moved or renamed.".to_string()
    } else if lower.contains("permission denied") || lower.contains("access is denied") {
        "This computer does not let Method Storyteller read it.".to_string()
    } else if lower.contains("cannot be opened from pdf") {
        "A PDF is a printed script. Open the .fountain, .osf or .fdx it was made from.".to_string()
    } else if lower.contains("larger than") {
        format!("{}.", capitalized(detail))
    } else if lower.contains("not valid utf-8") || lower.contains("is not a") || lower.contains("damaged") {
        format!("It is not a script Method Storyteller can read ({detail}).")
    } else {
        format!("({detail})")
    };
    format!("“{name}” did not open. {reason}")
}

/// A save that failed. The script is still open, so say so and point at Save As.
pub fn save_failed(path: &Path, detail: &str) -> String {
    let name = file_name(path);
    let lower = detail.to_lowercase();
    let reason = if lower.contains("permission denied") || lower.contains("access is denied") {
        "The folder does not allow changes.".to_string()
    } else if lower.contains("no space") || lower.contains("disk full") || lower.contains("not enough space") {
        "The disk is full.".to_string()
    } else if lower.contains("read-only") {
        "The disk is read-only.".to_string()
    } else if lower.contains("no such file") || lower.contains("cannot find") {
        "The folder is not there any more.".to_string()
    } else {
        format!("({detail})")
    };
    format!("“{name}” was not saved. {reason} Your changes are still here: use Save As to save them somewhere else.")
}

/// An update check that failed.
pub fn update_check_failed(detail: Option<&str>) -> String {
    match detail.map(str::trim).filter(|d| !d.is_empty()) {
        Some(detail) => format!("Could not check for updates ({detail}). Help > Check for Updates tries again."),
        None => "Could not check for updates. Help > Check for Updates tries again.".to_string(),
    }
}

/// An update that downloaded but would not install.
pub fn update_install_failed(detail: Option<&str>) -> String {
    match detail {
        Some(detail) => format!("The update did not install ({detail}). It is still ready: Restart to Update tries again."),
        None => "The update did not install. It is still ready: Restart to Update tries again.".to_string(),
    }
}

fn file_name(path: &Path) -> String {
    path.file_name().map_or_else(|| path.display().to_string(), |n| n.to_string_lossy().into_owned())
}

fn capitalized(text: &str) -> String {
    let mut chars = text.chars();
    chars.next().map(|c| c.to_uppercase().chain(chars).collect()).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every message stays in the family voice.
    fn voice(text: &str) {
        assert!(!text.contains('!'), "no exclamation marks: {text}");
        assert!(!text.contains('—'), "no em dashes: {text}");
    }

    #[test]
    fn open_failures_say_what_happened() {
        let path = Path::new("/scripts/Field Station.fdx");
        let missing = open_failed(path, "could not read /scripts/Field Station.fdx: No such file or directory (os error 2)");
        assert_eq!(missing, "“Field Station.fdx” did not open. The file is not there any more; it may have been moved or renamed.");
        let pdf = open_failed(Path::new("draft.pdf"), "scripts cannot be opened from PDF");
        assert!(pdf.contains("A PDF is a printed script"), "{pdf}");
        let big = open_failed(path, "the file is larger than the 32 MiB limit");
        assert!(big.ends_with("The file is larger than the 32 MiB limit."), "{big}");
        let broken = open_failed(path, "the XML is damaged: unexpected end");
        assert!(broken.contains("not a script Method Storyteller can read (the XML is damaged"), "{broken}");
        for text in [missing, pdf, big, broken, open_failed(path, "something odd")] {
            voice(&text);
        }
    }

    #[test]
    fn save_failures_keep_the_work_and_point_at_save_as() {
        let path = Path::new("/locked/Field Station.fountain");
        let denied = save_failed(path, "could not save /locked/Field Station.fountain: Permission denied (os error 13)");
        assert_eq!(
            denied,
            "“Field Station.fountain” was not saved. The folder does not allow changes. Your changes are still here: use Save As to save them somewhere else."
        );
        let full = save_failed(path, "No space left on device (os error 28)");
        assert!(full.contains("The disk is full."), "{full}");
        voice(&denied);
        voice(&full);
    }

    #[test]
    fn update_failures() {
        assert_eq!(update_check_failed(Some("offline")), "Could not check for updates (offline). Help > Check for Updates tries again.");
        assert_eq!(update_check_failed(None), "Could not check for updates. Help > Check for Updates tries again.");
        voice(&update_install_failed(Some("the disk is full")));
        assert_eq!(nothing_found("drum"), "No matches for “drum”");
        voice(NO_SCENES_BODY);
        voice(EMPTY_SCRIPT_HINT);
    }
}
