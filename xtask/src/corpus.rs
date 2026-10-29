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
    deletions: (usize, usize),
    sets_kept: Option<bool>,
    /// Locked pages: (read, the same after `.fdx` to OSF to `.fdx`), and scene headings whose
    /// recorded page matches the locked page they sit on: (headings, matching).
    locks: (usize, usize),
    lock_pages: (usize, usize),
    /// Paginated with the locks: (pages, pages printed with a locked number, A-pages or
    /// combined pages added), and scene headings printed on the page Final Draft recorded,
    /// compared as labels such as `12A`: (headings, matching, matching with the locks ignored).
    locked_print: (usize, usize, usize),
    locked_scenes: (usize, usize, usize),
    /// Locks read inside a paragraph: (read, landing at a word start).
    lock_words: (usize, usize),
    /// Locking the pages as they print now, then paginating: (pages, the same page by page),
    /// for scripts that were not locked.
    relocked: (usize, usize),
    /// The page layout and element settings, and the layout they paginate with, the same after
    /// `.fdx` to OSF and after `.fdx` to OSF to `.fdx`.
    format_kept: bool,
    layout_kept: bool,
    /// Words spell check ignores: (read, the same after `.fdx` to OSF to `.fdx`).
    ignored: (usize, usize),
}

/// The format without the XML each file format keeps for writing it back.
fn bare_format(script: &Script) -> storyteller_core::Format {
    let mut format = script.format.clone();
    if let Some(page) = format.page.as_mut() {
        page.fdx_page_layout.clear();
        page.fdx_mores.clear();
        page.fdx_scene_numbers.clear();
        page.osf_settings.clear();
    }
    for element in &mut format.elements {
        element.fdx_xml.clear();
        element.osf_xml.clear();
    }
    format
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
fn revised(script: &Script) -> (Vec<String>, Vec<String>, String) {
    let reread = fdx::parse(&fdx::write(script)).unwrap_or_default();
    let spans = reread.elements.iter().enumerate().flat_map(|(i, e)| e.revised.iter().map(move |r| format!("{i} {r:?}"))).collect();
    let deletions = reread.elements.iter().enumerate().flat_map(|(i, e)| e.deletions.iter().map(move |d| format!("{i} {d:?}"))).collect();
    let sets = format!("{:?} {:?}", reread.revisions, reread.revisions.sets.iter().map(|s| &s.fdx_attributes).collect::<Vec<_>>());
    (spans, deletions, sets)
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
    page_number(&cached_label(children)?)
}

/// That page as Final Draft printed it, such as `12A`.
fn cached_label(children: &str) -> Option<String> {
    let properties = children.find("<SceneProperties").map(|at| children.get(at..).unwrap_or_default())?;
    let tag = properties.get(..properties.find('>')?)?;
    Some(tag.split(" Page=\"").nth(1)?.split('"').next()?.trim().to_string())
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
    let via_osf = osf::parse(&osf::write(&script)).unwrap_or_default();
    let back = fdx::parse(&fdx::write(&via_osf)).unwrap_or_default();
    result.ignored =
        (script.spelling_ignored.len(), if back.spelling_ignored == script.spelling_ignored { script.spelling_ignored.len() } else { 0 });
    result.format_kept = bare_format(&via_osf) == bare_format(&script) && bare_format(&back) == bare_format(&script);
    let layout = Layout::for_script(&script);
    result.layout_kept = Layout::for_script(&via_osf) == layout && Layout::for_script(&back) == layout;
    let (spans, marks, sets) = revised(&script);
    if !spans.is_empty() || !marks.is_empty() || !script.revisions.is_empty() {
        let (after, after_marks, after_sets) = osf::parse(&osf::write(&script)).map(|s| revised(&s)).unwrap_or_default();
        result.revised = (spans.len(), spans.iter().zip(&after).filter(|(a, b)| a == b).count());
        result.deletions =
            (marks.len(), if marks == after_marks { marks.len() } else { marks.iter().zip(&after_marks).filter(|(a, b)| a == b).count() });
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
    if !script.pages_locked() {
        // Lock Pages freezes the breaks as they are: paginating again changes nothing.
        let mut locked = script.clone();
        paginate::lock_pages(&mut locked, &paginated);
        let again = paginate::paginate(&locked, &Layout::for_script(&locked));
        let same = |a: &paginate::Page, b: &paginate::Page| a.label() == b.label() && a.lines == b.lines;
        result.relocked = (paginated.pages.len(), paginated.pages.iter().zip(&again.pages).filter(|(a, b)| same(a, b)).count());
        if again.pages.len() != paginated.pages.len() {
            result.relocked.1 = result.relocked.1.min(paginated.pages.len().saturating_sub(1));
        }
    }
    if script.pages_locked() {
        // Imported locks paginate as unlocked for now; measure them kept, to see where they stand.
        let paginated = paginate::paginate_with(&script, &Layout::for_script(&script), true);
        // A page cannot start inside a word: locks read inside a paragraph should land on a word.
        for element in &script.elements {
            let plain: Vec<char> = storyteller_core::inline::plain(&element.text).chars().collect();
            for start in element.page_starts.iter().filter(|p| p.at > 0) {
                let at = usize::try_from(start.at).unwrap_or(usize::MAX);
                result.lock_words.0 += 1;
                result.lock_words.1 += usize::from(at >= plain.len() || plain.get(at - 1).is_some_and(|c| c.is_whitespace()));
            }
        }
        let locked: std::collections::HashSet<&str> = script.elements.iter().flat_map(|e| &e.page_starts).map(|p| p.label.as_str()).collect();
        let labels: Vec<String> = paginated.pages.iter().map(|p| p.label()).collect();
        let mut unlocked = script.clone();
        unlocked.elements.iter_mut().for_each(|e| e.page_starts.clear());
        let unlocked = paginate::paginate(&unlocked, &Layout::for_script(&unlocked));
        let kept = labels.iter().filter(|l| locked.contains(l.as_str())).count();
        result.locked_print = (labels.len(), kept, labels.len() - kept);
        for scene in &paginated.scenes {
            let Some(expected) = script.elements.get(scene.element).and_then(|e| e.fdx.as_ref()).and_then(|f| cached_label(&f.children)) else {
                continue;
            };
            let ours = usize::try_from(scene.page).ok().and_then(|p| p.checked_sub(1)).and_then(|p| labels.get(p));
            result.locked_scenes.0 += 1;
            result.locked_scenes.1 += usize::from(ours.is_some_and(|l| l.eq_ignore_ascii_case(&expected)));
            let plain = unlocked.scenes.iter().find(|s| s.element == scene.element).map(|s| s.page.to_string());
            result.locked_scenes.2 += usize::from(plain.is_some_and(|l| l.eq_ignore_ascii_case(&expected)));
        }
    }
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
    let mut deleted = (0usize, 0usize);
    let (mut locked, mut lock_pages, mut locked_files) = ((0usize, 0usize), (0usize, 0usize), 0usize);
    let (mut locked_print, mut locked_scenes) = ((0usize, 0usize, 0usize), (0usize, 0usize, 0usize));
    let mut lock_words = (0usize, 0usize);
    let mut relocked = (0usize, 0usize);
    let (mut formats, mut layouts) = (0usize, 0usize);
    let mut ignored = (0usize, 0usize);
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
                deleted.0 += r.deletions.0;
                deleted.1 += r.deletions.1;
                formats += usize::from(r.format_kept);
                ignored.0 += r.ignored.0;
                ignored.1 += r.ignored.1;
                layouts += usize::from(r.layout_kept);
                if r.locks.0 > 0 {
                    locked_files += 1;
                }
                locked.0 += r.locks.0;
                locked.1 += r.locks.1;
                lock_pages.0 += r.lock_pages.0;
                lock_pages.1 += r.lock_pages.1;
                locked_print = (locked_print.0 + r.locked_print.0, locked_print.1 + r.locked_print.1, locked_print.2 + r.locked_print.2);
                relocked = (relocked.0 + r.relocked.0, relocked.1 + r.relocked.1);
                lock_words = (lock_words.0 + r.lock_words.0, lock_words.1 + r.lock_words.1);
                locked_scenes = (locked_scenes.0 + r.locked_scenes.0, locked_scenes.1 + r.locked_scenes.1, locked_scenes.2 + r.locked_scenes.2);
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
    println!("spelling: {} ignored words read, {} the same after .fdx to OSF to .fdx", ignored.0, ignored.1);
    println!(
        "format: element settings and page layout the same after .fdx to OSF and back in {formats} files, the layout they paginate with in {layouts}"
    );
    println!(
        "locked pages: {locked_files} files, {} pages read, {} the same after .fdx to OSF to .fdx; scene headings on the locked page Final Draft recorded {}/{}",
        locked.0, locked.1, lock_pages.1, lock_pages.0
    );
    println!(
        "locked pagination (kept, though imported locks paginate unlocked for now): {} locks inside a paragraph, {} at a word start; {} pages, {} printed with a locked number, {} A-pages or combined pages added; scene headings on the page Final Draft printed {}/{} ({}/{} with the locks ignored)",
        lock_words.0,
        lock_words.1,
        locked_print.0,
        locked_print.1,
        locked_print.2,
        locked_scenes.1,
        locked_scenes.0,
        locked_scenes.2,
        locked_scenes.0
    );
    println!("locking: {} pages locked as they print, {} the same page by page when paginated locked", relocked.0, relocked.1);
    println!(
        "revisions: {} revised spans read, {} the same after .fdx to OSF to .fdx; {} deletion marks read, {} the same; revision sets the same in {}/{} files",
        revised.0, revised.1, deleted.0, deleted.1, sets.1, sets.0
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
