//! `cargo xtask screens`: retakes the README and site pictures from the invented Field Station
//! script (SUITE-STANDARDS.md §7). Needs a desktop session, since it drives the real app.
//!
//! Four shots, each in light and dark, in a 1280 by 800 window: `hero` (the editor),
//! `page` (the formatted page view), `import` (the script opened from an .fdx) and `file`
//! (the plain-text file and the command line, drawn by headless Chrome). Each is written to
//! `assets/screens/<name>-<theme>.png` and `site/img/`. Settings go to a scratch folder, so
//! the run never touches the user's own.

use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::time::{Duration, Instant};

const WIDTH: u32 = 1280;
const HEIGHT: u32 = 800;
const THEMES: [&str; 2] = ["light", "dark"];
/// The settings variable craft-settings reads for the app id `com.methodstoryteller.desktop`.
const CONFIG_VAR: &str = "COM_METHODSTORYTELLER_DESKTOP_CONFIG_DIR";
const SAMPLE: &str = "examples/field-station.fountain";

pub fn run(root: &Path) -> Result<(), String> {
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    let status = Command::new(&cargo)
        .args(["build", "--release", "-p", "method-storyteller", "-p", "storyteller-cli"])
        .current_dir(root)
        .status()
        .map_err(|e| e.to_string())?;
    if !status.success() {
        return Err("build failed".into());
    }
    let bin = root.join("target/release");
    let work = root.join("target/screens");
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work).map_err(|e| e.to_string())?;
    let fountain = work.join("field-station.fountain");
    std::fs::copy(root.join(SAMPLE), &fountain).map_err(|e| format!("{SAMPLE}: {e}"))?;
    let fdx = work.join("field-station.fdx");
    let cli = bin.join("method-storyteller");
    run_quiet(Command::new(&cli).arg("convert").arg(&fountain).arg(&fdx))?;

    let outputs = [root.join("assets/screens"), root.join("site/img")];
    for dir in &outputs {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let app = bin.join("method-storyteller-app");
    for theme in THEMES {
        // Every shot sets its own view, and every launch gets fresh settings, so nothing
        // carries over from one shot or theme to the next.
        let frame = |pages: bool| format!(r#"{{"width":{WIDTH},"height":{HEIGHT},"theme":"{theme}","page_view":{pages}}}"#);
        let mut session = AppSession::launch(&app, &fountain, &work, &format!("{theme}-script"))?;
        // The writing view: one continuous column.
        session.call("ui.window", &frame(false))?;
        session.call("ui.focus", r#"{"element":6,"offset":0}"#)?;
        session.shoot(&work, "hero", theme, &outputs)?;
        drop(session);

        // Page view, scrolled to the break between pages 1 and 2. A launch of its own, so
        // the scroll starts from the top every run.
        let mut session = AppSession::launch(&app, &fountain, &work, &format!("{theme}-page"))?;
        session.call("ui.window", &frame(true))?;
        std::thread::sleep(Duration::from_millis(800));
        session.call("ui.focus", r#"{"element":23,"offset":0}"#)?;
        session.shoot(&work, "page", theme, &outputs)?;
        drop(session);

        let mut session = AppSession::launch(&app, &fdx, &work, &format!("{theme}-import"))?;
        session.call("ui.window", &frame(true))?;
        session.call("ui.focus", r#"{"element":0,"offset":0}"#)?;
        session.shoot(&work, "import", theme, &outputs)?;
        drop(session);

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

/// The desktop app under the control channel. Dropping it ends the app.
struct AppSession {
    child: Child,
    reader: BufReader<TcpStream>,
    writer: TcpStream,
    next_id: u64,
}

impl AppSession {
    fn launch(app: &Path, file: &Path, work: &Path, name: &str) -> Result<Self, String> {
        let config = work.join("config").join(name);
        std::fs::create_dir_all(&config).map_err(|e| e.to_string())?;
        let port = free_port()?;
        let child = Command::new(app)
            .arg(file)
            .args(["--control", &port.to_string()])
            .env(CONFIG_VAR, config)
            .spawn()
            .map_err(|e| format!("{}: {e}", app.display()))?;
        let mut child = child;
        let started = Instant::now();
        let stream = loop {
            if let Ok(Some(status)) = child.try_wait() {
                return Err(format!("the app exited before opening its control channel ({status})"));
            }
            match TcpStream::connect(("127.0.0.1", port)) {
                Ok(stream) => break stream,
                Err(_) if started.elapsed() < Duration::from_secs(30) => std::thread::sleep(Duration::from_millis(200)),
                Err(e) => return Err(format!("the app did not open its control channel: {e}")),
            }
        };
        let writer = stream.try_clone().map_err(|e| e.to_string())?;
        let mut session = AppSession { child, reader: BufReader::new(stream), writer, next_id: 1 };
        session.call("app.ping", "{}")?;
        Ok(session)
    }

    fn call(&mut self, method: &str, params: &str) -> Result<String, String> {
        let id = self.next_id;
        self.next_id += 1;
        writeln!(self.writer, r#"{{"id":{id},"method":"{method}","params":{params}}}"#).map_err(|e| e.to_string())?;
        let mut reply = String::new();
        self.reader.read_line(&mut reply).map_err(|e| e.to_string())?;
        if reply.contains(r#""error""#) {
            return Err(format!("{method}: {}", reply.trim()));
        }
        Ok(reply)
    }

    /// Lets the window settle (resize, theme, layout), then saves one picture.
    fn shoot(&mut self, work: &Path, name: &str, theme: &str, outputs: &[PathBuf]) -> Result<(), String> {
        std::thread::sleep(Duration::from_millis(1500));
        let shot = work.join(format!("{name}-{theme}.png"));
        let path = shot.display().to_string().replace('\\', "\\\\").replace('"', "\\\"");
        self.call("ui.screenshot", &format!(r#"{{"path":"{path}"}}"#))?;
        copy_out(&shot, outputs)
    }
}

impl Drop for AppSession {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// A port nothing is listening on, so the run never drives another process.
fn free_port() -> Result<u16, String> {
    let listener = TcpListener::bind(("127.0.0.1", 0)).map_err(|e| e.to_string())?;
    listener.local_addr().map(|a| a.port()).map_err(|e| e.to_string())
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
