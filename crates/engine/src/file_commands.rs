//! File: new, open, save and the exports, and the reports' CSV and PDF output.

use super::*;

/// File: new, open, save and the exports.
pub(super) fn file_commands() -> Vec<Command> {
    use ParamType::*;
    vec![
        Command {
            id: "file.new",
            label: "New",
            menu: "File/New",
            shortcut: Some("Cmd+N"),
            params: &[],
            enabled: always,
            run: |s, _| {
                *s = Session::new(Script::blank());
                Ok(Outcome::done())
            },
        },
        Command {
            id: "file.new_tv",
            label: "New TV Script…",
            menu: "File/New TV Script…",
            shortcut: None,
            params: const {
                &[param(
                    "shape",
                    String,
                    false,
                    "one_hour (Teaser, Acts One to Five, Tag), half_hour (Cold Open, Acts One to Three, Tag) or multi_cam (Cold Open, Acts One and Two, Tag; lettered scenes)",
                )]
            },
            enabled: always,
            run: |s, p| {
                let shape = match opt_string(p, "shape")? {
                    Some(id) => storyteller_core::model::TvShape::from_id(&id)
                        .ok_or_else(|| CommandError::Invalid { name: "shape", reason: "one_hour, half_hour or multi_cam".into() })?,
                    None => storyteller_core::model::TvShape::default(),
                };
                *s = Session::new(Script::tv(shape));
                // The caret in the first act's scene heading, ready to write.
                s.caret = Caret { element: 1, offset: 0 };
                Ok(Outcome::done())
            },
        },
        Command {
            id: "file.open",
            label: "Open…",
            menu: "File/Open…",
            shortcut: Some("Cmd+O"),
            params: const { &[param("path", Path, true, "Fountain (.fountain) or Final Draft (.fdx) file")] },
            enabled: always,
            run: |s, p| {
                let path = path(p, "path")?;
                let (script, format) = files::open(&path)?;
                let layout = s.layout.clone();
                *s = Session::new(script);
                s.layout = layout;
                s.format = format;
                if files::is_import(&path) {
                    // Nothing writes this file back: Save asks where, in a format of ours.
                    s.format = FileFormat::default();
                    s.dirty = true;
                    return Ok(Outcome::message(format!("Imported {} elements. Save to keep them in a format of ours.", s.script.elements.len())));
                }
                s.path = Some(path);
                Ok(Outcome::message(format!("Opened {} elements", s.script.elements.len())))
            },
        },
        Command {
            id: "file.save",
            label: "Save",
            menu: "File/Save",
            shortcut: Some("Cmd+S"),
            params: &[],
            enabled: has_path,
            run: |s, _| {
                let path = s.path.clone().ok_or(CommandError::Missing("path"))?;
                s.learn_before_saving();
                files::save(&s.script, &path, s.format, &s.layout)?;
                s.dirty = false;
                s.break_typing();
                Ok(Outcome::message(files::saved_message("Saved", &path, &s.script, s.format)))
            },
        },
        Command {
            id: "file.save_as",
            label: "Save As…",
            menu: "File/Save As…",
            shortcut: Some("Cmd+Shift+S"),
            params: const { &[param("path", Path, true, "Destination; .fdx saves FDX, .osf or .fadein Open Screenplay Format, anything else Fountain")] },
            enabled: always,
            run: |s, p| {
                let path = path(p, "path")?;
                let format = match FileFormat::from_path(&path) {
                    FileFormat::Pdf => return Err(CommandError::Invalid { name: "path", reason: "use file.export_pdf for PDF".into() }),
                    other => other,
                };
                s.learn_before_saving();
                files::save(&s.script, &path, format, &s.layout)?;
                s.path = Some(path.clone());
                s.format = format;
                s.dirty = false;
                s.break_typing();
                Ok(Outcome::message(files::saved_message("Saved", &path, &s.script, format)))
            },
        },
        Command {
            id: "file.export_pdf",
            label: "Export PDF…",
            menu: "File/Export PDF…",
            shortcut: Some("Cmd+E"),
            params: const {
                &[
                    param("path", Path, true, "Destination .pdf"),
                    param("revisions", String, false, "Revisions to print: active, all or none; the view's choice when omitted"),
                    param("colour", Bool, false, "Print revised pages in their set's page colour (white when omitted)"),
                    param("pages", String, false, "Pages to issue, such as 12-18, 22A; every page when omitted"),
                    param("scenes", String, false, "Scenes whose pages to issue, such as 12-15"),
                    param("revised_only", Bool, false, "Only pages with a shown revision"),
                    param("clean", Bool, false, "A clean copy: no marks, no header label, no colours"),
                ]
            },
            enabled: always,
            run: |s, p| {
                let path = path(p, "path")?;
                let revisions = match opt_string(p, "revisions")? {
                    Some(show) => show_revisions("revisions", &show)?,
                    None => s.show_revisions,
                };
                let selection = storyteller_core::paginate::PageSelection {
                    pages: opt_string(p, "pages")?,
                    scenes: opt_string(p, "scenes")?,
                    revised_only: opt_bool(p, "revised_only")?.unwrap_or(false),
                };
                let options = storyteller_pdf::PdfOptions {
                    revisions,
                    coloured_pages: opt_bool(p, "colour")?.unwrap_or(false),
                    selection,
                    clean: opt_bool(p, "clean")?.unwrap_or(false),
                    ..Default::default()
                };
                files::save_pdf(&s.script, &path, &s.layout, &options)?;
                Ok(Outcome::message(files::saved_message("Exported", &path, &s.script, FileFormat::Pdf)))
            },
        },
        Command {
            id: "file.export_fdx",
            label: "Export FDX…",
            menu: "File/Export FDX (.fdx)…",
            shortcut: None,
            params: const { &[param("path", Path, true, "Destination .fdx")] },
            enabled: always,
            run: |s, p| export(s, p, FileFormat::Fdx),
        },
        Command {
            id: "file.export_osf",
            label: "Export Open Screenplay Format…",
            menu: "File/Export Open Screenplay Format (.osf)…",
            shortcut: None,
            params: const { &[param("path", Path, true, "Destination .osf, or .fadein for the ZIP container")] },
            enabled: always,
            run: |s, p| {
                let path = path(p, "path")?;
                let format = if FileFormat::from_path(&path) == FileFormat::FadeIn { FileFormat::FadeIn } else { FileFormat::Osf };
                files::save(&s.script, &path, format, &s.layout)?;
                Ok(Outcome::message(files::saved_message("Exported", &path, &s.script, format)))
            },
        },
        Command {
            id: "file.export_fountain",
            label: "Export Fountain…",
            menu: "File/Export Fountain…",
            shortcut: None,
            params: const { &[param("path", Path, true, "Destination .fountain")] },
            enabled: always,
            run: |s, p| export(s, p, FileFormat::Fountain),
        },
        Command {
            id: "file.export_text",
            label: "Export Plain Text…",
            menu: "File/Export Plain Text…",
            shortcut: None,
            params: const { &[param("path", Path, true, "Destination .txt")] },
            enabled: always,
            run: |s, p| {
                let path = path(p, "path")?;
                files::save_text(&path, &reports::plain_text(s.paginated()))?;
                Ok(Outcome::message(format!("Exported {}", path.display())))
            },
        },
        Command {
            id: "file.export_scenes",
            label: "Export for Method Schedule…",
            menu: "File/Export for Method Schedule…",
            shortcut: None,
            params: const { &[param("path", Path, true, "Destination .mscenes")] },
            enabled: always,
            run: |s, p| {
                let path = path(p, "path")?;
                files::save_text(&path, &scenes_file::write(s, &today_iso()))?;
                Ok(Outcome::message(format!("Exported {}", path.display())))
            },
        },
    ]
}

