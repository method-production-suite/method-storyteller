//! Method Storyteller desktop app.
//!
//! `method-storyteller-app [script.fountain|script.fdx] [--control <port>]`

mod app;
mod automation;
mod editor;
mod help;
mod messages;
#[cfg(target_os = "macos")]
mod native_menu;
mod prefs;
mod theme;
mod updates;

use std::path::PathBuf;

fn main() -> eframe::Result {
    let mut control_port = None;
    let mut file = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--control" => control_port = args.next().and_then(|p| p.parse::<u16>().ok()),
            "-V" | "--version" => {
                println!("{}", storyteller_engine::version_line());
                return Ok(());
            }
            other => file = Some(PathBuf::from(other)),
        }
    }

    // Last-resort guard: frame code also runs under `catch_unwind` (see `app.rs`), so a bug
    // shows an error instead of losing the script. The hook only reports.
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        eprintln!("Method Storyteller hit an internal error. A recovery copy of the script is kept; please report this.");
        default_hook(info);
    }));

    prefs::migrate_legacy_folders();
    updates::sweep_at_startup();

    let viewport = egui::ViewportBuilder::default()
        .with_title(storyteller_engine::APP_NAME)
        .with_inner_size([1180.0, 900.0])
        .with_min_inner_size([640.0, 480.0]);
    // macOS: one unified bar. The window's content runs under a transparent title bar, and the
    // app draws its title and controls there beside the traffic lights.
    #[cfg(target_os = "macos")]
    let viewport = viewport.with_fullsize_content_view(true).with_titlebar_shown(false).with_title_shown(false);
    let options = eframe::NativeOptions { viewport, ..Default::default() };
    eframe::run_native(storyteller_engine::APP_NAME, options, Box::new(move |cc| Ok(Box::new(app::StorytellerApp::new(cc, file, control_port)))))
}
