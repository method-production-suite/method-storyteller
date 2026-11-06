//! The README and site pictures, drawn offscreen: the real app rendered to an image by
//! egui_kittest, with no window, no focus and no input on anyone's screen. Run through
//! `cargo xtask screens`, which sets the paths below and an empty settings folder variable
//! (settings in memory only).

use std::path::{Path, PathBuf};

use egui_kittest::Harness;
use storyteller_core::model::TvShape;
use storyteller_engine::Caret;

use super::StorytellerApp;

/// The suite's frame: 1280 by 800 logical pixels, drawn at 2x.
const SIZE: egui::Vec2 = egui::vec2(1280.0, 800.0);
const PIXELS_PER_POINT: f32 = 2.0;

struct Shot {
    name: &'static str,
    /// `true` for the .fdx copy of the script.
    fdx: bool,
    page_view: bool,
    /// Element the caret goes to; the view scrolls to it.
    focus: usize,
}

const SHOTS: [Shot; 3] = [
    // The writing view: one continuous column.
    Shot { name: "hero", fdx: false, page_view: false, focus: 6 },
    // Page view at the break between pages 1 and 2.
    Shot { name: "page", fdx: false, page_view: true, focus: 25 },
    // The same script opened from an .fdx file.
    Shot { name: "import", fdx: true, page_view: true, focus: 0 },
];

fn var(name: &str) -> Option<PathBuf> {
    std::env::var_os(name).map(PathBuf::from)
}

/// Draws every shot in light and dark into `STORYTELLER_SCREENS_OUT`.
#[test]
#[ignore = "run by `cargo xtask screens`"]
fn screens() {
    let (Some(out), Some(script), Some(fdx)) = (var("STORYTELLER_SCREENS_OUT"), var("STORYTELLER_SCREENS_SCRIPT"), var("STORYTELLER_SCREENS_FDX"))
    else {
        panic!("run through `cargo xtask screens`");
    };
    for theme in [egui::Theme::Light, egui::Theme::Dark] {
        let suffix = if theme == egui::Theme::Dark { "dark" } else { "light" };
        for shot in &SHOTS {
            let file = if shot.fdx { &fdx } else { &script };
            let (width, height, rgba) = draw(file, theme, shot);
            let path = out.join(format!("{}-{suffix}.png", shot.name));
            if let Err(error) = write_png(&path, width, height, &rgba) {
                panic!("{}: {error}", path.display());
            }
            println!("  {}", path.display());
        }
    }
}

/// The shot as RGBA pixels: width, height, bytes.
fn draw(file: &Path, theme: egui::Theme, shot: &Shot) -> (u32, u32, Vec<u8>) {
    let file = file.to_path_buf();
    let mut harness = Harness::builder()
        .with_size(SIZE)
        .with_pixels_per_point(PIXELS_PER_POINT)
        .with_theme(theme)
        .with_max_steps(64)
        .build_eframe(|cc| StorytellerApp::without_menu(cc, Some(file), None, craft_settings::Location::Memory, Default::default()));
    for each in [egui::Theme::Light, egui::Theme::Dark] {
        harness.ctx.style_mut_of(each, |style| style.visuals.text_cursor.blink = false);
    }
    {
        let app = harness.state_mut();
        // No spelling underlines in the pictures: Field Station's invented names and British
        // spellings are not in the US dictionary.
        app.prefs.update(|p| {
            p.view.page_view = shot.page_view;
            p.view.check_spelling = false;
        });
        app.status.clear();
        app.pictures = true;
    }
    // Lay out the view first, then put the caret in place so the scroll lands from there.
    harness.run_steps(4);
    harness.state_mut().editor.pending_focus = Some((shot.focus, 0));
    harness.run_steps(12);
    match harness.render() {
        Ok(image) => (image.width(), image.height(), image.into_raw()),
        Err(error) => panic!("rendering {}: {error}", shot.name),
    }
}

fn write_png(path: &Path, width: u32, height: u32, rgba: &[u8]) -> Result<(), String> {
    let mut out = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut out, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().map_err(|e| e.to_string())?;
        writer.write_image_data(rgba).map_err(|e| e.to_string())?;
    }
    std::fs::write(path, out).map_err(|e| e.to_string())
}

