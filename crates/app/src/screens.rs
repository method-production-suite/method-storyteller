//! The README and site pictures, drawn offscreen: the real app rendered to an image by
//! egui_kittest, with no window, no focus and no input on anyone's screen. Run through
//! `cargo xtask screens`, which sets the paths below and an empty settings folder variable
//! (settings in memory only).

use std::path::{Path, PathBuf};

use egui_kittest::Harness;

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
        .build_eframe(|cc| StorytellerApp::without_menu(cc, Some(file), None, craft_settings::Location::Memory));
    for each in [egui::Theme::Light, egui::Theme::Dark] {
        harness.ctx.style_mut_of(each, |style| style.visuals.text_cursor.blink = false);
    }
    {
        let app = harness.state_mut();
        app.prefs.update(|p| p.view.page_view = shot.page_view);
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
/// `target/gallery`); nothing is committed. Run with
/// `cargo test -p method-storyteller --release gallery -- --ignored`.
#[test]
#[ignore = "pictures for the design pass"]
fn gallery() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let out = var("STORYTELLER_GALLERY_OUT").unwrap_or_else(|| root.join("target/gallery"));
    if let Err(error) = std::fs::create_dir_all(&out) {
        panic!("{}: {error}", out.display());
    }
    let script = root.join("examples/field-station.fountain");
    let states: [(&str, &[&str]); 13] = [
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
    ];
    for theme in [egui::Theme::Light, egui::Theme::Dark] {
        let suffix = if theme == egui::Theme::Dark { "dark" } else { "light" };
        for (name, commands) in states {
            let mut harness = Harness::builder()
                .with_size(SIZE)
                .with_pixels_per_point(PIXELS_PER_POINT)
                .with_theme(theme)
                .with_max_steps(64)
                .build_eframe(|cc| StorytellerApp::without_menu(cc, Some(script.clone()), None, craft_settings::Location::Memory));
            for each in [egui::Theme::Light, egui::Theme::Dark] {
                harness.ctx.style_mut_of(each, |style| style.visuals.text_cursor.blink = false);
            }
            harness.state_mut().status.clear();
            harness.run_steps(4);
            harness.state_mut().editor.pending_focus = Some((6, 0));
            harness.run_steps(6);
            for command in commands {
                harness.state_mut().queued.push((*command).into());
            }
            match name {
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
        }
    }
}
