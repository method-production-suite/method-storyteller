//! `cargo xtask corpus`: measures import and pagination against private Final Draft files.
//!
//! The files are the maintainer's own scripts (and possibly other people's), so they never
//! enter the repository and nothing about their content is printed. `STORYTELLER_CORPUS`
//! names a manifest outside the work tree: one `.fdx` path per line, optionally followed by a
//! tab and the `.pdf` Final Draft exported from it. `#` starts a comment.
//!
//! For each file it checks that exporting is stable (export, import, export gives the same file,
//! with every document section kept), then compares
//! each scene's start page with the page Final Draft recorded in the file
//! (`SceneProperties@Page`), each scene's length in eighths with the one it recorded
//! (`SceneProperties@Length`), and the page count with the PDF's. It also counts the script notes
//! that come back the same after `.fdx` to Open Screenplay Format to `.fdx`. Output is numbers only:
//! anonymous file numbers, counts, percentages and page numbers.

use std::path::{Path, PathBuf};
use std::process::Command;

use storyteller_core::layout::Layout;
use storyteller_core::model::ElementKind;
use storyteller_core::{Script, fdx, osf, paginate};

/// Corpus results for one file.
#[derive(Default)]
struct FileResult {
    scenes: usize,
    exact: usize,
    max_drift: u32,
    first_miss: Option<(usize, u32, u32)>,
    pages: usize,
    pdf_pages: Option<usize>,
    round_trip: bool,
    /// Scene lengths compared with Final Draft's: (scenes, exact).
    lengths: (usize, usize),
    /// Script notes: (read, the same after `.fdx` to OSF to `.fdx`).
    notes: (usize, usize),
    /// Revised spans: (read, the same after `.fdx` to OSF to `.fdx`), and whether the revision sets were.
    revised: (usize, usize),
    sets_kept: Option<bool>,
    /// Locked pages: (read, the same after `.fdx` to OSF to `.fdx`), and scene headings whose
    /// recorded page matches the locked page they sit on: (headings, matching).
    locks: (usize, usize),
    lock_pages: (usize, usize),
}

/// Every locked page start, after writing and reading `.fdx` again.
fn locks(script: &Script) -> Vec<String> {
    let reread = fdx::parse(&fdx::write(script)).unwrap_or_default();
    reread
        .elements
        .iter()
        .enumerate()
        .flat_map(|(i, e)| e.page_starts.iter().map(move |p| format!("{i} {} {:?} {:?}", p.at, p.label, p.fdx_attributes)))
        .collect()
}

/// Revised spans of every paragraph, after writing and reading `.fdx` again.
fn revised(script: &Script) -> (Vec<String>, String) {
    let reread = fdx::parse(&fdx::write(script)).unwrap_or_default();
    let spans = reread.elements.iter().enumerate().flat_map(|(i, e)| e.revised.iter().map(move |r| format!("{i} {r:?}"))).collect();
    let sets = format!("{:?} {:?}", reread.revisions, reread.revisions.sets.iter().map(|s| &s.fdx_attributes).collect::<Vec<_>>());
    (spans, sets)
}

/// Every note with what it covers and the Final Draft attributes it is written with.
fn notes(script: &Script) -> Vec<String> {
    let reread = fdx::parse(&fdx::write(script)).unwrap_or_default();
    reread
        .elements
        .iter()
        .enumerate()
        .flat_map(|(index, element)| {
            element.notes.iter().map(move |n| {
                let attributes = n.fdx.as_ref().map(|f| format!("{:?}", f.attributes)).unwrap_or_default();
                format!("{index} {:?} {:?} {:?} {:?} {} {attributes}", n.text, n.range, n.color, n.author, n.unanchored)
            })
        })
        .collect()
}

fn inside(path: &Path, root: &Path) -> bool {
    match (path.canonicalize(), root.canonicalize()) {
        (Ok(path), Ok(root)) => path.starts_with(root),
        _ => false,
    }
}

/// The leading number of a page label such as `12` or `12A`.
fn page_number(label: &str) -> Option<u32> {
    let digits: String = label.trim().chars().take_while(char::is_ascii_digit).collect();
    digits.parse().ok()
}

/// The page Final Draft recorded for a scene heading, from its kept `SceneProperties`.
fn cached_page(children: &str) -> Option<u32> {
    let properties = children.find("<SceneProperties").map(|at| children.get(at..).unwrap_or_default())?;
    let tag = properties.get(..properties.find('>')?)?;
    let value = tag.split(" Page=\"").nth(1)?.split('"').next()?;
    page_number(value)
}

