//! The release workflow's own logic, here rather than as shell inside the YAML, so it runs and
//! is tested on any machine (the tech-debt audit, 2026-11-17):
//!
//! - `cargo xtask release-check`: what the run builds. A rehearsal (run by hand on main with no
//!   tag) builds main's latest commit; a tag must be `vX.Y.Z` or `vX.Y.Z-pre`, match the version
//!   in Cargo.toml and point at a commit on main. Either way the version's notes must be
//!   written. Prints `tag=`, `ref=`, `version=` and `rehearsal=` lines for `$GITHUB_OUTPUT`.
//! - `cargo xtask release-files`: every installer is in the folder under its version-free name;
//!   the update manifest is written when the updater archive came; `SHA256SUMS` lists every
//!   file, in `sha256sum`'s format; and the manifest offers the one universal archive to both
//!   kinds of Mac.
//!
//! Neither meets a secret: the check and assemble jobs have none.

use std::path::Path;
use std::process::Command;

use sha2::{Digest, Sha256};

use crate::updater;

/// The installers every release offers, under the names the website links to.
pub const INSTALLERS: [&str; 4] = [
    "Method-Storyteller-macOS.dmg",
    "Method-Storyteller-windows-x64-setup.exe",
    "Method-Storyteller-linux-amd64.deb",
    "Method-Storyteller-linux-x86_64.AppImage",
];

const REPO: &str = "method-production-suite/method-storyteller";

/// What a run builds, as the later jobs read it.
#[derive(Debug, PartialEq, Eq)]
struct Plan {
    tag: String,
    git_ref: String,
    version: String,
    rehearsal: bool,
}

impl Plan {
    fn outputs(&self) -> String {
        format!("tag={}\nref={}\nversion={}\nrehearsal={}\n", self.tag, self.git_ref, self.version, self.rehearsal)
    }
}

/// `vX.Y.Z` or `vX.Y.Z-pre`, where the pre-release part is letters, digits and dots.
fn is_version_tag(tag: &str) -> bool {
    let Some(rest) = tag.strip_prefix('v') else { return false };
    let (core, pre) = match rest.split_once('-') {
        Some((core, pre)) => (core, Some(pre)),
        None => (rest, None),
    };
    let numbers: Vec<&str> = core.split('.').collect();
    numbers.len() == 3
        && numbers.iter().all(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
        && pre.is_none_or(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'.'))
}

/// What is wrong with a version's notes, if anything: missing, empty, or still holding the
/// skeleton's `{{placeholders}}`.
fn notes_problem(path: &str, text: Option<&str>) -> Option<String> {
    match text {
        None => Some(format!("{path} is missing: write it from docs/release-notes-template.md first.")),
        Some(t) if t.trim().is_empty() => Some(format!("{path} is empty: write it from docs/release-notes-template.md first.")),
        Some(t) if t.contains("{{") => Some(format!("{path} still has {{{{placeholders}}}} from the skeleton.")),
        Some(_) => None,
    }
}

/// The version in `root`'s Cargo.toml, as it is checked out now.
fn version_at(root: &Path) -> Result<String, String> {
    let manifest = std::fs::read_to_string(root.join("Cargo.toml")).map_err(|e| format!("Cargo.toml: {e}"))?;
    crate::package::version_from_manifest(&manifest).ok_or_else(|| "no version under [workspace.package] in Cargo.toml".into())
}

fn check_notes(root: &Path, version: &str) -> Result<(), String> {
    let path = format!("docs/releases/{version}.md");
    let text = std::fs::read_to_string(root.join(&path)).ok();
    notes_problem(&path, text.as_deref()).map_or(Ok(()), Err)
}

fn git(root: &Path, args: &[&str]) -> Result<String, String> {
    let output = Command::new("git").args(args).current_dir(root).output().map_err(|e| format!("git: {e}"))?;
    if !output.status.success() {
        return Err(format!("git {}: {}", args.join(" "), String::from_utf8_lossy(&output.stderr).trim()));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn arg<'a>(values: &'a [(String, String)], name: &str) -> &'a str {
    values.iter().find(|(n, _)| n == name).map_or("", |(_, v)| v.as_str())
}

fn flags(rest: &[String]) -> Result<Vec<(String, String)>, String> {
    let mut values = Vec::new();
    let mut it = rest.iter();
    while let Some(flag) = it.next() {
        let name = flag.strip_prefix("--").ok_or_else(|| format!("unexpected argument {flag}"))?;
        // An empty value is allowed: a tag push has no input tag.
        let value = it.next().ok_or_else(|| format!("--{name} needs a value"))?;
        values.push((name.to_string(), value.clone()));
    }
    Ok(values)
}

