//! Report tables as PDF (docs/production-spec.md §4.3): landscape pages in Courier Prime 9 pt,
//! the column heads repeated on every page, long cells wrapped inside their column, and
//! "Page 2 of 5" at the foot.

use craft_pdf::write::backend::color::rgb;
use craft_pdf::write::backend::geom::{PathBuilder, Point, Rect, Size};
use craft_pdf::write::backend::page::PageSettings;
use craft_pdf::write::backend::paint::{Fill, Stroke};
use craft_pdf::write::backend::surface::Surface;
use craft_pdf::write::backend::text::{Font, TextDirection};
use craft_pdf::write::{self, Metadata, Writer};

use crate::{COURIER_PRIME_BOLD, COURIER_PRIME_REGULAR, PdfError};

const SIZE: f32 = 9.0;
/// Courier Prime is 0.6 em wide.
const ADVANCE: f32 = SIZE * 0.6;
const LINE: f32 = 11.0;
const MARGIN: f32 = 36.0;
const TITLE_SIZE: f32 = 12.0;
/// Space between columns, in characters.
const GAP: usize = 2;
/// The narrowest a column is squeezed to, in characters.
const MIN_COLUMN: usize = 6;

/// A report to print: a title, a line of totals, the column heads and the rows.
#[derive(Clone, Debug, Default)]
pub struct PdfTable {
    /// Such as `Field Station: Scenes`.
    pub title: String,
    /// Such as `42 scenes, 98 4/8 pages`.
    pub summary: String,
    pub header: Vec<String>,
    pub rows: Vec<Vec<String>>,
    /// Columns set flush right (counts and lengths), by index.
    pub right: Vec<usize>,
    /// Producer/creator string, such as `Method Storyteller 0.1.0`.
    pub creator: String,
}

/// Each column's width in characters: as wide as its widest cell, the widest squeezed first
/// until the table fits `available`.
fn widths(table: &PdfTable, available: usize) -> Vec<usize> {
    let count = table.header.len();
    let mut widths: Vec<usize> = (0..count)
        .map(|c| {
            let cells = table.rows.iter().filter_map(|r| r.get(c)).map(|t| t.chars().count());
            cells.chain(table.header.get(c).map(|h| h.chars().count())).max().unwrap_or(0).max(1)
        })
        .collect();
    let budget = available.saturating_sub(GAP * count.saturating_sub(1));
    while widths.iter().sum::<usize>() > budget {
        let Some((widest, width)) = widths.iter().copied().enumerate().max_by_key(|&(i, w)| (w, std::cmp::Reverse(i))) else { break };
        if width <= MIN_COLUMN {
            break;
        }
        if let Some(w) = widths.get_mut(widest) {
            *w -= 1;
        }
    }
    widths
}

/// Text broken into lines of at most `width` characters, at spaces where it can be.
pub fn wrap(text: &str, width: usize) -> Vec<String> {
    let width = width.max(1);
    let mut lines = Vec::new();
    for paragraph in text.split('\n') {
        let mut line = String::new();
        for word in paragraph.split(' ').filter(|w| !w.is_empty()) {
            let mut word: Vec<char> = word.chars().collect();
            loop {
                let used = line.chars().count();
                let room = if used == 0 { width } else { width.saturating_sub(used + 1) };
                if word.len() <= room {
                    if used > 0 {
                        line.push(' ');
                    }
                    line.extend(word.iter());
                    break;
                }
                if used > 0 {
                    lines.push(std::mem::take(&mut line));
                    continue;
                }
                // A word longer than the column breaks where the column ends.
                let rest = word.split_off(width);
                lines.push(word.iter().collect());
                word = rest;
            }
        }
        lines.push(line);
    }
    lines
}

/// One row's cells, wrapped: `lines[line][column]`.
fn row_lines(cells: &[String], widths: &[usize]) -> Vec<Vec<String>> {
    let wrapped: Vec<Vec<String>> = widths.iter().enumerate().map(|(c, &w)| wrap(cells.get(c).map_or("", String::as_str), w)).collect();
    let height = wrapped.iter().map(Vec::len).max().unwrap_or(1).max(1);
    (0..height).map(|l| wrapped.iter().map(|cell| cell.get(l).cloned().unwrap_or_default()).collect()).collect()
}

struct Fonts {
    regular: Font,
    bold: Font,
}

fn text(surface: &mut Surface<'_>, font: &Font, size: f32, text: &str, x: f32, y: f32) {
    if !text.trim().is_empty() {
        surface.draw_text(Point::from_xy(x, y), font.clone(), size, text, false, TextDirection::Auto);
    }
}

fn rule(surface: &mut Surface<'_>, x0: f32, x1: f32, y: f32) {
    let mut path = PathBuilder::new();
    path.move_to(x0, y);
    path.line_to(x1, y);
    if let Some(path) = path.finish() {
        surface.set_fill(None);
        surface.set_stroke(Some(Stroke { width: 0.5, ..Stroke::default() }));
        surface.draw_path(&path);
        surface.set_stroke(None);
        surface.set_fill(Some(Fill::default()));
    }
}

fn band(surface: &mut Surface<'_>, x: f32, y: f32, width: f32, height: f32) {
    let Some(rect) = Rect::from_xywh(x, y, width, height) else { return };
    let mut path = PathBuilder::new();
    path.push_rect(rect);
    if let Some(path) = path.finish() {
        surface.set_fill(Some(Fill { paint: rgb::Color::new(0xEE, 0xEE, 0xEE).into(), ..Fill::default() }));
        surface.draw_path(&path);
        surface.set_fill(Some(Fill::default()));
    }
}

