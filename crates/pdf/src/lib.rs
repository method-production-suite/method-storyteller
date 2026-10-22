//! PDF export: paginated screenplay pages drawn with Courier Prime (embedded and subset) through
//! `craft-pdf` (Krilla). Text stays selectable; every scene gets a bookmark.

use craft_pdf::write::backend::color::rgb;
use craft_pdf::write::backend::destination::XyzDestination;
use craft_pdf::write::backend::geom::{PathBuilder, Point, Rect, Size, Transform};
use craft_pdf::write::backend::outline::{Outline, OutlineNode};
use craft_pdf::write::backend::page::PageSettings;
use craft_pdf::write::backend::paint::{Fill, Stroke};
use craft_pdf::write::backend::text::{Font, TextDirection};
use craft_pdf::write::{self, Metadata, WriteError, Writer};
use storyteller_core::Script;
use storyteller_core::inline::{self, Style};
use storyteller_core::layout::{Layout, POINTS_PER_COLUMN, POINTS_PER_ROW, REVISION_MARK_INCHES, WRAP_ADVANCE_POINTS};
use storyteller_core::model::ShowRevisions;
use storyteller_core::paginate::{Page, Paginated, paginate};

/// Courier Prime, SIL Open Font License 1.1 (see `assets/fonts/OFL.txt`).
pub const COURIER_PRIME_REGULAR: &[u8] = include_bytes!("../../../assets/fonts/CourierPrime-Regular.ttf");
pub const COURIER_PRIME_BOLD: &[u8] = include_bytes!("../../../assets/fonts/CourierPrime-Bold.ttf");
pub const COURIER_PRIME_ITALIC: &[u8] = include_bytes!("../../../assets/fonts/CourierPrime-Italic.ttf");
pub const COURIER_PRIME_BOLD_ITALIC: &[u8] = include_bytes!("../../../assets/fonts/CourierPrime-BoldItalic.ttf");

const FONT_SIZE: f32 = 12.0;
/// Baseline below the top of a 12 pt row.
const BASELINE: f32 = 9.6;

#[derive(Debug, thiserror::Error)]
pub enum PdfError {
    #[error(transparent)]
    Write(#[from] WriteError),
    #[error("the page size is invalid")]
    PageSize,
}

struct Fonts {
    regular: Font,
    bold: Font,
    italic: Font,
    bold_italic: Font,
}

impl Fonts {
    fn load() -> Result<Self, WriteError> {
        Ok(Fonts {
            regular: write::font(COURIER_PRIME_REGULAR, 0, None)?,
            bold: write::font(COURIER_PRIME_BOLD, 0, None)?,
            italic: write::font(COURIER_PRIME_ITALIC, 0, None)?,
            bold_italic: write::font(COURIER_PRIME_BOLD_ITALIC, 0, None)?,
        })
    }

