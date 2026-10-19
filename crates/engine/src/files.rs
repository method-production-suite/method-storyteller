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

/// What kind of trouble a file error is, so the app can say it plainly without reading the
/// operating system's message (which Windows translates, and which carries the path).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileProblem {
    /// The file, or the folder it was going into, is not there.
    NotFound,
    /// The system does not allow reading the file or writing to the folder.
    PermissionDenied,
    /// The disk is full.
    StorageFull,
    /// The disk or folder is read-only.
    ReadOnly,
    /// The file is over the size or element limit.
    TooLarge,
    /// The file is not a script in a format we read, or it is damaged.
    NotAScript,
    /// A PDF, which is a printed script and cannot be opened.
    Pdf,
    /// Anything else; the error's text says what.
    Other,
}

impl FileError {
    /// The kind of trouble, taken from the error's type and never from its text.
    pub fn problem(&self) -> FileProblem {
        match self {
            FileError::Read { source, .. } | FileError::Write { source, .. } => match source.kind() {
                std::io::ErrorKind::NotFound => FileProblem::NotFound,
                std::io::ErrorKind::PermissionDenied => FileProblem::PermissionDenied,
                std::io::ErrorKind::StorageFull | std::io::ErrorKind::QuotaExceeded => FileProblem::StorageFull,
                std::io::ErrorKind::ReadOnlyFilesystem => FileProblem::ReadOnly,
                _ => FileProblem::Other,
            },
            FileError::Format(error) => match error {
                CoreError::TooLarge | CoreError::TooManyElements => FileProblem::TooLarge,
                CoreError::NotUtf8 | CoreError::NotFdx(_) | CoreError::NotOsf(_) | CoreError::Xml(_) => FileProblem::NotAScript,
            },
            FileError::Archive(_) => FileProblem::NotAScript,
            FileError::CannotOpenPdf => FileProblem::Pdf,
            FileError::Pdf(_) => FileProblem::Other,
        }
    }
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

/// Atomically write a script in a format.
pub fn save(script: &Script, path: &Path, format: FileFormat, layout: &Layout) -> Result<(), FileError> {
    let bytes = encode(script, format, layout)?;
    craft_atomic::atomic_write(path, &bytes).map_err(|source| FileError::Write { path: path.display().to_string(), source })
}