/// The length Final Draft recorded for a scene, in eighths (`1 2/8` is 10).
fn cached_eighths(children: &str) -> Option<u32> {
    let properties = children.find("<SceneProperties").map(|at| children.get(at..).unwrap_or_default())?;
    let tag = properties.get(..properties.find('>')?)?;
    let value = tag.split(" Length=\"").nth(1)?.split('"').next()?.trim();
    let (whole, fraction) = match value.split_once(' ') {
        Some((whole, fraction)) => (whole.parse::<u32>().ok()?, Some(fraction)),
        None if value.contains('/') => (0, Some(value)),
        None => (value.parse::<u32>().ok()?, None),
    };
    let eighths = match fraction {
        Some(f) => f.split_once('/').and_then(|(n, d)| (d.trim() == "8").then(|| n.trim().parse::<u32>().ok()).flatten())?,
        None => 0,
    };
    Some(whole * 8 + eighths)
}

fn pdf_pages(pdf: &Path) -> Option<usize> {
    let output = Command::new("pdfinfo").arg(pdf).output().ok()?;
    String::from_utf8_lossy(&output.stdout).lines().find_map(|l| l.strip_prefix("Pages:").and_then(|n| n.trim().parse().ok()))
}

fn measure(fdx_path: &Path, pdf: Option<&Path>) -> Result<FileResult, String> {
    let bytes = std::fs::read(fdx_path).map_err(|e| format!("unreadable ({})", e.kind()))?;
    let text = storyteller_core::decode_text(&bytes).map_err(|_| "not UTF-8 text".to_string())?;
    let script = fdx::parse(text).map_err(|_| "does not import".to_string())?;
    // Exports refresh scene properties, so compare a second export with the first instead.
    let first = fdx::write(&script);
    let round_trip = fdx::parse(&first).is_ok_and(|again| fdx::write(&again) == first && again.fdx == script.fdx);
    let mut result = FileResult { round_trip, ..FileResult::default() };
    let (spans, sets) = revised(&script);
    if !spans.is_empty() || !script.revisions.is_empty() {
        let (after, after_sets) = osf::parse(&osf::write(&script)).map(|s| revised(&s)).unwrap_or_default();
        result.revised = (spans.len(), spans.iter().zip(&after).filter(|(a, b)| a == b).count());
        result.sets_kept = Some(sets == after_sets);
    }
    if script.pages_locked() {
        let read = locks(&script);
        let after = osf::parse(&osf::write(&script)).map(|s| locks(&s)).unwrap_or_default();
        result.locks = (read.len(), read.iter().zip(&after).filter(|(a, b)| a == b).count());
        // The page each heading starts on, by the locks, against the page Final Draft recorded.
        let mut label: Option<u32> = None;
        for element in &script.elements {
            if let Some(first) = element.page_starts.iter().find(|p| p.at == 0) {
                label = page_number(&first.label);
            }
            if element.kind == ElementKind::SceneHeading
                && let Some(expected) = element.fdx.as_ref().and_then(|f| cached_page(&f.children))
            {
                result.lock_pages.0 += 1;
                result.lock_pages.1 += usize::from(label == Some(expected));
            }
            if let Some(last) = element.page_starts.last() {
                label = page_number(&last.label);
            }
        }
    }
    let before = notes(&script);
    if !before.is_empty() {
        let after = osf::parse(&osf::write(&script)).map(|s| notes(&s)).unwrap_or_default();
        result.notes = (before.len(), before.iter().zip(&after).filter(|(a, b)| a == b).count().min(after.len()));
    }

    let paginated = paginate::paginate(&script, &Layout::for_script(&script));
    result.pages = paginated.page_count() + usize::from(paginated.title_page.is_some());
    for (index, scene) in paginated.scenes.iter().enumerate() {
        let Some(element) = script.elements.get(scene.element) else { continue };
        if element.kind != ElementKind::SceneHeading {
            continue;
        }
        let Some(expected) = element.fdx.as_ref().and_then(|f| cached_page(&f.children)) else { continue };
        result.scenes += 1;
        let drift = scene.page.abs_diff(expected);
        if drift == 0 {
            result.exact += 1;
        } else if result.first_miss.is_none() {
            result.first_miss = Some((index + 1, expected, scene.page));
        }
        result.max_drift = result.max_drift.max(drift);
    }
    let layout = Layout::for_script(&script);
    // Scene lengths in eighths, compared with what Final Draft recorded (omitted scenes aside).
    for scene in paginate::scene_lengths(&script, &paginated, &layout) {
        let Some(expected) = script.elements.get(scene.element).and_then(|e| e.fdx.as_ref()).and_then(|f| cached_eighths(&f.children)) else {
            continue;
        };
        if expected == 0 {
            continue;
        }
        result.lengths.0 += 1;
        result.lengths.1 += usize::from(scene.eighths == expected);
    }
    result.pdf_pages = pdf.and_then(pdf_pages);
    Ok(result)
}

fn percent(part: usize, whole: usize) -> String {
    if whole == 0 { "-".into() } else { format!("{:.1}%", part as f64 * 100.0 / whole as f64) }
}