/// `cargo xtask release-check --event E --input-tag T --ref R --ref-name N --sha S`.
pub fn check_command(root: &Path, rest: &[String]) -> Result<(), String> {
    let values = flags(rest)?;
    let plan = plan(root, arg(&values, "event"), arg(&values, "input-tag"), arg(&values, "ref"), arg(&values, "ref-name"), arg(&values, "sha"))?;
    if plan.rehearsal {
        eprintln!("Rehearsing v{} on main ({}).", plan.version, plan.git_ref);
    }
    print!("{}", plan.outputs());
    Ok(())
}

fn plan(root: &Path, event: &str, input_tag: &str, git_ref: &str, ref_name: &str, sha: &str) -> Result<Plan, String> {
    // A rehearsal: run by hand on main with no tag. It builds, signs and checks main's latest
    // commit exactly as a release would, with its notes, and drafts nothing.
    if event == "workflow_dispatch" && input_tag.is_empty() {
        if git_ref != "refs/heads/main" {
            return Err("a rehearsal runs on main.".into());
        }
        let version = version_at(root)?;
        check_notes(root, &version)?;
        return Ok(Plan { tag: String::new(), git_ref: sha.to_string(), version, rehearsal: true });
    }
    let tag = if input_tag.is_empty() { ref_name } else { input_tag };
    if !is_version_tag(tag) {
        return Err(format!("{tag} is not a version tag (vX.Y.Z or vX.Y.Z-pre)."));
    }
    git(root, &["checkout", "--quiet", &format!("refs/tags/{tag}")])?;
    let version = version_at(root)?;
    if tag != format!("v{version}") {
        return Err(format!("the tag {tag} is not v{version}, the version in Cargo.toml."));
    }
    check_notes(root, &version)?;
    // A tag on a side branch must not build a release.
    let commit = git(root, &["rev-list", "-n", "1", tag])?;
    if git(root, &["merge-base", "--is-ancestor", &commit, "origin/main"]).is_err() {
        return Err(format!("{tag} points at a commit that is not on main."));
    }
    Ok(Plan { tag: tag.to_string(), git_ref: tag.to_string(), version, rehearsal: false })
}

/// `SHA256SUMS` for every file in `dir` but itself, in `sha256sum`'s format and name order.
fn checksums(dir: &Path) -> Result<String, String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .map_err(|e| format!("{}: {e}", dir.display()))?
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|name| name != "SHA256SUMS")
        .collect();
    names.sort();
    let mut out = String::new();
    for name in names {
        let bytes = std::fs::read(dir.join(&name)).map_err(|e| format!("{name}: {e}"))?;
        let digest = Sha256::digest(&bytes);
        let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
        out.push_str(&format!("{hex}  {name}\n"));
    }
    Ok(out)
}

/// The manifest offers the one universal archive, and only it, to both kinds of Mac.
fn manifest_problem(manifest: &str, version: &str) -> Option<String> {
    let value: serde_json::Value = match serde_json::from_str(manifest) {
        Ok(value) => value,
        Err(error) => return Some(format!("latest.json does not parse: {error}")),
    };
    let Some(platforms) = value.get("platforms").and_then(serde_json::Value::as_object) else {
        return Some("latest.json has no platforms".into());
    };
    let mut keys: Vec<&str> = platforms.keys().map(String::as_str).collect();
    keys.sort_unstable();
    if keys != ["darwin-aarch64", "darwin-x86_64"] {
        return Some(format!("latest.json offers {keys:?}, not darwin-aarch64 and darwin-x86_64"));
    }
    let want = format!("https://github.com/{REPO}/releases/download/v{version}/{}", updater::archive_name(version));
    let wrong: Vec<&str> = platforms.values().filter_map(|p| p.get("url").and_then(serde_json::Value::as_str)).filter(|u| *u != want).collect();
    let missing = platforms.values().any(|p| p.get("url").and_then(serde_json::Value::as_str).is_none());
    if missing || !wrong.is_empty() {
        return Some(format!("latest.json points at {wrong:?}, not {want}"));
    }
    None
}

