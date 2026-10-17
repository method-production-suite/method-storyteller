//! Method Storyteller engine: the editing [`Session`], the [`commands::Registry`] through which
//! every action runs, Final Draft–style element [`flow`], file I/O and the JSON-lines
//! [`control`] protocol shared by the desktop app, the CLI and headless automation.

pub mod commands;
pub mod container;
pub mod control;
pub mod files;
pub mod flow;
pub mod session;

pub use commands::{Command, CommandError, Outcome, Registry};
pub use session::{Caret, Session};

/// Product name shown in titles and PDF metadata.
pub const APP_NAME: &str = "Method Storyteller";
/// Command name, `method-storyteller`.
pub const BIN_NAME: &str = "method-storyteller";
/// Bundle id; also names the settings and data folders.
pub const BUNDLE_ID: &str = "com.methodstoryteller.desktop";
/// Version string, `Method Storyteller 0.1.0`.
pub fn app_version() -> String {
    format!("{APP_NAME} {}", env!("CARGO_PKG_VERSION"))
}
/// `--version` output, `method-storyteller 0.1.0 (Method Storyteller)`.
pub fn version_line() -> String {
    format!("{BIN_NAME} {} ({APP_NAME})", env!("CARGO_PKG_VERSION"))
}
