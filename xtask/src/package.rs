//! Packaging: `cargo xtask version`, `bundle-macos`, `dmg-macos`, `package-linux` and
//! `package-windows`. The release workflow (`.github/workflows/release.yml`) runs these; they
//! also run on a developer's machine. Everything lands in `target/bundle/<platform>/` under the
//! suite's version-free names. Releasing: docs/RELEASING.md.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::root;

const APP_NAME: &str = "Method Storyteller";
const APP_BIN: &str = "method-storyteller-app";
const APP_PACKAGE: &str = "method-storyteller";
/// The Linux package, desktop file and icon name.
const LINUX_NAME: &str = "method-storyteller";

pub const DMG: &str = "Method-Storyteller-macOS.dmg";
pub const DEB: &str = "Method-Storyteller-linux-amd64.deb";
pub const APPIMAGE: &str = "Method-Storyteller-linux-x86_64.AppImage";
pub const SETUP_EXE: &str = "Method-Storyteller-windows-x64-setup.exe";

/// Options shared by the packaging tasks: `--target <triple>`, `--sign <identity>`,
/// `--no-build`, `--appimagetool <path>`, `--runtime <path>`, `--iscc <path>`.
#[derive(Debug, Default, PartialEq)]
pub struct Options {
    target: Option<String>,
    sign: Option<String>,
    no_build: bool,
    appimagetool: Option<PathBuf>,
    runtime: Option<PathBuf>,
    iscc: Option<PathBuf>,
}

pub fn parse_options(args: &[String]) -> Result<Options, String> {
    let mut options = Options::default();
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        let mut value = || args.next().cloned().ok_or_else(|| format!("{arg} needs a value"));
        match arg.as_str() {
            "--target" => options.target = Some(value()?),
            "--sign" => options.sign = Some(value()?),
            "--no-build" => options.no_build = true,
            "--appimagetool" => options.appimagetool = Some(PathBuf::from(value()?)),
            "--runtime" => options.runtime = Some(PathBuf::from(value()?)),
            "--iscc" => options.iscc = Some(PathBuf::from(value()?)),
            other => return Err(format!("unknown option {other}")),
        }
    }
    Ok(options)
}

/// The `version` under `[workspace.package]` in the root Cargo.toml.
pub fn workspace_version() -> Result<String, String> {
    let manifest = std::fs::read_to_string(root().join("Cargo.toml")).map_err(|e| format!("Cargo.toml: {e}"))?;
    version_from_manifest(&manifest).ok_or_else(|| "no version under [workspace.package] in Cargo.toml".into())
}

fn version_from_manifest(manifest: &str) -> Option<String> {
    let mut in_package = false;
    for line in manifest.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_package = line == "[workspace.package]";
            continue;
        }
        if in_package && let Some((key, value)) = line.split_once('=') {
            let value = value.trim().trim_matches('"');
            if key.trim() == "version" && !value.is_empty() {
                return Some(value.to_string());
            }
        }
    }
    None
}

/// `0.2.0-beta.1` sorts before `0.2.0` in Debian only with a tilde: `0.2.0~beta.1`.
fn debian_version(version: &str) -> String {
    version.replacen('-', "~", 1)
}

/// The `X.Y.Z` part, for places that take numbers only (the Windows file version).
fn numeric_version(version: &str) -> &str {
    version.split(['-', '+']).next().unwrap_or(version)
}

fn target_dir() -> PathBuf {
    std::env::var_os("CARGO_TARGET_DIR").map(PathBuf::from).unwrap_or_else(|| root().join("target"))
}

fn bundle_dir(platform: &str) -> PathBuf {
    target_dir().join("bundle").join(platform)
}

/// The universal Mac target: both slices built, then joined with `lipo -create` into
/// `target/universal-apple-darwin/release/` (SUITE-STANDARDS.md §3).
pub const UNIVERSAL: &str = "universal-apple-darwin";
/// The two slices of [`UNIVERSAL`].
pub const MAC_SLICES: [&str; 2] = ["aarch64-apple-darwin", "x86_64-apple-darwin"];

