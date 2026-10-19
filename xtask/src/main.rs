//! Repository tasks: `cargo xtask layers`, `cargo xtask assets`, `cargo xtask privacy`,
//! `cargo xtask ci`, `cargo xtask corpus` (local only, see `corpus.rs`), `cargo xtask screens`
//! (retakes the README pictures, see `screens.rs`), and the packaging tasks `version`,
//! `bundle-macos`, `dmg-macos`, `package-linux`, `package-windows` (see `package.rs` and
//! docs/RELEASING.md), and `updater-archive` and `update-manifest`, the updater's files for a
//! release (see `updater.rs`).

mod corpus;
mod package;
mod screens;
mod updater;

use std::path::Path;
use std::process::{Command, ExitCode};

/// Crates and the workspace crates each may depend on (docs/architecture.md §1).
const LAYERS: [(&str, &[&str]); 5] = [
    ("storyteller-core", &[]),
    ("storyteller-pdf", &["storyteller-core"]),
    ("storyteller-engine", &["storyteller-core", "storyteller-pdf"]),
    ("storyteller-cli", &["storyteller-core", "storyteller-engine"]),
    ("method-storyteller", &["storyteller-core", "storyteller-engine", "storyteller-pdf"]),
];
/// Dependencies only the desktop app may use.
const UI_ONLY: [&str; 5] = ["egui", "eframe", "winit", "rfd", "craft-quit"];

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (task, rest) = args.split_first().map(|(t, r)| (t.as_str(), r)).unwrap_or(("", &[]));
    let result =
        match task {
            "layers" => layers(),
            "assets" => assets(),
            "privacy" => privacy(),
            "ci" => ci(),
            "corpus" => corpus::run(root()),
            "screens" => screens::run(root()),
        "updater-archive" => updater::archive_command(root(), rest),
        "update-manifest" => updater::manifest_command(root(), rest),
            "version" => package::workspace_version().map(|v| println!("{v}")),
            "bundle-macos" => package::parse_options(rest).and_then(|o| package::bundle_macos(&o)),
            "dmg-macos" => package::parse_options(rest).and_then(|o| package::dmg_macos(&o)),
            "package-linux" => package::parse_options(rest).and_then(|o| package::package_linux(&o)),
            "package-windows" => package::parse_options(rest).and_then(|o| package::package_windows(&o)),
            _ => Err("usage: cargo xtask <layers|assets|privacy|ci|corpus|screens|updater-archive|update-manifest|version|bundle-macos|dmg-macos|package-linux|package-windows>"
                .to_string()),
        };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("xtask: {message}");
            ExitCode::FAILURE
        }
    }
}

fn root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap_or(Path::new("."))
}

/// Dependency names from a manifest's `[dependencies]` table (a small TOML subset).
fn dependencies(manifest: &str) -> Vec<String> {
    let mut in_deps = false;
    let mut names = Vec::new();
    for line in manifest.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_deps = line == "[dependencies]";
            continue;
        }
        if in_deps && let Some((name, _)) = line.split_once('=') {
            names.push(name.trim().to_string());
        }
    }
    names
}

fn layers() -> Result<(), String> {
    let mut problems = Vec::new();
    for entry in std::fs::read_dir(root().join("crates")).map_err(|e| e.to_string())?.flatten() {
        let manifest = std::fs::read_to_string(entry.path().join("Cargo.toml")).map_err(|e| e.to_string())?;
        let name = manifest
            .lines()
            .find_map(|l| l.trim().strip_prefix("name = ").map(|n| n.trim_matches('"').to_string()))
            .ok_or("a crate manifest has no name")?;
        let Some((_, allowed)) = LAYERS.iter().find(|(crate_name, _)| *crate_name == name) else {
            problems.push(format!("{name} is not listed in xtask LAYERS"));
            continue;
        };
        for dep in dependencies(&manifest) {
            if dep.starts_with("storyteller-") && !allowed.contains(&dep.as_str()) {
                problems.push(format!("{name} must not depend on {dep}"));
            }
            if UI_ONLY.contains(&dep.as_str()) && name != "method-storyteller" {
                problems.push(format!("{name} must not depend on the UI crate {dep}"));
            }
        }
    }
    if problems.is_empty() {
        println!("layers: ok");
        Ok(())
    } else {
        Err(problems.join("\n"))
    }
}

/// Every bundled asset has a `.attribution` sidecar and an ATTRIBUTION.md entry.
fn assets() -> Result<(), String> {
    let attribution = std::fs::read_to_string(root().join("ATTRIBUTION.md")).map_err(|e| format!("ATTRIBUTION.md: {e}"))?;
    let mut problems = Vec::new();
    let mut stack = vec![root().join("assets")];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).map_err(|e| e.to_string())?.flatten() {
            let path = entry.path();
            if path.is_dir() {
                // Pictures of the app, taken by `cargo xtask screens` from our own sample.
                if path.ends_with("assets/screens") {
                    continue;
                }
                stack.push(path);
                continue;
            }
            let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            if name.ends_with(".attribution") || name == "OFL.txt" || name.starts_with('.') {
                continue;
            }
            if !path.with_file_name(format!("{name}.attribution")).exists() {
                problems.push(format!("{name} has no .attribution sidecar"));
            }
            if !attribution.contains(&name) {
                problems.push(format!("{name} is not listed in ATTRIBUTION.md"));
            }
        }
    }
    if problems.is_empty() {
        println!("assets: ok");
        Ok(())
    } else {
        Err(problems.join("\n"))
    }
}

/// Script and PDF files may only be tracked in test fixture folders: real scripts used for
/// measuring (`cargo xtask corpus`) must never be committed.
/// Script files the repository may track outside test fixtures: the invented Field Station
/// sample, written for the Method suite (examples/README.md).
const INVENTED_SAMPLES: [&str; 2] = ["examples/field-station.fdx", "examples/field-station.pdf"];

fn privacy() -> Result<(), String> {
    let output = Command::new("git").args(["ls-files", "-z"]).current_dir(root()).output().map_err(|e| e.to_string())?;
    if !output.status.success() {
        return Err("git ls-files failed".into());
    }
    let listed = String::from_utf8_lossy(&output.stdout);
    let problems: Vec<&str> = listed
        .split('\0')
        .filter(|path| {
            let lower = path.to_ascii_lowercase();
            [".fdx", ".fdr", ".pdf", ".celtx"].iter().any(|ext| lower.ends_with(ext))
        })
        .filter(|path| !(path.starts_with("crates/") && (path.contains("/tests/fixtures/") || path.contains("/tests/golden/"))))
        // The suite's invented sample, Field Station, in every format the other apps take it in.
        .filter(|path| !INVENTED_SAMPLES.contains(path))
        .collect();
    if problems.is_empty() {
        println!("privacy: ok");
        Ok(())
    } else {
        Err(format!("script or PDF files tracked outside test fixtures:\n{}", problems.join("\n")))
    }
}

fn cargo(args: &[&str]) -> Result<(), String> {
    println!("$ cargo {}", args.join(" "));
    let status =
        Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".into())).args(args).current_dir(root()).status().map_err(|e| e.to_string())?;
    if status.success() { Ok(()) } else { Err(format!("cargo {} failed", args.join(" "))) }
}

fn ci() -> Result<(), String> {
    cargo(&["fmt", "--all", "--check"])?;
    cargo(&["clippy", "--workspace", "--all-targets", "--", "-D", "warnings"])?;
    cargo(&["test", "--workspace"])?;
    layers()?;
    assets()?;
    privacy()?;
    println!("ci: all gates passed");
    Ok(())
}