/// A report as a PDF on landscape pages of the script's paper (`page` is its portrait size in
/// points).
pub fn render_table(table: &PdfTable, page: (f32, f32)) -> Result<Vec<u8>, PdfError> {
    write::guard(|| {
        let fonts = Fonts { regular: write::font(COURIER_PRIME_REGULAR, 0, None)?, bold: write::font(COURIER_PRIME_BOLD, 0, None)? };
        let (width, height) = (page.0.max(page.1), page.0.min(page.1));
        let size = Size::from_wh(width, height).ok_or(PdfError::PageSize)?;
        let available = ((width - 2.0 * MARGIN) / ADVANCE).floor() as usize;
        let widths = widths(table, available);
        let starts: Vec<f32> = widths
            .iter()
            .scan(0usize, |at, &w| {
                let start = *at;
                *at += w + GAP;
                Some(MARGIN + start as f32 * ADVANCE)
            })
            .collect();
        let right_edge = MARGIN + (widths.iter().sum::<usize>() + GAP * widths.len().saturating_sub(1)) as f32 * ADVANCE;
        let head = row_lines(&table.header, &widths);
        // The title, the totals, a space, the column heads and their rule.
        let body_top = MARGIN + TITLE_SIZE + 4.0 + LINE + 8.0 + head.len() as f32 * LINE + 4.0;
        let body_bottom = height - MARGIN - LINE - 6.0;
        let per_page = (((body_bottom - body_top) / LINE).floor() as usize).max(1);

        // Rows onto pages, a row kept whole unless it is taller than a page.
        let mut pages: Vec<Vec<(usize, Vec<String>)>> = vec![Vec::new()];
        for (r, cells) in table.rows.iter().enumerate() {
            let lines = row_lines(cells, &widths);
            let used = pages.last().map_or(0, Vec::len);
            if used > 0 && used + lines.len() > per_page && lines.len() <= per_page {
                pages.push(Vec::new());
            }
            for line in lines {
                if pages.last().is_some_and(|p| p.len() >= per_page) {
                    pages.push(Vec::new());
                }
                if let Some(page) = pages.last_mut() {
                    page.push((r, line));
                }
            }
        }

        let metadata =
            Metadata { creator: table.creator.clone(), producer: table.creator.clone(), title: Some(table.title.clone()), ..Metadata::default() };
        let mut writer = Writer::new(Default::default(), metadata);
        let document = writer.document_mut();
        let total = pages.len();
        for (number, lines) in pages.iter().enumerate() {
            let mut pdf_page = document.start_page_with(PageSettings::new(size));
            let mut surface = pdf_page.surface();
            surface.set_fill(Some(Fill::default()));
            let mut y = MARGIN + TITLE_SIZE * 0.8;
            text(&mut surface, &fonts.bold, TITLE_SIZE, &table.title, MARGIN, y);
            y += 4.0 + LINE;
            text(&mut surface, &fonts.regular, SIZE, &table.summary, MARGIN, y);
            y += 8.0;
            for line in &head {
                y += LINE;
                for (c, cell) in line.iter().enumerate() {
                    let x = starts.get(c).copied().unwrap_or(MARGIN);
                    text(&mut surface, &fonts.bold, SIZE, cell, x, y);
                }
            }
            rule(&mut surface, MARGIN, right_edge, y + 4.0);
            if table.rows.is_empty() {
                text(&mut surface, &fonts.regular, SIZE, "Nothing to report yet.", MARGIN, body_top + LINE);
            }
            let mut y = body_top;
            for (r, line) in lines {
                if r % 2 == 1 {
                    band(&mut surface, MARGIN - 3.0, y + 2.0, right_edge - MARGIN + 6.0, LINE);
                }
                y += LINE;
                for (c, cell) in line.iter().enumerate() {
                    let start = starts.get(c).copied().unwrap_or(MARGIN);
                    let width = widths.get(c).copied().unwrap_or(0);
                    let x = if table.right.contains(&c) { start + width.saturating_sub(cell.chars().count()) as f32 * ADVANCE } else { start };
                    text(&mut surface, &fonts.regular, SIZE, cell, x, y);
                }
            }
            let folio = format!("Page {} of {total}", number + 1);
            let x = right_edge - folio.chars().count() as f32 * ADVANCE;
            text(&mut surface, &fonts.regular, SIZE, &folio, x.max(MARGIN), height - MARGIN);
            surface.finish();
            pdf_page.finish();
        }
        Ok(writer.finish()?)
    })
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::indexing_slicing)]
    use super::*;

    #[test]
    fn wrapping_breaks_at_spaces_and_inside_long_words() {
        assert_eq!(wrap("NORA, OWEN, DANA", 11), ["NORA, OWEN,", "DANA"]);
        assert_eq!(wrap("ABCDEFGHIJ", 4), ["ABCD", "EFGH", "IJ"]);
        assert_eq!(wrap("", 5), [""]);
    }

    #[test]
    fn the_widest_column_gives_way_first() {
        let table = PdfTable { header: vec!["Scene".into(), "Heading".into()], rows: vec![vec!["1".into(), "X".repeat(200)]], ..PdfTable::default() };
        let w = widths(&table, 100);
        assert_eq!(w[0], 5);
        assert_eq!(w[0] + w[1] + GAP, 100);
    }
}