/// Builds the app in release mode (unless `--no-build`) and returns its binary.
fn build_app(options: &Options) -> Result<PathBuf, String> {
    if options.target.as_deref() == Some(UNIVERSAL) {
        if !options.no_build {
            let mut args = vec!["build", "--release", "--locked", "--package", APP_PACKAGE, "--bin", APP_BIN];
            for slice in MAC_SLICES {
                args.extend(["--target", slice]);
            }
            crate::cargo(&args)?;
        }
        let slices: Vec<PathBuf> = MAC_SLICES.iter().map(|t| target_dir().join(t).join("release").join(APP_BIN)).collect();
        let out_dir = target_dir().join(UNIVERSAL).join("release");
        let binary = out_dir.join(APP_BIN);
        // With --no-build, a binary joined earlier (the release workflow joins it before any
        // secret) is used as it is.
        if !options.no_build || !binary.is_file() {
            for slice in &slices {
                if !slice.is_file() {
                    return Err(format!("{} not found; build it first", slice.display()));
                }
            }
            std::fs::create_dir_all(&out_dir).map_err(|e| format!("{}: {e}", out_dir.display()))?;
            let mut args: Vec<&std::ffi::OsStr> = vec![os("-create"), os("-output"), binary.as_os_str()];
            args.extend(slices.iter().map(|s| s.as_os_str()));
            run("lipo", &args)?;
        }
        return Ok(binary);
    }
    if !options.no_build {
        let mut args = vec!["build", "--release", "--locked", "--package", APP_PACKAGE, "--bin", APP_BIN];
        if let Some(target) = &options.target {
            args.extend(["--target", target.as_str()]);
        }
        crate::cargo(&args)?;
    }
    let mut dir = target_dir();
    if let Some(target) = &options.target {
        dir.push(target);
    }
    let binary = dir.join("release").join(format!("{APP_BIN}{}", std::env::consts::EXE_SUFFIX));
    if binary.is_file() { Ok(binary) } else { Err(format!("{} not found; build it first", binary.display())) }
}

fn run(program: impl AsRef<std::ffi::OsStr>, args: &[&std::ffi::OsStr]) -> Result<(), String> {
    let program = program.as_ref();
    let shown: Vec<String> = args.iter().map(|a| a.to_string_lossy().into_owned()).collect();
    println!("$ {} {}", program.to_string_lossy(), shown.join(" "));
    let status = Command::new(program).args(args).current_dir(root()).status().map_err(|e| format!("{}: {e}", program.to_string_lossy()))?;
    if status.success() { Ok(()) } else { Err(format!("{} failed", program.to_string_lossy())) }
}

fn os(s: &str) -> &std::ffi::OsStr {
    std::ffi::OsStr::new(s)
}

fn fresh_dir(path: &Path) -> Result<(), String> {
    if path.exists() {
        std::fs::remove_dir_all(path).map_err(|e| format!("{}: {e}", path.display()))?;
    }
    std::fs::create_dir_all(path).map_err(|e| format!("{}: {e}", path.display()))
}

fn copy(from: &Path, to: &Path) -> Result<(), String> {
    if let Some(parent) = to.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
    }
    std::fs::copy(from, to).map(|_| ()).map_err(|e| format!("{} -> {}: {e}", from.display(), to.display()))
}

fn write(path: &Path, contents: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
    }
    std::fs::write(path, contents).map_err(|e| format!("{}: {e}", path.display()))
}

fn read(path: &Path) -> Result<String, String> {
    std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))
}

#[cfg(unix)]
fn set_mode(path: &Path, mode: u32) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).map_err(|e| format!("{}: {e}", path.display()))
}

#[cfg(not(unix))]
fn set_mode(_path: &Path, _mode: u32) -> Result<(), String> {
    Ok(())
}

fn packaging(relative: &str) -> PathBuf {
    root().join("packaging").join(relative)
}

/// The file types the macOS bundle declares, each with its document icon.
const DOCUMENT_KINDS: [&str; 3] = ["osf", "fountain", "fdx"];