/// Every surface in light and dark, for the design pass: the dialogs, the Help windows, the
/// find bar and a selection on the page. Written to `STORYTELLER_GALLERY_OUT` (default
/// `gallery` beside the build: the shared cache, or `target/`); nothing is committed. Run with
/// `cargo test -p method-storyteller --release gallery -- --ignored`.
#[test]
#[ignore = "pictures for the design pass"]
fn gallery() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    // Beside the build: the shared cache when CARGO_TARGET_DIR names it, else ./target.
    let target = var("CARGO_TARGET_DIR").filter(|d| !d.as_os_str().is_empty()).unwrap_or_else(|| root.join("target"));
    let out = var("STORYTELLER_GALLERY_OUT").unwrap_or_else(|| target.join("gallery"));
    if let Err(error) = std::fs::create_dir_all(&out) {
        panic!("{}: {error}", out.display());
    }
    let script = root.join("examples/field-station.fountain");
    // One shot by name, when only that one is wanted.
    let only = std::env::var("STORYTELLER_GALLERY_ONLY").ok();
    let states: [(&str, &[&str]); 32] = [
        ("autocomplete-lists", &["app.lists"]),
        ("go-to-act", &["app.go_to"]),
        ("go-to-bookmark", &["app.go_to"]),
        ("add-bookmark", &["app.add_bookmark"]),
        ("new-tv", &["file.new_tv"]),
        ("navigator-tv", &[]),
        ("navigator-scenes", &[]),
        ("scene-properties", &[]),
        ("characters", &["app.characters"]),
        ("recovery", &[]),
        ("styled-text", &[]),
        ("multi-cam-page", &[]),
        ("open-recent", &[]),
        ("spelling", &["app.spelling"]),
        ("picker", &[]),
        ("selection-across", &[]),
        ("find", &["app.find"]),
        ("go-to", &["app.go_to"]),
        ("rename", &["app.rename_character"]),
        ("title-page", &["app.title_page"]),
        ("about", &["help.about"]),
        ("shortcuts", &["help.shortcuts"]),
        ("whats-new", &["help.whats_new"]),
        ("unsaved", &[]),
        ("selection", &[]),
        ("empty-script", &["file.new"]),
        ("nothing-found", &["app.find"]),
        ("open-failed", &[]),
        ("save-failed", &[]),
        ("revised-page", &[]),
        ("export-pdf", &["file.export_pdf"]),
        ("reports", &["app.reports"]),
    ];
    for theme in [egui::Theme::Light, egui::Theme::Dark] {
        let suffix = if theme == egui::Theme::Dark { "dark" } else { "light" };
        for (name, commands) in states {
            if only.as_deref().is_some_and(|only| only != name) {
                continue;
            }
            let mut harness =
                Harness::builder().with_size(SIZE).with_pixels_per_point(PIXELS_PER_POINT).with_theme(theme).with_max_steps(64).build_eframe(|cc| {
                    StorytellerApp::without_menu(cc, Some(script.clone()), None, craft_settings::Location::Memory, Default::default())
                });
            for each in [egui::Theme::Light, egui::Theme::Dark] {
                harness.ctx.style_mut_of(each, |style| style.visuals.text_cursor.blink = false);
            }
            harness.state_mut().status.clear();
            harness.state_mut().prefs.update(|p| p.view.check_spelling = false);
            harness.run_steps(4);
            harness.state_mut().editor.pending_focus = Some((6, 0));
            harness.run_steps(6);
            for command in commands {
                harness.state_mut().queued.push((*command).into());
            }
            let recovery_dir = tempfile::tempdir().ok();
            match name {
                "go-to-act" | "go-to-bookmark" => {
                    let app = harness.state_mut();
                    for (index, bookmark) in [(6, "The map"), (120, "The turn"), (400, "Last act")] {
                        if let Some(element) = app.session.script.elements.get_mut(index) {
                            element.bookmark = Some(bookmark.into());
                        }
                    }
                    harness.run_steps(3);
                    if let Some(draft) = harness.state_mut().go_to.as_mut() {
                        draft.mode = if name == "go-to-act" { super::GoToMode::Act } else { super::GoToMode::Bookmark };
                    }
                }
                "navigator-tv" | "multi-cam-page" => {
                    let shape = if name == "navigator-tv" { TvShape::OneHour } else { TvShape::MultiCam };
                    let app = harness.state_mut();
                    app.run("file.new_tv", super::params(&[("shape", serde_json::json!(shape.id()))]));
                    if name == "multi-cam-page" {
                        // An invented scene on the first lettered page.
                        let at = app.session.script.scene_indices().first().copied().unwrap_or(0);
                        let lines = "INT. DINER - DAY\n\nJO WIPES THE COUNTER. BEN WATCHES THE DOOR.\n\nJO\nYou're early.\n\nBEN\nI'm hiding.\n";
                        app.run("edit.paste_screenplay", super::params(&[("index", serde_json::json!(at)), ("text", serde_json::json!(lines))]));
                        app.session.caret = Caret { element: at, offset: 0 };
                        app.editor.pending_focus = Some((at, 0));
                    }
                    app.status.clear();
                }
                "navigator-scenes" | "scene-properties" => {
                    let app = harness.state_mut();
                    let headings = app.session.script.scene_indices();
                    let notes = [
                        ("The long walk", "#E06666"),
                        ("Nobody home", "#FFD966"),
                        ("The radio", "#93C47D"),
                        ("Up the mast", "#6FA8DC"),
                        ("Static", "#C27BA0"),
                    ];
                    for (&heading, (title, colour)) in headings.iter().zip(notes) {
                        if let Some(element) = app.session.script.elements.get_mut(heading) {
                            element.scene.title = title.into();
                            element.scene.color = Some(colour.into());
                            element.scene.summary = "Mara and Delacroix reach the station and find it empty.".into();
                        }
                    }
                    if name == "scene-properties" {
                        app.open_scene_dialog(headings.first().copied().unwrap_or(0));
                    }
                    harness.run_steps(3);
                    if name == "navigator-scenes" {
                        for each in [egui::Theme::Light, egui::Theme::Dark] {
                            harness.ctx.style_mut_of(each, |style| style.interaction.tooltip_delay = 0.0);
                        }
                        let at = harness.state().automation.find("navigator.scene.1").map(|row| row.rect.center());
                        if let Some(at) = at {
                            harness.input_mut().events.push(egui::Event::PointerMoved(at));
                        }
                    }
                }
                "recovery" => {
                    if let Some(dir) = &recovery_dir {
                        let _ = crate::recovery::write(dir.path(), "Field Station", b"INT. LOST - NIGHT\n\nThe unsaved line.\n");
                        harness.state_mut().recovery_dir = Some(dir.path().to_path_buf());
                        harness.state_mut().find_recovery();
                    }
                }
                "styled-text" => {
                    let app = harness.state_mut();
                    if let Some(element) = app.session.script.elements.get_mut(2) {
                        element.text = "A **bold** word, an *italic* word and an _underlined_ one, each with its markup faint.".into();
                    }
                    app.session.invalidate_layout();
                    app.editor.pending_focus = Some((0, 0));
                }
                "open-recent" => {
                    let app = harness.state_mut();
                    for script in ["Pilot.mstory", "Field Station.osf", "Second Act.fountain"] {
                        app.recent.push(&format!("/scripts/{script}"));
                    }
                    use egui_kittest::kittest::Queryable;
                    harness.run_steps(3);
                    harness.get_by_label("File").click();
                    harness.run_steps(3);
                    harness.get_by_label_contains("Open Recent").hover();
                }
                "spelling" => {
                    harness.state_mut().prefs.update(|p| p.view.check_spelling = true);
                }
                "picker" => {
                    let app = harness.state_mut();
                    app.run("element.insert", super::params(&[("index", serde_json::json!(5)), ("kind", serde_json::json!("action"))]));
                    app.session.caret = Caret { element: 6, offset: 0 };
                    app.editor.pending_focus = Some((6, 0));
                    app.editor.picker = Some(crate::editor::Picker { index: 6, selected: 1 });
                    app.status.clear();
                }
                "selection-across" => {
                    let app = harness.state_mut();
                    let edits = app.session.edits();
                    app.editor.span = Some(crate::editor::Span::between(Caret { element: 6, offset: 12 }, Caret { element: 10, offset: 8 }, edits));
                }
                "find" | "nothing-found" => {
                    harness.run_steps(3);
                    if let Some(bar) = harness.state_mut().find.as_mut() {
                        bar.text = if name == "find" { "drum" } else { "lighthouse" }.into();
                    }
                }
                "open-failed" => harness.state_mut().open_path(&root.join("examples/Missing Draft.fdx")),
                "save-failed" => {
                    let app = harness.state_mut();
                    app.session.path = Some(root.join("examples/no such folder/Field Station.fountain"));
                    app.save(false);
                }
                "unsaved" => {
                    harness.state_mut().session.dirty = true;
                    harness.state_mut().prompt = Some(super::Pending::Close);
                }
                "revised-page" => {
                    // Blue and Pink sets on the first page, Pink active, in page colours.
                    use storyteller_core::model::{Deletion, Revised};
                    let app = harness.state_mut();
                    app.prefs.update(|p| p.view.page_view = true);
                    let script = &mut app.session.script;
                    for date in ["", "10/14/26", "10/20/26"] {
                        script.add_revision_set(date);
                    }
                    if let Some(element) = script.elements.get_mut(2) {
                        element.revised.push(Revised { from: 0, to: 24, set: 2 });
                    }
                    if let Some(element) = script.elements.get_mut(6) {
                        element.revised.push(Revised { from: 4, to: 30, set: 3 });
                        element.deletions.push(Deletion { at: 60, set: 3 });
                    }
                    app.session.page_colours = true;
                    app.session.invalidate_layout();
                    harness.state_mut().editor.pending_focus = Some((0, 0));
                }
                "selection" => {
                    harness.run_steps(2);
                    harness.key_press_modifiers(egui::Modifiers::SHIFT, egui::Key::End);
                }
                _ => {}
            }
            harness.run_steps(12);
            let image = match harness.render() {
                Ok(image) => image,
                Err(error) => panic!("rendering {name}: {error}"),
            };
            let path = out.join(format!("{name}-{suffix}.png"));
            if let Err(error) = write_png(&path, image.width(), image.height(), &image.into_raw()) {
                panic!("{}: {error}", path.display());
            }
            println!("  {}", path.display());
            drop(recovery_dir);
        }
    }
    if only.is_none() {
        contact_sheet(&out, &states.map(|(name, _)| name));
    }
}

