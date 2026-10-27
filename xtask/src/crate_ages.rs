//! scripts/check-crate-ages.py's exceptions, run (SUITE-STANDARDS.md §6): `--release` honours
//! the security lines, and an owner-approved override only for the release its line names,
//! given as `--tag`. `--print-exceptions` answers without the network, so this runs the script
//! itself against scratch exception files. Skipped, saying so, where no Python 3.11 or later is
//! on PATH.
//!
//! A unit test, not one in `tests/`, for the reason release_workflow.rs gives.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// A Python with tomllib (3.11 or later), or None.
fn python() -> Option<&'static str> {
    ["python3", "python"].into_iter().find(|p| Command::new(p).args(["-c", "import tomllib"]).output().is_ok_and(|o| o.status.success()))
}

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..")
}

fn run(py: &str, exceptions: &Path, release: bool, tag: Option<&str>) -> Output {
    let mut c = Command::new(py);
    c.arg(repo().join("scripts/check-crate-ages.py")).arg("--exceptions").arg(exceptions).arg("--print-exceptions");
    if release {
        c.arg("--release");
    }
    if let Some(tag) = tag {
        c.args(["--tag", tag]);
    }
    c.output().expect("python runs")
}

fn honoured(out: &Output) -> Vec<String> {
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8_lossy(&out.stdout).lines().map(str::to_string).collect()
}

fn scratch(name: &str, text: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("storyteller-crate-ages-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("scratch folder");
    let path = dir.join(name);
    std::fs::write(&path, text).expect("scratch file");
    path
}

const LINES: &str = "# header\nrustls 0.23.46   # security fix RUSTSEC-2026-0346\nparley 0.7.0   # age-out, 14 days old on 2026-10-23\nfoo 1.2.3   # owner-approved override, the owner, 2026-10-27, release v0.6.0 (\"Yes\")\n";

#[test]
fn an_owner_approved_override_holds_for_its_named_release_only() {
    let Some(py) = python() else {
        eprintln!("skipped: no Python 3.11 or later on PATH");
        return;
    };
    let f = scratch("named.txt", LINES);
    // Main honours every line.
    assert_eq!(honoured(&run(py, &f, false, None)), ["foo 1.2.3", "parley 0.7.0", "rustls 0.23.46"]);
    // A release honours the override only when it is the release named.
    assert_eq!(honoured(&run(py, &f, true, Some("v0.6.0"))), ["foo 1.2.3", "rustls 0.23.46"]);
    for other in [Some("v0.6.1"), Some("v0.5.0"), Some("v0.6.0-rc.1"), Some(""), None] {
        assert_eq!(honoured(&run(py, &f, true, other)), ["rustls 0.23.46"], "{other:?}");
    }
}

#[test]
fn the_release_named_ends_where_the_version_does() {
    let Some(py) = python() else {
        eprintln!("skipped: no Python 3.11 or later on PATH");
        return;
    };
    // A pre-release is its own release, not v0.6.0.
    let f = scratch("rc.txt", "foo 1.2.3   # owner-approved override, the owner, 2026-10-27, release v0.6.0-rc1\n");
    assert!(honoured(&run(py, &f, true, Some("v0.6.0"))).is_empty());
    assert_eq!(honoured(&run(py, &f, true, Some("v0.6.0-rc1"))), ["foo 1.2.3"]);
    // A fourth number names no release at all.
    let f = scratch("four.txt", "foo 1.2.3   # owner-approved override, the owner, 2026-10-27, release v0.6.0.1\n");
    assert!(!run(py, &f, true, Some("v0.6.0")).status.success());
}

#[test]
fn an_override_that_names_no_release_is_refused() {
    let Some(py) = python() else {
        eprintln!("skipped: no Python 3.11 or later on PATH");
        return;
    };
    let f = scratch("unnamed.txt", "foo 1.2.3   # owner-approved override, the owner, 2026-10-27\n");
    for (release, tag) in [(false, None), (true, None), (true, Some("v0.6.0"))] {
        let out = run(py, &f, release, tag);
        assert!(!out.status.success(), "{release} {tag:?} passed");
        assert!(String::from_utf8_lossy(&out.stderr).contains("names no \"release vX.Y.Z\""));
    }
}

#[test]
fn the_committed_exceptions_parse() {
    let Some(py) = python() else {
        eprintln!("skipped: no Python 3.11 or later on PATH");
        return;
    };
    let f = repo().join("scripts/crate-age-exceptions.txt");
    honoured(&run(py, &f, false, None));
    honoured(&run(py, &f, true, None));
}

#[test]
fn the_release_workflow_names_its_tag_to_the_crate_age_check() {
    let workflow = std::fs::read_to_string(repo().join(".github/workflows/release.yml")).expect("release.yml");
    assert!(workflow.contains("VERSION: ${{ steps.tag.outputs.version }}"), "the check is given the version the tag step settled");
    assert!(
        workflow.contains("check-crate-ages.py --release --tag \"v$VERSION\""),
        "release.yml's --release run must pass the tag, or no override can hold"
    );
}
