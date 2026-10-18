//! `method-storyteller`: the Method Storyteller command line.
//!
//! Two binaries share this entry point: `method-storyteller` and the deprecated alias `mst`,
//! which behaves identically apart from one note on stderr.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use serde_json::{Map, Value};
use storyteller_core::ElementKind;
use storyteller_core::paginate::render_text;
use storyteller_engine::control::{Headless, handle_line, serve};
use storyteller_engine::files::{self, FileFormat};
use storyteller_engine::{Registry, Session};

/// The command's name, used in help, usage and error messages.
pub const BIN_NAME: &str = storyteller_engine::BIN_NAME;

/// Printed on stderr when the CLI runs under its old name.
pub const ALIAS_NOTE: &str = "note: `mst` is now `method-storyteller`; the old name will be removed in a future release.";

const USAGE: &str = "\
Method Storyteller command line

USAGE:
    method-storyteller convert <input> <output>        Convert between .fountain, .fdx and .pdf (by extension)
    method-storyteller pdf <input> [output.pdf]        Export a PDF (default: next to the input)
    method-storyteller paginate <input> [--text]       Page count and scenes; --text prints the pages
    method-storyteller info <input>                    Scenes, characters and pages
    method-storyteller run <input> <command> [k=v…]    Run an engine command, then save the file
    method-storyteller commands                        List engine commands
    method-storyteller serve [--control <port>] [file] Headless JSON-lines control server (default port 9876)
";

/// Which name the CLI was started under.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Invocation {
    /// `method-storyteller`.
    Primary,
    /// The deprecated `mst` alias: same behaviour, plus [`ALIAS_NOTE`] on stderr.
    Alias,
}

/// Shared `main` for both binaries: parses the process arguments and runs the command.
pub fn main(invocation: Invocation) -> ExitCode {
    if invocation == Invocation::Alias {
        // stderr only: stdout stays byte-identical to `method-storyteller`. No subcommand
        // speaks a protocol on stdio (`serve` listens on TCP), so the note is always safe.
        eprintln!("{ALIAS_NOTE}");
    }
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("{BIN_NAME}: {message}");
            ExitCode::FAILURE
        }
    }
}

fn open(path: &Path) -> Result<Session, String> {
    let (script, format) = files::open(path).map_err(|e| e.to_string())?;
    let mut session = Session::new(script);
    session.path = Some(path.to_path_buf());
    session.format = format;
    Ok(session)
}

fn arg(args: &[String], index: usize, what: &str) -> Result<PathBuf, String> {
    args.get(index).map(PathBuf::from).ok_or_else(|| format!("missing {what}\n\n{USAGE}"))
}

/// Runs one command line (without the program name).
pub fn run(args: &[String]) -> Result<(), String> {
    let Some(command) = args.first() else {
        print!("{USAGE}");
        return Ok(());
    };
    match command.as_str() {
        "convert" => {
            let input = arg(args, 1, "input")?;
            let output = arg(args, 2, "output")?;
            let session = open(&input)?;
            files::save(&session.script, &output, FileFormat::from_path(&output), &session.layout).map_err(|e| e.to_string())?;
            println!("wrote {}", output.display());
        }
        "pdf" => {
            let input = arg(args, 1, "input")?;
            let output = args.get(2).map(PathBuf::from).unwrap_or_else(|| input.with_extension("pdf"));
            let session = open(&input)?;
            files::save(&session.script, &output, FileFormat::Pdf, &session.layout).map_err(|e| e.to_string())?;
            println!("wrote {}", output.display());
        }
        "paginate" => {
            let input = arg(args, 1, "input")?;
            let mut session = open(&input)?;
            let paginated = session.paginated().clone();
            if args.iter().any(|a| a == "--text") {
                print!("{}", render_text(&paginated));
            } else {
                println!("{} pages", paginated.page_count());
                for scene in &paginated.scenes {
                    println!("{:>4}  p.{:<4} {}", scene.number.as_deref().unwrap_or(""), scene.page, scene.heading);
                }
            }
        }
        "info" => {
            let input = arg(args, 1, "input")?;
            let mut session = open(&input)?;
            let pages = session.paginated().page_count();
            let script = &session.script;
            let mut characters: Vec<(String, usize)> = Vec::new();
            for element in script.elements.iter().filter(|e| e.kind == ElementKind::Character) {
                let name = element.character_name().to_uppercase();
                match characters.iter_mut().find(|(n, _)| *n == name) {
                    Some((_, count)) => *count += 1,
                    None => characters.push((name, 1)),
                }
            }
            characters.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
            println!("{}", script.title_page.get("Title").map(storyteller_core::inline::plain).unwrap_or_else(|| input.display().to_string()));
            println!("{pages} pages, {} scenes, {} elements", script.scene_indices().len(), script.elements.len());
            for (name, count) in characters {
                println!("  {name:<30} {count} {}", if count == 1 { "speech" } else { "speeches" });
            }
        }
        "run" => {
            let input = arg(args, 1, "input")?;
            let id = args.get(2).ok_or_else(|| format!("missing command\n\n{USAGE}"))?;
            let mut params = Map::new();
            for pair in args.iter().skip(3) {
                let (key, value) = pair.split_once('=').ok_or_else(|| format!("parameters are key=value, got `{pair}`"))?;
                let value = serde_json::from_str(value).unwrap_or_else(|_| Value::String(value.to_string()));
                params.insert(key.to_string(), value);
            }
            let registry = Registry::new();
            let mut session = open(&input)?;
            let outcome = registry.run(id, &mut session, &params).map_err(|e| e.to_string())?;
            if session.dirty {
                registry.run("file.save", &mut session, &Map::new()).map_err(|e| e.to_string())?;
            }
            println!("{}", serde_json::to_string_pretty(&outcome).unwrap_or_default());
        }
        "commands" => {
            let registry = Registry::new();
            let session = Session::default();
            for command in registry.list(&session) {
                let params: Vec<String> =
                    command.params.iter().map(|p| if p.required { p.name.to_string() } else { format!("[{}]", p.name) }).collect();
                println!("{:<26} {:<12} {}  {}", command.id, command.shortcut.unwrap_or(""), command.label, params.join(" "));
            }
        }
        "serve" => {
            let mut port: u16 = 9876;
            let mut file = None;
            let mut rest = args.iter().skip(1);
            while let Some(a) = rest.next() {
                if a == "--control" {
                    port = rest.next().and_then(|p| p.parse().ok()).ok_or("--control needs a port number")?;
                } else {
                    file = Some(PathBuf::from(a));
                }
            }
            let mut session = match &file {
                Some(path) => open(path)?,
                None => Session::default(),
            };
            let registry = Registry::new();
            let requests = serve(port, || {}).map_err(|e| format!("cannot listen on 127.0.0.1:{port}: {e}"))?;
            eprintln!("{BIN_NAME}: headless control server on 127.0.0.1:{port}");
            while let Ok(request) = requests.recv() {
                let reply = handle_line(&request.line, &mut session, &registry, &mut Headless);
                let _ = request.reply.send(reply);
            }
        }
        "-h" | "--help" | "help" => print!("{USAGE}"),
        "-V" | "--version" => println!("{}", storyteller_engine::version_line()),
        other => return Err(format!("unknown command `{other}`\n\n{USAGE}")),
    }
    Ok(())
}