/// The owner's one picture: every shot, light beside dark, labelled.
fn contact_sheet(out: &Path, names: &[&str]) {
    let thumb = egui::vec2(320.0, 200.0);
    let per_row = 3.0;
    let rows = (names.len() as f32 / per_row).ceil();
    let size = egui::vec2((thumb.x * 2.0 + 24.0) * per_row + 16.0, (thumb.y + 36.0) * rows + 16.0);
    let mut textures: Vec<(String, egui::TextureHandle, egui::TextureHandle)> = Vec::new();
    let names: Vec<String> = names.iter().map(ToString::to_string).collect();
    let dir = out.to_path_buf();
    let mut harness = Harness::builder().with_size(size).with_pixels_per_point(1.0).with_theme(egui::Theme::Light).build_ui(move |ui| {
        if textures.is_empty() {
            for name in &names {
                let load = |suffix: &str| {
                    let image = read_png(&dir.join(format!("{name}-{suffix}.png")));
                    ui.ctx().load_texture(format!("{name}-{suffix}"), image, egui::TextureOptions::LINEAR)
                };
                textures.push((name.clone(), load("light"), load("dark")));
            }
        }
        egui::Grid::new("sheet").spacing(egui::vec2(16.0, 8.0)).show(ui, |ui| {
            for (n, (name, light, dark)) in textures.iter().enumerate() {
                ui.vertical(|ui| {
                    ui.label(egui::RichText::new(name).strong());
                    ui.horizontal(|ui| {
                        ui.add(egui::Image::new(light).fit_to_exact_size(thumb));
                        ui.add(egui::Image::new(dark).fit_to_exact_size(thumb));
                    });
                });
                if (n + 1) % 3 == 0 {
                    ui.end_row();
                }
            }
        });
    });
    harness.run_steps(3);
    let image = match harness.render() {
        Ok(image) => image,
        Err(error) => panic!("rendering the contact sheet: {error}"),
    };
    let path = out.join("contact-sheet.png");
    if let Err(error) = write_png(&path, image.width(), image.height(), &image.into_raw()) {
        panic!("{}: {error}", path.display());
    }
    println!("  {}", path.display());
}

fn read_png(path: &Path) -> egui::ColorImage {
    let bytes = std::fs::read(path).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    let mut reader = png::Decoder::new(std::io::Cursor::new(bytes)).read_info().unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    let mut data = vec![0; reader.output_buffer_size().unwrap_or(0)];
    let info = reader.next_frame(&mut data).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    // A quarter the width and height, each pixel the mean of four across: a thumbnail, and within
    // the largest texture the renderer takes.
    let (width, height) = (info.width as usize, info.height as usize);
    let step = 4;
    let (small_width, small_height) = (width / step, height / step);
    let mut small = Vec::with_capacity(small_width * small_height * 4);
    for y in 0..small_height {
        for x in 0..small_width {
            for channel in 0..4 {
                let sum: u32 = (0..step * step)
                    .map(|n| {
                        let at = ((y * step + n / step) * width + x * step + n % step) * 4 + channel;
                        u32::from(data.get(at).copied().unwrap_or(255))
                    })
                    .sum();
                small.push((sum / (step * step) as u32) as u8);
            }
        }
    }
    egui::ColorImage::from_rgba_unmultiplied([small_width, small_height], &small)
}
