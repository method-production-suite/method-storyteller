//! `cargo xtask screens`: retakes the README and site pictures from the invented Field Station
//! script (SUITE-STANDARDS.md §7). Nothing opens on screen: the app is drawn offscreen by its
//! `screens` test (egui_kittest), and the terminal picture by headless Chrome.
//!
//! Four shots, each in light and dark, in a 1280 by 800 frame at 2x: `hero` (the writing
//! view), `page` (page view at a page break), `import` (the script opened from an .fdx) and
//! `file` (the plain-text file and the command line). Each is written to
//! `assets/screens/<name>-<theme>.png` and `site/img/`. Settings stay in memory.

use std::path::{Path, PathBuf};
use std::process::Command;

const WIDTH: u32 = 1280;
const HEIGHT: u32 = 800;
const THEMES: [&str; 2] = ["light", "dark"];
const APP_SHOTS: [&str; 3] = ["hero", "page", "import"];
/// The settings variable craft-settings reads for the app id `com.methodstoryteller.desktop`;
/// empty means no settings folder at all.
const CONFIG_VAR: &str = "COM_METHODSTORYTELLER_DESKTOP_CONFIG_DIR";
const SAMPLE: &str = "examples/field-station.osf";

pub fn run(root: &Path) -> Result<(), String> {
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    let status = Command::new(&cargo).args(["build", "--release", "-p", "storyteller-cli"]).current_dir(root).status().map_err(|e| e.to_string())?;
    if !status.success() {
        return Err("build failed".into());
    }
    let work = crate::target_dir().join("screens");
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work).map_err(|e| e.to_string())?;
    // The master is Open Screenplay Format; the import shot opens its .fdx, and the terminal
    // shows its Fountain text.
    let script = work.join("field-station.osf");
    std::fs::copy(root.join(SAMPLE), &script).map_err(|e| format!("{SAMPLE}: {e}"))?;
    let cli = crate::target_dir().join("release").join("method-storyteller");
    let fdx = work.join("field-station.fdx");
    run_quiet(Command::new(&cli).arg("convert").arg(&script).arg(&fdx))?;
    let fountain = work.join("field-station.fountain");
    run_quiet(Command::new(&cli).arg("convert").arg(&script).arg(&fountain))?;

    let status = Command::new(&cargo)
        .args([
            "test",
            "--release",
            "-p",
            "method-storyteller",
            "--bin",
            "method-storyteller-app",
            "--",
            "--ignored",
            "--exact",
            "app::screens::screens",
        ])
        .env(CONFIG_VAR, "")
        .env("STORYTELLER_SCREENS_OUT", &work)
        .env("STORYTELLER_SCREENS_SCRIPT", &script)
        .env("STORYTELLER_SCREENS_FDX", &fdx)
        .current_dir(root)
        .status()
        .map_err(|e| e.to_string())?;
    if !status.success() {
        return Err("the offscreen pictures failed".into());
    }

    let outputs = [root.join("assets/screens"), root.join("site/img")];
    for dir in &outputs {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    for theme in THEMES {
        for name in APP_SHOTS {
            copy_out(&work.join(format!("{name}-{theme}.png")), &outputs)?;
        }
        file_shot(&cli, &fountain, &work, theme, &outputs)?;
    }
    println!("screens: 8 pictures in assets/screens and site/img");
    Ok(())
}

fn run_quiet(command: &mut Command) -> Result<String, String> {
    let output = command.output().map_err(|e| e.to_string())?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    } else {
        Err(format!("{command:?} failed: {}", String::from_utf8_lossy(&output.stderr)))
    }
}

fn copy_out(shot: &Path, outputs: &[PathBuf]) -> Result<(), String> {
    let name = shot.file_name().ok_or("no file name")?;
    for dir in outputs {
        std::fs::copy(shot, dir.join(name)).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    println!("  {}", name.to_string_lossy());
    Ok(())
}

/// Shot 4: the script is a plain-text file, and the command line reads it. Real output from
/// the real tools, set in a terminal window by headless Chrome.
fn file_shot(cli: &Path, fountain: &Path, work: &Path, theme: &str, outputs: &[PathBuf]) -> Result<(), String> {
    let text = std::fs::read_to_string(fountain).map_err(|e| e.to_string())?;
    let head: Vec<&str> = text.lines().take(14).collect();
    let info = run_quiet(Command::new(cli).arg("info").arg("field-station.fountain").current_dir(work))?;
    let body = format!(
        "<span class=p>$</span> head -14 field-station.fountain\n{}\n\n<span class=p>$</span> method-storyteller info field-station.fountain\n{}",
        escape(&head.join("\n")),
        escape(info.trim_end())
    );
    let (bg, fg, prompt, bar) =
        if theme == "dark" { ("#1E1E20", "#E4E4E7", "#EE8DA2", "#2A2A2D") } else { ("#FFFFFF", "#26262A", "#AE3D56", "#F6F6F7") };
    let html = format!(
        "<!doctype html><meta charset=utf-8><style>html,body{{margin:0;height:100%;background:{bg};color:{fg}}}\
         .bar{{height:36px;background:{bar};font:13px -apple-system,system-ui,sans-serif;display:flex;align-items:center;justify-content:center;opacity:.8}}\
         pre{{margin:0;padding:20px 28px;font:15px/1.5 ui-monospace,Menlo,monospace;white-space:pre-wrap}}.p{{color:{prompt}}}</style>\
         <div class=bar>field-station</div><pre>{body}</pre>"
    );
    let page = work.join(format!("terminal-{theme}.html"));
    std::fs::write(&page, html).map_err(|e| e.to_string())?;
    let shot = work.join(format!("file-{theme}.png"));
    let chrome = chrome().ok_or("headless Chrome not found (set CHROME to its path)")?;
    run_quiet(
        Command::new(chrome)
            .args(["--headless", "--disable-gpu", "--hide-scrollbars", "--force-device-scale-factor=2"])
            .arg(format!("--window-size={WIDTH},{HEIGHT}"))
            .arg(format!("--screenshot={}", shot.display()))
            .arg(format!("file://{}", page.display())),
    )?;
    copy_out(&shot, outputs)
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

fn chrome() -> Option<PathBuf> {
    if let Ok(path) = std::env::var("CHROME") {
        return Some(PathBuf::from(path));
    }
    ["/Applications/Google Chrome.app/Contents/MacOS/Google Chrome", "/usr/bin/google-chrome", "/usr/bin/chromium"]
        .iter()
        .map(PathBuf::from)
        .find(|p| p.exists())
}