/// `cargo xtask bundle-macos [--target <triple>] [--sign <identity>] [--no-build]`: builds
/// `target/bundle/macos/Method Storyteller.app` and signs it, with the Developer ID identity
/// (hardened runtime and a secure timestamp, ready to notarize) or ad hoc without one.
pub fn bundle_macos(options: &Options) -> Result<(), String> {
    if !cfg!(target_os = "macos") {
        return Err("bundle-macos runs on macOS only".into());
    }
    let version = workspace_version()?;
    let binary = build_app(options)?;
    let out = bundle_dir("macos");
    std::fs::create_dir_all(&out).map_err(|e| format!("{}: {e}", out.display()))?;
    let app = out.join(format!("{APP_NAME}.app"));
    fresh_dir(&app)?;
    let contents = app.join("Contents");
    copy(&binary, &contents.join("MacOS").join(APP_BIN))?;
    set_mode(&contents.join("MacOS").join(APP_BIN), 0o755)?;
    copy(&packaging("icons/icon.icns"), &contents.join("Resources").join("AppIcon.icns"))?;
    // The document icons the Info.plist names for .osf, .fountain and .fdx files.
    for kind in DOCUMENT_KINDS {
        let icon = format!("document-{kind}.icns");
        copy(&packaging(&format!("icons/{icon}")), &contents.join("Resources").join(&icon))?;
    }
    let plist = read(&packaging("macos/Info.plist"))?;
    if !plist.contains("{{VERSION}}") {
        return Err("packaging/macos/Info.plist has no {{VERSION}} placeholder".into());
    }
    let info = contents.join("Info.plist");
    write(&info, &plist.replace("{{VERSION}}", &version))?;
    write(&contents.join("PkgInfo"), "APPL????")?;
    run("plutil", &[os("-lint"), info.as_os_str()])?;
    match &options.sign {
        Some(identity) => {
            run("codesign", &[os("--force"), os("--options"), os("runtime"), os("--timestamp"), os("--sign"), os(identity), app.as_os_str()])?
        }
        None => run("codesign", &[os("--force"), os("--sign"), os("-"), app.as_os_str()])?,
    }
    run("codesign", &[os("--verify"), os("--deep"), os("--strict"), os("--verbose=2"), app.as_os_str()])?;
    println!("bundle-macos: {} ({version}, {})", app.display(), if options.sign.is_some() { "Developer ID" } else { "ad hoc" });
    Ok(())
}

/// `cargo xtask dmg-macos [--sign <identity>]`: puts the bundle made by `bundle-macos` (and a
/// link to /Applications) into `target/bundle/macos/Method-Storyteller-macOS.dmg`, signed
/// with the identity when one is given.
pub fn dmg_macos(options: &Options) -> Result<(), String> {
    if !cfg!(target_os = "macos") {
        return Err("dmg-macos runs on macOS only".into());
    }
    let out = bundle_dir("macos");
    let app = out.join(format!("{APP_NAME}.app"));
    if !app.is_dir() {
        return Err(format!("{} not found; run cargo xtask bundle-macos first", app.display()));
    }
    let staging = out.join("dmg");
    fresh_dir(&staging)?;
    // ditto keeps the signature, the stapled ticket and the extended attributes.
    run("ditto", &[app.as_os_str(), staging.join(format!("{APP_NAME}.app")).as_os_str()])?;
    run("ln", &[os("-s"), os("/Applications"), staging.join("Applications").as_os_str()])?;
    let dmg = out.join(DMG);
    run(
        "hdiutil",
        &[os("create"), os("-volname"), os(APP_NAME), os("-srcfolder"), staging.as_os_str(), os("-ov"), os("-format"), os("UDZO"), dmg.as_os_str()],
    )?;
    std::fs::remove_dir_all(&staging).map_err(|e| format!("{}: {e}", staging.display()))?;
    if let Some(identity) = &options.sign {
        run("codesign", &[os("--force"), os("--timestamp"), os("--sign"), os(identity), dmg.as_os_str()])?;
    }
    println!("dmg-macos: {}", dmg.display());
    Ok(())
}

/// The Debian package's control file. The libraries are the ones eframe loads at run time
/// (OpenGL, xkbcommon, X11 and Wayland); the binary itself links only libc and libgcc.
fn deb_control(version: &str, installed_kib: u64) -> String {
    format!(
        "Package: {LINUX_NAME}\n\
         Version: {}\n\
         Architecture: amd64\n\
         Maintainer: Method Production Suite <hello@methodproduction.com>\n\
         Installed-Size: {installed_kib}\n\
         Depends: libc6 (>= 2.35), libgcc-s1, libgl1, libegl1, libxkbcommon0, libxkbcommon-x11-0, libx11-6, libxcursor1, libxi6, libxrandr2, libwayland-client0, libwayland-cursor0, libwayland-egl1\n\
         Recommends: xdg-desktop-portal\n\
         Section: editors\n\
         Priority: optional\n\
         Homepage: https://methodstoryteller.com\n\
         Description: Screenwriting app that formats itself\n \
         Method Storyteller is an open-source screenwriting app. Tab and Enter move\n \
         between scene heading, action, character and dialogue, pages break the way a\n \
         production expects, and the script is a Fountain file you own.\n",
        debian_version(version)
    )
}

