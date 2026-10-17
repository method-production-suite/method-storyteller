//! Method Storyteller core: the screenplay document model, the Fountain and Final Draft (FDX)
//! formats, and industry-standard pagination.
//!
//! Pure Rust, no UI and no filesystem access, so it also builds for `wasm32-unknown-unknown`.
//! Every parser treats its input as hostile: sizes are capped, loops are bounded and nothing
//! here panics.

pub mod autocomplete;
pub mod fdx;
pub mod fountain;
pub mod inline;
pub mod layout;
pub mod model;
pub mod osf;
pub mod paginate;
pub mod wrap;

pub use model::{Element, ElementKind, FdxDocument, FdxParagraph, FdxSection, OsfDocument, OsfParagraph, OsfSection, Script, TitlePage};

/// Largest script file accepted by any parser (32 MiB).
pub const MAX_INPUT_BYTES: usize = 32 * 1024 * 1024;
/// Most elements a script may hold.
pub const MAX_ELEMENTS: usize = 200_000;

/// Failures shared by the format readers.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum CoreError {
    #[error("the file is larger than the {} MiB limit", MAX_INPUT_BYTES / (1024 * 1024))]
    TooLarge,
    #[error("the script has more than {MAX_ELEMENTS} elements")]
    TooManyElements,
    #[error("the file is not valid UTF-8 text")]
    NotUtf8,
    #[error("the file is not a Final Draft script: {0}")]
    NotFdx(String),
    #[error("the file is not an Open Screenplay Format document: {0}")]
    NotOsf(String),
    #[error("the XML is damaged: {0}")]
    Xml(String),
}

/// Decode bytes from disk as UTF-8 text, dropping a leading byte-order mark.
pub fn decode_text(bytes: &[u8]) -> Result<&str, CoreError> {
    if bytes.len() > MAX_INPUT_BYTES {
        return Err(CoreError::TooLarge);
    }
    let text = std::str::from_utf8(bytes).map_err(|_| CoreError::NotUtf8)?;
    Ok(text.strip_prefix('\u{feff}').unwrap_or(text))
}