    fn get(&self, style: Style) -> Font {
        match (style.bold, style.italic) {
            (true, true) => self.bold_italic.clone(),
            (true, false) => self.bold.clone(),
            (false, true) => self.italic.clone(),
            (false, false) => self.regular.clone(),
        }
    }
}

/// Options that are not part of the layout.
#[derive(Clone, Debug, Default)]
pub struct PdfOptions {
    /// Producer/creator string, such as `Method Storyteller 0.1.0`.
    pub creator: String,
    /// Which revisions print their marks and header label.
    pub revisions: ShowRevisions,
    /// Revised pages in their set's page colour; otherwise every page stays white.
    pub coloured_pages: bool,
}

/// A `#RRGGBB` colour.
fn colour(hex: &str) -> Option<rgb::Color> {
    let hex = hex.strip_prefix('#').filter(|h| h.len() == 6)?;
    let byte = |i: usize| hex.get(i..i + 2).and_then(|h| u8::from_str_radix(h, 16).ok());
    Some(rgb::Color::new(byte(0)?, byte(2)?, byte(4)?))
}

/// Draw text with its left edge at `x` and baseline at `y`. Lines hold as many characters as
/// Final Draft fits (see `WRAP_ADVANCE_POINTS`), so text is drawn that much narrower than
/// Courier's 7.2 points per character.
fn draw(surface: &mut craft_pdf::write::backend::surface::Surface<'_>, fonts: &Fonts, style: Style, text: &str, x: f32, y: f32) {
    if text.trim().is_empty() {
        return;
    }
    let squeeze = WRAP_ADVANCE_POINTS / POINTS_PER_COLUMN;
    surface.push_transform(&Transform::from_scale(squeeze, 1.0));
    surface.draw_text(Point::from_xy(x / squeeze, y), fonts.get(style), FONT_SIZE, text, false, TextDirection::Auto);
    surface.pop();
}

/// Paginate and export a script.
pub fn export(script: &Script, layout: &Layout, options: &PdfOptions) -> Result<Vec<u8>, PdfError> {
    let paginated = paginate(script, layout);
    render(script, &paginated, layout, options)
}

/// Export already paginated pages.
pub fn render(script: &Script, paginated: &Paginated, layout: &Layout, options: &PdfOptions) -> Result<Vec<u8>, PdfError> {
    write::guard(|| {
        let fonts = Fonts::load()?;
        let (width, height) = layout.page_size_points();
        let size = Size::from_wh(width, height).ok_or(PdfError::PageSize)?;
        let title = script.title_page.get("Title").map(inline::plain).filter(|t| !t.trim().is_empty());
        let authors = script.title_page.get("Author").map(|a| a.lines().map(inline::plain).collect()).unwrap_or_default();
        let metadata = Metadata { creator: options.creator.clone(), producer: options.creator.clone(), title, authors, ..Metadata::default() };
        let mut writer = Writer::new(Default::default(), metadata);
        let document = writer.document_mut();

        let pages: Vec<&Page> = paginated.title_page.iter().chain(paginated.pages.iter()).collect();
        if pages.is_empty() {
            // An empty script still produces one blank page.
            document.start_page_with(PageSettings::new(size)).finish();
        }
        for page in &pages {
            let mut pdf_page = document.start_page_with(PageSettings::new(size));
            let mut surface = pdf_page.surface();
            let revision = if page.number > 0 { page.revision(&script.revisions, options.revisions) } else { None };
            if let Some(paper) = revision.filter(|_| options.coloured_pages).and_then(|set| set.page_color.as_deref()).and_then(colour)
                && let Some(rect) = Rect::from_xywh(0.0, 0.0, width, height)
            {
                let mut path = PathBuilder::new();
                path.push_rect(rect);
                if let Some(path) = path.finish() {
                    surface.set_fill(Some(Fill { paint: paper.into(), ..Fill::default() }));
                    surface.draw_path(&path);
                }
            }
            surface.set_fill(Some(Fill::default()));
            // The set's name and date under the page number, right-aligned with it.
            if let Some(set) = revision {
                let chars = set.name.chars().count() as f32;
                let x = f32::from(layout.page_number_right) * POINTS_PER_COLUMN - chars * WRAP_ADVANCE_POINTS;
                let y = f32::from(layout.page_number_row.saturating_add(1)) * POINTS_PER_ROW + BASELINE;
                draw(&mut surface, &fonts, Style::default(), &set.name, x, y);
            }
            // Each line with a shown revision gets its set's mark in the margin, once a row.
            let mut marked_rows = Vec::new();
            for line in &page.lines {
                if marked_rows.contains(&line.row) {
                    continue;
                }
                if let Some(set) = script.revisions.latest(&line.marks, options.revisions) {
                    marked_rows.push(line.row);
                    let y = f32::from(line.row) * POINTS_PER_ROW + BASELINE;
                    draw(&mut surface, &fonts, Style::default(), &set.mark, REVISION_MARK_INCHES * 72.0, y);
                }
            }
            for line in &page.lines {
                let start = f32::from(line.col) * POINTS_PER_COLUMN;
                let mut offset = 0.0;
                let y = f32::from(line.row) * POINTS_PER_ROW + BASELINE;
                for run in &line.runs {
                    let x = start + offset * WRAP_ADVANCE_POINTS;
                    let chars = run.text.chars().count() as f32;
                    draw(&mut surface, &fonts, run.style, &run.text, x, y);
                    if run.style.underline && chars > 0.0 {
                        let mut path = PathBuilder::new();
                        path.move_to(x, y + 1.5);
                        path.line_to(x + chars * WRAP_ADVANCE_POINTS, y + 1.5);
                        if let Some(path) = path.finish() {
                            surface.set_fill(None);
                            surface.set_stroke(Some(Stroke { width: 0.6, ..Stroke::default() }));
                            surface.draw_path(&path);
                            surface.set_stroke(None);
                            surface.set_fill(Some(Fill::default()));
                        }
                    }
                    offset += chars;
                }
            }
            surface.finish();
            pdf_page.finish();
        }

        let offset = usize::from(paginated.title_page.is_some());
        let mut outline = Outline::new();
        for scene in &paginated.scenes {
            let Some(page_index) = usize::try_from(scene.page).ok().and_then(|p| p.checked_sub(1)).map(|p| p + offset) else { continue };
            let label = match &scene.number {
                Some(number) => format!("{number}. {}", scene.heading),
                None => scene.heading.clone(),
            };
            let label = if label.trim().is_empty() { "(untitled scene)".to_string() } else { label };
            outline.push_child(OutlineNode::new(label, XyzDestination::new(page_index, Point::from_xy(0.0, 0.0))));
        }
        document.set_outline(outline);
        Ok(writer.finish()?)
    })
}