/// Lays out the files both Linux packages share under `dir` (`usr/bin`, the desktop file,
/// the icons and the licences).
fn linux_tree(dir: &Path, binary: &Path) -> Result<(), String> {
    let bin = dir.join("usr/bin").join(APP_BIN);
    copy(binary, &bin)?;
    set_mode(&bin, 0o755)?;
    copy(&packaging("linux/method-storyteller.desktop"), &dir.join("usr/share/applications").join(format!("{LINUX_NAME}.desktop")))?;
    for size in ["256", "512"] {
        copy(&packaging(&format!("icons/icon-{size}.png")), &dir.join(format!("usr/share/icons/hicolor/{size}x{size}/apps/{LINUX_NAME}.png")))?;
    }
    let mut copyright = read(&root().join("NOTICE"))?;
    for licence in ["LICENSE-MIT", "LICENSE-APACHE"] {
        copyright.push_str(&format!("\n\n---- {licence} ----\n\n"));
        copyright.push_str(&read(&root().join(licence))?);
    }
    write(&dir.join("usr/share/doc").join(LINUX_NAME).join("copyright"), &copyright)
}

fn tree_size(dir: &Path) -> Result<u64, String> {
    let mut total = 0u64;
    let mut stack = vec![dir.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).map_err(|e| format!("{}: {e}", dir.display()))?.flatten() {
            let meta = entry.metadata().map_err(|e| e.to_string())?;
            if meta.is_dir() {
                stack.push(entry.path());
            } else {
                total = total.saturating_add(meta.len());
            }
        }
    }
    Ok(total)
}

/// `cargo xtask package-linux [--no-build] [--appimagetool <path> --runtime <path>]`: the
/// `.deb` (with `dpkg-deb`) and, given appimagetool and its runtime file, the AppImage.
pub fn package_linux(options: &Options) -> Result<(), String> {
    if !cfg!(target_os = "linux") {
        return Err("package-linux runs on Linux only".into());
    }
    let version = workspace_version()?;
    let binary = build_app(options)?;
    let out = bundle_dir("linux");
    std::fs::create_dir_all(&out).map_err(|e| format!("{}: {e}", out.display()))?;

    let deb_root = out.join("deb");
    fresh_dir(&deb_root)?;
    linux_tree(&deb_root, &binary)?;
    let installed_kib = tree_size(&deb_root)?.div_ceil(1024);
    write(&deb_root.join("DEBIAN/control"), &deb_control(&version, installed_kib))?;
    let deb = out.join(DEB);
    run("dpkg-deb", &[os("--root-owner-group"), os("--build"), deb_root.as_os_str(), deb.as_os_str()])?;
    println!("package-linux: {}", deb.display());

    let (Some(tool), Some(runtime)) = (&options.appimagetool, &options.runtime) else {
        println!("package-linux: no AppImage (pass --appimagetool <path> --runtime <path>)");
        return Ok(());
    };
    let app_dir = out.join("AppDir");
    fresh_dir(&app_dir)?;
    linux_tree(&app_dir, &binary)?;
    let app_run = app_dir.join("AppRun");
    write(&app_run, &format!("#!/bin/sh\nHERE=\"$(dirname \"$(readlink -f \"$0\")\")\"\nexec \"$HERE/usr/bin/{APP_BIN}\" \"$@\"\n"))?;
    set_mode(&app_run, 0o755)?;
    copy(&packaging("linux/method-storyteller.desktop"), &app_dir.join(format!("{LINUX_NAME}.desktop")))?;
    copy(&packaging("icons/icon-256.png"), &app_dir.join(format!("{LINUX_NAME}.png")))?;
    copy(&packaging("icons/icon-256.png"), &app_dir.join(".DirIcon"))?;
    let appimage = out.join(APPIMAGE);
    println!("$ ARCH=x86_64 {} --no-appstream --runtime-file {} {} {}", tool.display(), runtime.display(), app_dir.display(), appimage.display());
    let status = Command::new(tool)
        .env("ARCH", "x86_64")
        // Runs without FUSE (build machines rarely have it).
        .env("APPIMAGE_EXTRACT_AND_RUN", "1")
        .args([os("--no-appstream"), os("--runtime-file"), runtime.as_os_str(), app_dir.as_os_str(), appimage.as_os_str()])
        .current_dir(root())
        .status()
        .map_err(|e| format!("{}: {e}", tool.display()))?;
    if !status.success() {
        return Err("appimagetool failed".into());
    }
    println!("package-linux: {}", appimage.display());
    Ok(())
}