/// The optional CSV destination every report takes.
pub(super) const REPORT_PATH: ParamSpec = param("path", ParamType::Path, false, "Write the report to this .csv file, or print it to this .pdf file");

/// A report's rows, also written as CSV or printed as a PDF table when the command has a `path`.
pub(super) fn report(session: &Session, params: &Params, rows: Value, table: reports::Table) -> Result<Outcome, CommandError> {
    let Some(path) = opt_string(params, "path")?.filter(|p| !p.trim().is_empty()).map(PathBuf::from) else {
        return Ok(Outcome::value(rows));
    };
    let extension = path.extension().map(|e| e.to_string_lossy().to_ascii_lowercase());
    match extension.as_deref() {
        Some("csv") => files::save_text(&path, &table.csv())?,
        Some("pdf") => {
            let title = session.script.title_page.get("Title").map(storyteller_core::inline::plain);
            let bytes = table.pdf(title.as_deref(), session.layout.page_size_points()).map_err(files::FileError::from)?;
            files::save_bytes(&path, &bytes)?;
        }
        _ => return Err(CommandError::Invalid { name: "path", reason: "reports are written as .csv or .pdf".into() }),
    }
    Ok(Outcome { message: Some(format!("Exported {}", path.display())), value: rows })
}

pub(super) fn export(session: &mut Session, params: &Params, format: FileFormat) -> Result<Outcome, CommandError> {
    let path = path(params, "path")?;
    files::save(&session.script, &path, format, &session.layout)?;
    Ok(Outcome::message(files::saved_message("Exported", &path, &session.script, format)))
}
