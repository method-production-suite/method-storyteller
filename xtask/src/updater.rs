//! The files a release gives the suite updater (`method-update`), so installed apps on Apple
//! silicon Macs find the new version (SUITE-STANDARDS.md §1, §3):
//!
//! - `cargo xtask updater-archive --app <Method Storyteller.app> --version X.Y.Z --out-dir D`
//!   packs the signed, notarized, stapled bundle as `Method-Storyteller_X.Y.Z_aarch64.app.tar.gz`
//!   (top folder `Method Storyteller.app`) and signs it with the update key, read from
//!   `UPDATER_PRIVATE_KEY` (base64 of the minisign secret key box) and
//!   `UPDATER_PRIVATE_KEY_PASSWORD`. The trusted comment names the version, which the
//!   updater checks.
//! - `cargo xtask update-manifest --dir D --version X.Y.Z --out D/latest.json` writes the
//!   manifest in the latest.json static format, after checking the signature against the
//!   public key the app itself carries (crates/app/src/updates.rs), so a release can never
//!   publish an update its own apps would refuse.

use std::fs::File;
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};

use base64::Engine;
use base64::engine::general_purpose::STANDARD;

const REPO: &str = "method-production-suite/method-storyteller";
/// The bundle's name inside the archive. The updater swaps in whatever top folder it finds,
/// so this is for people reading the archive.
const BUNDLE: &str = "Method Storyteller.app";
/// The updater's only platform.
const PLATFORM: &str = "darwin-aarch64";

pub fn archive_name(version: &str) -> String {
    format!("Method-Storyteller_{version}_aarch64.app.tar.gz")
}

struct Args {
    values: Vec<(String, String)>,
}

impl Args {
    fn parse(rest: &[String]) -> Result<Self, String> {
        let mut values = Vec::new();
        let mut it = rest.iter();
        while let Some(flag) = it.next() {
            let name = flag.strip_prefix("--").ok_or_else(|| format!("unexpected argument {flag}"))?;
            let value = it.next().ok_or_else(|| format!("--{name} needs a value"))?;
            values.push((name.to_string(), value.clone()));
        }
        Ok(Args { values })
    }

    fn get(&self, name: &str) -> Result<&str, String> {
        self.values.iter().find(|(n, _)| n == name).map(|(_, v)| v.as_str()).ok_or_else(|| format!("missing --{name}"))
    }
}

/// `cargo xtask updater-archive`.
pub fn archive_command(rest: &[String]) -> Result<(), String> {
    let args = Args::parse(rest)?;
    let key = std::env::var("UPDATER_PRIVATE_KEY").map_err(|_| "UPDATER_PRIVATE_KEY is not set".to_string())?;
    let password = std::env::var("UPDATER_PRIVATE_KEY_PASSWORD").unwrap_or_default();
    let archive = write_archive(Path::new(args.get("app")?), args.get("version")?, Path::new(args.get("out-dir")?))?;
    let signature = sign(&archive, args.get("version")?, &key, &password)?;
    let sig_path = PathBuf::from(format!("{}.sig", archive.display()));
    std::fs::write(&sig_path, signature).map_err(|e| format!("{}: {e}", sig_path.display()))?;
    println!("updater-archive: {} and its .sig", archive.display());
    Ok(())
}

/// `cargo xtask update-manifest`.
pub fn manifest_command(root: &Path, rest: &[String]) -> Result<(), String> {
    let args = Args::parse(rest)?;
    let version = args.get("version")?;
    let pubkey = app_public_key(root)?;
    let notes = std::fs::read_to_string(root.join("docs/releases").join(format!("{version}.md"))).unwrap_or_default();
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
    let manifest = manifest(Path::new(args.get("dir")?), version, &pubkey, &notes, now)?;
    let out = args.get("out")?;
    std::fs::write(out, manifest).map_err(|e| format!("{out}: {e}"))?;
    println!("update-manifest: wrote {out}");
    Ok(())
}