/// `cargo xtask package-windows [--no-build] [--iscc <path>]`: the Inno Setup installer.
pub fn package_windows(options: &Options) -> Result<(), String> {
    if !cfg!(target_os = "windows") {
        return Err("package-windows runs on Windows only".into());
    }
    if options.target.is_some() {
        return Err("package-windows builds for the host only (the installer script reads target/release)".into());
    }
    let version = workspace_version()?;
    build_app(options)?;
    let out = bundle_dir("windows");
    std::fs::create_dir_all(&out).map_err(|e| format!("{}: {e}", out.display()))?;
    let iscc = options.iscc.clone().unwrap_or_else(|| PathBuf::from(r"C:\Program Files (x86)\Inno Setup 6\ISCC.exe"));
    let defines = [
        format!("/DAppVersion={version}"),
        format!("/DAppNumericVersion={}", numeric_version(&version)),
        format!("/DSourceDir={}", root().display()),
        format!("/DOutputDir={}", out.display()),
    ];
    let script = packaging("windows/method-storyteller.iss");
    let mut args: Vec<&std::ffi::OsStr> = defines.iter().map(|d| os(d)).collect();
    args.push(script.as_os_str());
    run(&iscc, &args)?;
    let setup = out.join(SETUP_EXE);
    if !setup.is_file() {
        return Err(format!("{} was not written", setup.display()));
    }
    println!("package-windows: {}", setup.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_workspace_version_only() {
        let manifest =
            "[package]\nversion = \"9.9.9\"\n[workspace.package]\nedition = \"2024\"\nversion = \"0.2.0\"\n[dependencies]\nversion = \"1\"\n";
        assert_eq!(version_from_manifest(manifest).as_deref(), Some("0.2.0"));
        assert_eq!(version_from_manifest("[workspace]\nmembers = []\n"), None);
    }

    #[test]
    fn the_real_manifest_has_a_version() {
        assert!(workspace_version().is_ok_and(|v| !v.is_empty()));
    }

    #[test]
    fn prereleases_sort_first_in_debian_and_drop_out_on_windows() {
        assert_eq!(debian_version("0.2.0"), "0.2.0");
        assert_eq!(debian_version("0.2.0-beta.1"), "0.2.0~beta.1");
        assert_eq!(numeric_version("0.2.0-beta.1"), "0.2.0");
        assert_eq!(numeric_version("1.0.0+build.5"), "1.0.0");
    }

    #[test]
    fn options() {
        let args: Vec<String> =
            ["--target", "aarch64-apple-darwin", "--no-build", "--sign", "Developer ID Application: X"].map(String::from).to_vec();
        let parsed = parse_options(&args);
        assert!(parsed.as_ref().is_ok_and(|o| o.no_build && o.target.as_deref() == Some("aarch64-apple-darwin") && o.sign.is_some()));
        assert!(parse_options(&["--sign".to_string()]).is_err());
        assert!(parse_options(&["--bogus".to_string()]).is_err());
    }

    #[test]
    fn deb_control_is_well_formed() {
        let control = deb_control("0.2.0-beta.1", 12345);
        assert!(control.contains("Version: 0.2.0~beta.1\n"));
        assert!(control.contains("Installed-Size: 12345\n"));
        assert!(control.ends_with('\n'));
        // Every continuation line of the description starts with one space.
        let description = control.split("Description: ").nth(1).unwrap_or_default();
        assert!(description.lines().skip(1).all(|l| l.starts_with(' ') && !l.starts_with("  ")));
    }

    #[test]
    fn the_plist_names_the_bundle_id_and_the_binary() {
        let plist = read(&packaging("macos/Info.plist")).unwrap_or_default();
        assert!(plist.contains("<string>com.methodstoryteller.desktop</string>"));
        assert!(plist.contains(&format!("<string>{APP_BIN}</string>")));
        assert!(plist.contains("{{VERSION}}"));
    }

    #[test]
    fn the_plist_declares_each_document_type_with_its_icon() {
        let plist = read(&packaging("macos/Info.plist")).unwrap_or_default();
        assert!(plist.contains("<key>CFBundleDocumentTypes</key>"));
        for kind in DOCUMENT_KINDS {
            assert!(plist.contains(&format!("<string>{kind}</string>")), "{kind} is declared as an extension");
            assert!(plist.contains(&format!("<string>document-{kind}</string>")), "{kind} names its icon");
            assert!(packaging(&format!("icons/document-{kind}.icns")).is_file(), "{kind}'s icon is in packaging/icons");
        }
        // Each declared type is an Editor at Default rank: open formats, so the app claims none.
        assert_eq!(plist.matches("<string>Editor</string>").count(), DOCUMENT_KINDS.len());
        assert_eq!(plist.matches("<string>Default</string>").count(), DOCUMENT_KINDS.len());
        assert!(!plist.contains("<string>Owner</string>"));
    }
}