/// `cargo xtask release-files --dir D --version V`.
pub fn files_command(root: &Path, rest: &[String]) -> Result<(), String> {
    let values = flags(rest)?;
    let (dir, version) = (Path::new(arg(&values, "dir")), arg(&values, "version"));
    if version.is_empty() || arg(&values, "dir").is_empty() {
        return Err("usage: cargo xtask release-files --dir <folder> --version <version>".into());
    }
    for name in INSTALLERS {
        if std::fs::metadata(dir.join(name)).map_or(true, |m| m.len() == 0) {
            return Err(format!("{} is missing.", dir.join(name).display()));
        }
    }
    // The update manifest, when the Mac build brought the signed updater archive. It is checked
    // against the app's own public key before it is written.
    let archive = dir.join(updater::archive_name(version));
    let signature = dir.join(format!("{}.sig", updater::archive_name(version)));
    let manifest = dir.join("latest.json");
    if [&archive, &signature].iter().all(|p| std::fs::metadata(p).is_ok_and(|m| m.len() > 0)) {
        let args = ["--dir", &dir.display().to_string(), "--version", version, "--out", &manifest.display().to_string()].map(str::to_string);
        updater::manifest_command(root, &args)?;
    } else {
        println!(
            "::notice::No updater archive in this release (no update key, or not notarized): no latest.json, so installed apps won't see it (docs/RELEASING.md)."
        );
    }
    // One SHA256SUMS, under exactly that name, listing every file the release offers, latest.json
    // and the updater archive included (SUITE-STANDARDS.md, releases). Written last, so it covers
    // everything else.
    let sums = checksums(dir)?;
    std::fs::write(dir.join("SHA256SUMS"), &sums).map_err(|e| format!("SHA256SUMS: {e}"))?;
    print!("{sums}");
    if checksums(dir)? != sums {
        return Err("SHA256SUMS does not match the files read back.".into());
    }
    if let Ok(text) = std::fs::read_to_string(&manifest)
        && let Some(problem) = manifest_problem(&text, version)
    {
        return Err(problem);
    }
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn version_tags() {
        for good in ["v0.7.0", "v1.20.3", "v0.8.0-beta.1", "v2.0.0-rc1"] {
            assert!(is_version_tag(good), "{good}");
        }
        for bad in ["0.7.0", "v0.7", "v0.7.0.1", "v0.7.x", "v0.7.0-", "v0.7.0-beta_1", "v", "", "v.7.0", "release-0.7.0"] {
            assert!(!is_version_tag(bad), "{bad}");
        }
    }

    #[test]
    fn notes_must_be_written() {
        let path = "docs/releases/0.7.0.md";
        assert!(notes_problem(path, None).unwrap().contains("is missing"));
        assert!(notes_problem(path, Some("  \n")).unwrap().contains("is empty"));
        assert!(notes_problem(path, Some("# Method Storyteller {{version}}")).unwrap().contains("{{placeholders}}"));
        assert_eq!(notes_problem(path, Some("# Method Storyteller 0.7.0\n")), None);
    }

    #[test]
    fn outputs_read_as_github_outputs() {
        let plan = Plan { tag: "v0.7.0".into(), git_ref: "v0.7.0".into(), version: "0.7.0".into(), rehearsal: false };
        assert_eq!(plan.outputs(), "tag=v0.7.0\nref=v0.7.0\nversion=0.7.0\nrehearsal=false\n");
    }

    /// A repository with a commit on main for each version, tagged, and origin/main at the last
    /// commit on main; then a commit on a side branch, tagged v9.9.9.
    fn repository(name: &str) -> std::path::PathBuf {
        let dir = scratch(name);
        let git = |args: &[&str]| {
            let out = Command::new("git").args(args).current_dir(&dir).output().unwrap();
            assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
        };
        git(&["init", "--quiet", "--initial-branch", "main"]);
        let commit = |version: &str, notes: Option<&str>| {
            std::fs::write(dir.join("Cargo.toml"), format!("[workspace]\n\n[workspace.package]\nversion = \"{version}\"\n")).unwrap();
            std::fs::create_dir_all(dir.join("docs/releases")).unwrap();
            if let Some(notes) = notes {
                std::fs::write(dir.join(format!("docs/releases/{version}.md")), notes).unwrap();
            }
            git(&["add", "-A"]);
            git(&["-c", "user.name=Test", "-c", "user.email=test@example.com", "commit", "--quiet", "-m", version]);
        };
        commit("0.6.0", Some("# Method Storyteller 0.6.0\n"));
        git(&["tag", "v0.6.0"]);
        git(&["tag", "v0.6.5"]);
        commit("0.7.0", None);
        git(&["tag", "v0.7.0"]);
        git(&["update-ref", "refs/remotes/origin/main", "HEAD"]);
        git(&["switch", "--quiet", "-c", "side"]);
        commit("9.9.9", Some("# Method Storyteller 9.9.9\n"));
        git(&["tag", "v9.9.9"]);
        git(&["switch", "--quiet", "main"]);
        dir
    }

    #[test]
    fn a_tag_builds_only_its_own_version_from_main_with_notes() {
        let dir = repository("tags");
        let push = |tag: &str| plan(&dir, "push", "", &format!("refs/tags/{tag}"), tag, "abc");
        assert_eq!(push("v0.6.0").unwrap(), Plan { tag: "v0.6.0".into(), git_ref: "v0.6.0".into(), version: "0.6.0".into(), rehearsal: false });
        assert!(push("v0.6.5").unwrap_err().contains("is not v0.6.0"));
        assert!(push("v0.7.0").unwrap_err().contains("0.7.0.md is missing"));
        assert!(push("v9.9.9").unwrap_err().contains("not on main"));
        // Run by hand with a tag: the same checks on that tag.
        assert!(plan(&dir, "workflow_dispatch", "v0.6.0", "refs/heads/main", "main", "abc").unwrap().tag == "v0.6.0");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_rehearsal_runs_on_main_with_notes() {
        let dir = repository("rehearsal");
        assert!(plan(&dir, "workflow_dispatch", "", "refs/heads/side", "side", "abc").unwrap_err().contains("runs on main"));
        // main's latest commit is 0.7.0, which has no notes yet.
        assert!(plan(&dir, "workflow_dispatch", "", "refs/heads/main", "main", "abc").unwrap_err().contains("0.7.0.md is missing"));
        std::fs::write(dir.join("docs/releases/0.7.0.md"), "# Method Storyteller 0.7.0\n").unwrap();
        assert_eq!(
            plan(&dir, "workflow_dispatch", "", "refs/heads/main", "main", "abc").unwrap(),
            Plan { tag: String::new(), git_ref: "abc".into(), version: "0.7.0".into(), rehearsal: true }
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    fn scratch(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("storyteller-xtask-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn checksums_are_written_as_sha256sum_writes_them() {
        let dir = scratch("sums");
        std::fs::write(dir.join("b.txt"), "abc").unwrap();
        std::fs::write(dir.join("a.txt"), "").unwrap();
        std::fs::write(dir.join("SHA256SUMS"), "stale").unwrap();
        std::fs::create_dir(dir.join("sub")).unwrap();
        assert_eq!(
            checksums(&dir).unwrap(),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855  a.txt\n\
             ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad  b.txt\n"
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_release_needs_every_installer() {
        let dir = scratch("installers");
        for name in INSTALLERS.iter().skip(1) {
            std::fs::write(dir.join(name), "x").unwrap();
        }
        let args = ["--dir", &dir.display().to_string(), "--version", "0.7.0"].map(str::to_string);
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        assert!(files_command(root, &args).unwrap_err().contains("Method-Storyteller-macOS.dmg is missing"));
        std::fs::write(dir.join(INSTALLERS[0]), "x").unwrap();
        files_command(root, &args).unwrap();
        let sums = std::fs::read_to_string(dir.join("SHA256SUMS")).unwrap();
        assert_eq!(sums.lines().count(), 4, "{sums}");
        assert!(!dir.join("latest.json").exists(), "no updater archive, no manifest");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn the_manifest_offers_the_universal_archive_to_both_macs() {
        let url = format!("https://github.com/{REPO}/releases/download/v0.7.0/Method-Storyteller_0.7.0_universal.app.tar.gz");
        let good = format!(r#"{{"platforms": {{"darwin-aarch64": {{"url": "{url}"}}, "darwin-x86_64": {{"url": "{url}"}}}}}}"#);
        assert_eq!(manifest_problem(&good, "0.7.0"), None);
        assert!(manifest_problem(&good, "0.7.1").unwrap().contains("points at"));
        let one = format!(r#"{{"platforms": {{"darwin-aarch64": {{"url": "{url}"}}}}}}"#);
        assert!(manifest_problem(&one, "0.7.0").unwrap().contains("offers"));
        assert!(manifest_problem("{", "0.7.0").unwrap().contains("does not parse"));
    }
}