/// Packs `app` as the updater archive in `out_dir`.
pub fn write_archive(app: &Path, version: &str, out_dir: &Path) -> Result<PathBuf, String> {
    if !app.join("Contents/Info.plist").is_file() {
        return Err(format!("{} is not an app bundle", app.display()));
    }
    std::fs::create_dir_all(out_dir).map_err(|e| e.to_string())?;
    let path = out_dir.join(archive_name(version));
    let file = File::create(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let gz = flate2::write::GzEncoder::new(file, flate2::Compression::best());
    let mut tar = tar::Builder::new(gz);
    // Links stay links: a signed bundle's own symlinks must survive the trip.
    tar.follow_symlinks(false);
    tar.append_dir_all(BUNDLE, app).map_err(|e| format!("packing {}: {e}", app.display()))?;
    let gz = tar.into_inner().map_err(|e| e.to_string())?;
    gz.finish().map_err(|e| e.to_string())?;
    Ok(path)
}

/// The `.sig` content for an archive: base64 of the minisign signature box, with
/// `timestamp:<t>\tfile:<name>\tversion:<v>` as its trusted comment.
pub fn sign(archive: &Path, version: &str, key_b64: &str, password: &str) -> Result<String, String> {
    let key_box = decode_text(key_b64.trim()).map_err(|e| format!("the update key: {e}"))?;
    let secret = minisign::SecretKeyBox::from_string(&key_box)
        .and_then(|b| b.into_secret_key(Some(password.to_string())))
        .map_err(|e| format!("the update key would not open: {e}"))?;
    let name = archive.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let mut bytes = Vec::new();
    File::open(archive).and_then(|mut f| f.read_to_end(&mut bytes)).map_err(|e| format!("{}: {e}", archive.display()))?;
    let timestamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
    let trusted = format!("timestamp:{timestamp}\tfile:{name}\tversion:{version}");
    // With the public key, minisign checks the new signature before it returns it.
    let public = minisign::PublicKey::from_secret_key(&secret).map_err(|e| format!("the update key's public half: {e}"))?;
    let signature =
        minisign::sign(Some(&public), &secret, Cursor::new(&bytes), Some(&trusted), Some("signature from the Method Storyteller update key"))
            .map_err(|e| format!("signing {name}: {e}"))?;
    Ok(STANDARD.encode(signature.to_string()))
}

/// latest.json for the archive in `dir`, refused unless its signature checks against `pubkey`
/// and names `version`.
pub fn manifest(dir: &Path, version: &str, pubkey_b64: &str, notes: &str, unix_now: u64) -> Result<String, String> {
    let name = archive_name(version);
    let archive = dir.join(&name);
    let sig_path = dir.join(format!("{name}.sig"));
    let bytes = std::fs::read(&archive).map_err(|e| format!("{}: {e}", archive.display()))?;
    let sig_b64 = std::fs::read_to_string(&sig_path).map_err(|e| format!("{}: {e}", sig_path.display()))?;
    let sig_b64 = sig_b64.trim();
    let public = minisign::PublicKeyBox::from_string(&decode_text(pubkey_b64)?)
        .and_then(|b| b.into_public_key())
        .map_err(|e| format!("the app's public key: {e}"))?;
    let signature = minisign::SignatureBox::from_string(&decode_text(sig_b64)?).map_err(|e| format!("{name}.sig: {e}"))?;
    minisign::verify(&public, &signature, Cursor::new(&bytes), true, false, false)
        .map_err(|e| format!("{name} does not verify against the app's public key: {e}"))?;
    let trusted = signature.trusted_comment().map_err(|e| e.to_string())?;
    let signed = trusted.split('\t').find_map(|f| f.strip_prefix("version:"));
    if signed != Some(version) {
        return Err(format!("{name}.sig is signed for version {signed:?}, not {version}"));
    }
    let manifest = serde_json::json!({
        "version": version,
        "notes": notes,
        "pub_date": rfc3339(unix_now),
        "platforms": {
            PLATFORM: {
                "signature": sig_b64,
                "url": format!("https://github.com/{REPO}/releases/download/v{version}/{name}"),
            }
        }
    });
    serde_json::to_string_pretty(&manifest).map(|s| s + "\n").map_err(|e| e.to_string())
}

/// The `PUBKEY` constant in crates/app/src/updates.rs.
fn app_public_key(root: &Path) -> Result<String, String> {
    let source = std::fs::read_to_string(root.join("crates/app/src/updates.rs")).map_err(|e| e.to_string())?;
    let line = source.lines().find(|l| l.trim_start().starts_with("const PUBKEY: &str = \"")).ok_or("no PUBKEY in updates.rs")?;
    let key = line.split('"').nth(1).unwrap_or_default();
    if key.is_empty() {
        return Err("PUBKEY in updates.rs is empty: the app would refuse any update".into());
    }
    Ok(key.to_string())
}

fn decode_text(b64: &str) -> Result<String, String> {
    let bytes = STANDARD.decode(b64.trim()).map_err(|e| format!("not base64: {e}"))?;
    String::from_utf8(bytes).map_err(|_| "not UTF-8 inside the base64".to_string())
}

/// `2026-10-18T12:00:00Z` for a Unix time (UTC).
fn rfc3339(secs: u64) -> String {
    let days = secs / 86_400;
    let rest = secs % 86_400;
    // Civil from days (Howard Hinnant's algorithm).
    let z = days as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z", rest / 3600, rest % 3600 / 60, rest % 60)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]
    use super::*;

    /// A throwaway bundle, a throwaway key in the release secret's shape, and a full trip.
    #[test]
    fn archive_sign_and_manifest_round_trip() {
        let dir = std::env::temp_dir().join(format!("storyteller-updater-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let app = dir.join(BUNDLE);
        std::fs::create_dir_all(app.join("Contents/MacOS")).unwrap();
        std::fs::write(app.join("Contents/Info.plist"), "<plist/>").unwrap();
        std::fs::write(app.join("Contents/MacOS/method-storyteller-app"), "binary").unwrap();

        let pair = minisign::KeyPair::generate_encrypted_keypair(Some("pw".into())).unwrap();
        let secret_b64 = STANDARD.encode(pair.sk.to_box(None).unwrap().to_string());
        let public_b64 = STANDARD.encode(pair.pk.to_box().unwrap().to_string());

        let out = dir.join("updater");
        let archive = write_archive(&app, "0.2.0", &out).unwrap();
        assert_eq!(archive.file_name().unwrap().to_string_lossy(), "Method-Storyteller_0.2.0_aarch64.app.tar.gz");
        // The archive holds the bundle under its own name.
        let mut names = Vec::new();
        let mut reader = tar::Archive::new(flate2::read::GzDecoder::new(File::open(&archive).unwrap()));
        for entry in reader.entries().unwrap() {
            names.push(entry.unwrap().path().unwrap().display().to_string());
        }
        assert!(names.iter().any(|n| n == "Method Storyteller.app/Contents/MacOS/method-storyteller-app"), "{names:?}");

        let sig = sign(&archive, "0.2.0", &secret_b64, "pw").unwrap();
        std::fs::write(format!("{}.sig", archive.display()), &sig).unwrap();
        assert!(sign(&archive, "0.2.0", &secret_b64, "wrong").is_err(), "a wrong password does not sign");

        let json = manifest(&out, "0.2.0", &public_b64, "Notes.", 1_792_281_600).unwrap();
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(value["version"], "0.2.0");
        assert_eq!(value["pub_date"], "2026-10-18T00:00:00Z");
        assert_eq!(value["platforms"]["darwin-aarch64"]["signature"], sig.as_str());
        assert_eq!(
            value["platforms"]["darwin-aarch64"]["url"],
            "https://github.com/method-production-suite/method-storyteller/releases/download/v0.2.0/Method-Storyteller_0.2.0_aarch64.app.tar.gz"
        );

        // The suite updater itself accepts it: the signature with the version it announces,
        // and the archive unpacks to the bundle.
        let archive_bytes = std::fs::read(&archive).unwrap();
        method_update::verify::verify_signature(&archive_bytes, &sig, &public_b64, "0.2.0", true).unwrap();
        assert!(method_update::verify::verify_signature(&archive_bytes, &sig, &public_b64, "0.3.0", true).is_err());
        let unpacked = dir.join("unpacked");
        method_update::install::unpack_app_archive(&archive_bytes, &unpacked).unwrap();
        let found: Vec<_> = walk(&unpacked).into_iter().filter(|p| p.ends_with("Contents/MacOS/method-storyteller-app")).collect();
        assert_eq!(found.len(), 1, "the unpacked archive holds the app binary: {:?}", walk(&unpacked));

        // Refused: another version, another key, a changed archive.
        assert!(manifest(&out, "0.3.0", &public_b64, "", 0).is_err());
        let other = minisign::KeyPair::generate_unencrypted_keypair().unwrap();
        let other_b64 = STANDARD.encode(other.pk.to_box().unwrap().to_string());
        assert!(manifest(&out, "0.2.0", &other_b64, "", 0).unwrap_err().contains("does not verify"));
        std::fs::write(&archive, b"tampered").unwrap();
        assert!(manifest(&out, "0.2.0", &public_b64, "", 0).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn walk(dir: &Path) -> Vec<PathBuf> {
        let mut out = Vec::new();
        for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
            let path = entry.path();
            if path.is_dir() {
                out.extend(walk(&path));
            } else {
                out.push(path);
            }
        }
        out
    }

    #[test]
    fn the_app_carries_a_public_key() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        let key = app_public_key(root).unwrap();
        assert!(decode_text(&key).unwrap().contains("minisign public key"));
    }
}
