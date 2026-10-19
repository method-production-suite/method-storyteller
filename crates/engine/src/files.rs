//! Reading and writing script files. Saves are atomic (`craft-atomic`): a failed save never
//! damages the previous file.

use std::io::Read;
use std::path::Path;

use serde::{Deserialize, Serialize};
use storyteller_core::layout::Layout;
use storyteller_core::{CoreError, MAX_INPUT_BYTES, Script, decode_text, fdx, fountain, osf};

use crate::container;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileFormat {
    #[default]
    Fountain,
    Fdx,
    /// Open Screenplay Format, plain XML (`.osf`).
    Osf,
    /// Open Screenplay Format in a ZIP archive (`.fadein`).
    FadeIn,
    Pdf,
}

impl FileFormat {
    /// Format implied by a file extension (Fountain for anything unknown).
    pub fn from_path(path: &Path) -> Self {
        match path.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase).as_deref() {
            Some("fdx") => FileFormat::Fdx,
            Some("osf") => FileFormat::Osf,
            Some("fadein") => FileFormat::FadeIn,
            Some("pdf") => FileFormat::Pdf,
            _ => FileFormat::Fountain,
        }
    }

    pub fn extension(self) -> &'static str {
        match self {
            FileFormat::Fountain => "fountain",
            FileFormat::Fdx => "fdx",
            FileFormat::Osf => "osf",
            FileFormat::FadeIn => "fadein",
            FileFormat::Pdf => "pdf",
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum FileError {
    #[error("could not read {path}: {source}")]
    Read { path: String, source: std::io::Error },
    #[error("could not save {path}: {source}")]
    Write { path: String, source: std::io::Error },
    #[error("{0}")]
    Format(#[from] CoreError),
    #[error("PDF export failed: {0}")]
    Pdf(#[from] storyteller_pdf::PdfError),
    #[error("scripts cannot be opened from PDF")]
    CannotOpenPdf,
    #[error("{0}")]
    Archive(String),
}

/// Read and parse a script; the format is sniffed from the content, then the extension.
pub fn open(path: &Path) -> Result<(Script, FileFormat), FileError> {
    let read_error = |source| FileError::Read { path: path.display().to_string(), source };
    let file = std::fs::File::open(path).map_err(read_error)?;
    let mut bytes = Vec::new();
    // Read one byte past the limit so oversize files are refused rather than truncated.
    file.take(MAX_INPUT_BYTES as u64 + 1).read_to_end(&mut bytes).map_err(read_error)?;
    if container::is_zip(&bytes) {
        let (document, others) = container::read_document(&bytes).map_err(FileError::Archive)?;
        let mut script = osf::parse(decode_text(&document)?)?;
        if let Some(osf) = script.osf.as_mut() {
            osf.archive_entries = others;
        }
        return Ok((script, FileFormat::FadeIn));
    }
    let text = decode_text(&bytes)?;
    let format = sniff(text).unwrap_or_else(|| FileFormat::from_path(path));
    let script = match format {
        FileFormat::Pdf => return Err(FileError::CannotOpenPdf),
        FileFormat::Fdx => fdx::parse(text)?,
        FileFormat::Osf | FileFormat::FadeIn => osf::parse(text)?,
        FileFormat::Fountain => fountain::parse(text)?,
    };
    // A plain-XML document is .osf even if it was named .fadein.
    let format = if format == FileFormat::FadeIn { FileFormat::Osf } else { format };
    Ok((script, format))
}

/// The XML format a text's root element names, if it is XML.
fn sniff(text: &str) -> Option<FileFormat> {
    let head = text.trim_start();
    if !head.starts_with('<') {
        return None;
    }
    let start = head.get(..head.len().min(4096)).unwrap_or(head);
    let root = start.split('<').filter(|tag| !tag.starts_with('?') && !tag.starts_with('!')).nth(1).unwrap_or_default();
    if root.starts_with("FinalDraft") {
        Some(FileFormat::Fdx)
    } else if root.starts_with("document") {
        Some(FileFormat::Osf)
    } else {
        None
    }
}

/// Serialize a script in a format.
pub fn encode(script: &Script, format: FileFormat, layout: &Layout) -> Result<Vec<u8>, FileError> {
    Ok(match format {
        FileFormat::Fountain => fountain::write(script).into_bytes(),
        FileFormat::Fdx => fdx::write_with_layout(script, layout).into_bytes(),
        FileFormat::Osf => osf::write(script).into_bytes(),
        FileFormat::FadeIn => {
            // Other members of the archive the script came in go back unchanged.
            let others = script.osf.as_ref().map(|d| d.archive_entries.as_slice()).unwrap_or_default();
            container::write_document(osf::write(script).as_bytes(), others).map_err(|source| FileError::Write { path: "archive".into(), source })?
        }
        FileFormat::Pdf => storyteller_pdf::export(script, layout, &storyteller_pdf::PdfOptions { creator: crate::app_version() })?,
    })
}

/// What saving `script` as `format` would lose or show differently in other apps, in words a
/// writer can act on. Empty when nothing is lost. Studied in docs/research/osf-vs-fountain.md.
pub fn save_warnings(script: &Script, format: FileFormat) -> Vec<String> {
    use storyteller_core::ElementKind as K;
    let mut warnings = Vec::new();
    let plural = |n: usize, one: &str, many: &str| if n == 1 { format!("1 {one}") } else { format!("{n} {many}") };
    match format {
        FileFormat::Fountain => {
            let shots = script.elements.iter().filter(|e| e.kind == K::Shot).count();
            if shots > 0 {
                warnings.push(format!("{} will read back as action (Fountain has no shot)", plural(shots, "shot", "shots")));
            }
            // Speeches with more than one dialogue paragraph: count where each run starts.
            let kinds: Vec<K> = script.elements.iter().map(|e| e.kind).collect();
            let joined = (1..kinds.len())
                .filter(|&i| {
                    kinds.get(i) == Some(&K::Dialogue) && kinds.get(i - 1) == Some(&K::Dialogue) && (i < 2 || kinds.get(i - 2) != Some(&K::Dialogue))
                })
                .count();
            if joined > 0 {
                warnings.push(format!(
                    "{} in more than one paragraph will become one paragraph each (Fountain has one per speech)",
                    plural(joined, "speech", "speeches")
                ));
            }
        }
        FileFormat::Osf | FileFormat::FadeIn => {
            let sections = script.elements.iter().filter(|e| e.kind == K::Section).count();
            if sections > 0 {
                warnings.push(format!(
                    "{} kept for Method Storyteller; other OSF apps may print them as text (OSF has no outline element)",
                    plural(sections, "section is", "sections are")
                ));
            }
        }
        FileFormat::Fdx | FileFormat::Pdf => {}
    }
    warnings
}

/// A save's message: "Saved <path>", then anything the format did not keep.
pub fn saved_message(verb: &str, path: &Path, script: &Script, format: FileFormat) -> String {
    let warnings = save_warnings(script, format);
    let other = if format == FileFormat::Fountain { "Open Screenplay Format" } else { "Fountain" };
    match warnings.as_slice() {
        [] => format!("{verb} {}", path.display()),
        _ if format == FileFormat::Fountain => format!("{verb} {}. Note: {}. Save as {other} to keep them.", path.display(), warnings.join("; ")),
        _ => format!("{verb} {}. Note: {}.", path.display(), warnings.join("; ")),
    }
}

/// Atomically write a script in a format.
pub fn save(script: &Script, path: &Path, format: FileFormat, layout: &Layout) -> Result<(), FileError> {
    let bytes = encode(script, format, layout)?;
    craft_atomic::atomic_write(path, &bytes).map_err(|source| FileError::Write { path: path.display().to_string(), source })
}