pub fn run(root: &Path) -> Result<(), String> {
    let manifest = std::env::var_os("STORYTELLER_CORPUS")
        .map(PathBuf::from)
        .ok_or("set STORYTELLER_CORPUS to a manifest outside the repository (see xtask/src/corpus.rs)")?;
    if inside(&manifest, root) {
        return Err("the corpus manifest must live outside the repository".into());
    }
    let list = std::fs::read_to_string(&manifest).map_err(|e| format!("cannot read the manifest: {}", e.kind()))?;
    let entries: Vec<(PathBuf, Option<PathBuf>)> = list
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(|l| {
            let mut parts = l.split('\t');
            (PathBuf::from(parts.next().unwrap_or_default()), parts.next().map(PathBuf::from))
        })
        .collect();

    let (mut files, mut failed, mut round_trips, mut scenes, mut exact, mut all_exact_files, mut page_matches, mut with_pdf) =
        (0, 0, 0, 0, 0, 0, 0, 0);
    let mut max_drift = 0;
    let mut lengths = (0usize, 0usize);
    let mut notes = (0usize, 0usize);
    let (mut revised, mut sets) = ((0usize, 0usize), (0usize, 0usize));
    let (mut locked, mut lock_pages, mut locked_files) = ((0usize, 0usize), (0usize, 0usize), 0usize);
    for (n, (fdx_path, pdf)) in entries.iter().enumerate() {
        let n = n + 1;
        if inside(fdx_path, root) || pdf.as_deref().is_some_and(|p| inside(p, root)) {
            return Err(format!("file {n} is inside the repository; corpus files must stay outside it"));
        }
        files += 1;
        match measure(fdx_path, pdf.as_deref()) {
            Err(why) => {
                failed += 1;
                println!("file {n:>3}: {why}");
            }
            Ok(r) => {
                round_trips += usize::from(r.round_trip);
                lengths.0 += r.lengths.0;
                lengths.1 += r.lengths.1;
                notes.0 += r.notes.0;
                notes.1 += r.notes.1;
                revised.0 += r.revised.0;
                revised.1 += r.revised.1;
                if r.locks.0 > 0 {
                    locked_files += 1;
                }
                locked.0 += r.locks.0;
                locked.1 += r.locks.1;
                lock_pages.0 += r.lock_pages.0;
                lock_pages.1 += r.lock_pages.1;
                if let Some(kept) = r.sets_kept {
                    sets.0 += 1;
                    sets.1 += usize::from(kept);
                }
                scenes += r.scenes;
                exact += r.exact;
                all_exact_files += usize::from(r.scenes > 0 && r.exact == r.scenes);
                max_drift = max_drift.max(r.max_drift);
                let pdf_note = match r.pdf_pages {
                    Some(fd) => {
                        with_pdf += 1;
                        page_matches += usize::from(fd == r.pages);
                        format!(", pages {}/{fd}", r.pages)
                    }
                    None => format!(", pages {}", r.pages),
                };
                let miss = r.first_miss.map(|(s, fd, ours)| format!(", first miss scene {s} (FD p.{fd}, ours p.{ours})")).unwrap_or_default();
                println!(
                    "file {n:>3}: scenes {:>3}, exact {:>6}, max drift {}{pdf_note}{miss}{}",
                    r.scenes,
                    percent(r.exact, r.scenes),
                    r.max_drift,
                    if r.round_trip { "" } else { ", ROUND TRIP DIFFERS" }
                );
            }
        }
    }
    println!("notes: {} read, {} the same after .fdx to OSF to .fdx", notes.0, notes.1);
    println!(
        "locked pages: {locked_files} files, {} pages read, {} the same after .fdx to OSF to .fdx; scene headings on the locked page Final Draft recorded {}/{}",
        locked.0, locked.1, lock_pages.1, lock_pages.0
    );
    println!(
        "revisions: {} revised spans read, {} the same after .fdx to OSF to .fdx; revision sets the same in {}/{} files",
        revised.0, revised.1, sets.1, sets.0
    );
    println!(
        "corpus: {files} files, {failed} failed to import, {round_trips} round-trip identically; scenes exact {exact}/{scenes} ({}), files scene-exact {all_exact_files}, max drift {max_drift} pages; scene lengths exact {}/{} ({}); page count matches PDF {page_matches}/{with_pdf}",
        percent(exact, scenes),
        lengths.1,
        lengths.0,
        percent(lengths.1, lengths.0)
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_cached_page() {
        assert_eq!(cached_page(r#"<SceneProperties Length="2/8" Page="12A" Title=""/>"#), Some(12));
        assert_eq!(cached_page(r#"<SceneProperties Length="2/8" Title=""/>"#), None);
        assert_eq!(cached_page("<OmittedScene/>"), None);
    }
}
