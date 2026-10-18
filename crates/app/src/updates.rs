//! Help > Check for Updates, the automatic checks and "Restart to Update", through the suite's
//! updater (`method-update`, SUITE-STANDARDS.md §1). Apple silicon Macs only.
//!
//! The updater stays inactive, with no timer and no network, until Storyteller has an updater
//! key: [`PUBKEY`] is empty, so `AppConfig::unavailable_reason` reports "no updater key" and the
//! menu item says updates arrive with the first release.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use method_update::policy::TICK;
use method_update::{AppConfig, Said, Status, Updater, baked};

/// Where this app looks for a new version.
const ENDPOINT: &str = "https://github.com/method-production-suite/method-storyteller/releases/latest/download/latest.json";
/// The minisign public key the releases are signed with. Empty until the key exists (MOrg
/// arranges it): the updater then never checks.
const PUBKEY: &str = "";

/// The bundle as the installer names it.
pub const BUNDLE_NAME: &str = "Method Storyteller.app";

/// The updater has one target, `darwin-aarch64`: any other build never touches the network.
const APPLE_SILICON: bool = cfg!(all(target_os = "macos", target_arch = "aarch64"));

/// What the menu says while this build cannot update itself.
pub const UPDATES_LATER: &str = "Updates arrive with the first release";
/// Anywhere but an Apple silicon Mac.
pub const MAC_ONLY: &str = "Updates come through the app on Apple silicon Macs; elsewhere, download new versions from methodproduction.com";

pub fn update_config() -> AppConfig {
    AppConfig::new(
        storyteller_engine::APP_NAME,
        BUNDLE_NAME,
        concat!("method-storyteller-update/", env!("CARGO_PKG_VERSION")),
        // A rehearsal build may bake a local endpoint, a throwaway key and plain HTTP from
        // METHOD_STORYTELLER_UPDATE_ENDPOINT, _PUBKEY and _INSECURE; build.rs refuses them
        // in any other build.
        baked!(prefix = "METHOD_STORYTELLER_UPDATE", rehearsal = cfg!(feature = "rehearsal"), endpoint = ENDPOINT, pubkey = PUBKEY,),
    )
}

/// Puts back a bundle an interrupted update left hidden, before the window opens.
pub fn sweep_at_startup() {
    if APPLE_SILICON && let Ok(exe) = std::env::current_exe() {
        let swept = method_update::install::sweep_at_startup(&exe, &update_config().bundle);
        if swept.restored.is_some() || !swept.removed.is_empty() {
            eprintln!("update sweep: restored {:?}, removed {:?}", swept.restored, swept.removed);
        }
    }
}

/// The running app's updater and the answer to the last check the writer asked for.
pub struct Updates {
    updater: Arc<Updater>,
    said: Arc<Mutex<Option<Said>>>,
    asking: Arc<AtomicBool>,
}

impl Updates {
    /// `settings`: the `updates.json` file (`None` keeps the switch in memory, for tests).
    /// `initial_automatic`: the switch from older settings, used while nothing is stored yet.
    pub fn start(ctx: &egui::Context, settings: Option<PathBuf>, initial_automatic: bool) -> Self {
        let mut updater =
            Updater::new(update_config(), env!("CARGO_PKG_VERSION"), settings, Instant::now()).with_initial_automatic(initial_automatic);
        if let Some(bundle) = std::env::current_exe().ok().as_deref().and_then(method_update::install::bundle_of_exe) {
            updater = updater.with_bundle(bundle);
        }
        let updater = Arc::new(updater);
        // The schedule: a look every TICK, a check when one is due. A build that cannot
        // update itself starts no timer at all.
        if APPLE_SILICON && updater.unavailable().is_none() {
            let (u, ctx) = (updater.clone(), ctx.clone());
            std::thread::spawn(move || {
                loop {
                    std::thread::sleep(TICK);
                    if u.due(Instant::now()) {
                        u.check_now(false);
                        ctx.request_repaint();
                    }
                }
            });
        }
        Updates { updater, said: Arc::default(), asking: Arc::default() }
    }

    /// Why Check for Updates cannot run here, for the menu's tooltip; `None` when it can.
    pub fn unavailable(&self) -> Option<&'static str> {
        if !APPLE_SILICON {
            Some(MAC_ONLY)
        } else if self.updater.unavailable().is_some() {
            Some(UPDATES_LATER)
        } else {
            None
        }
    }

    pub fn automatic(&self) -> bool {
        self.updater.automatic()
    }

    pub fn set_automatic(&self, on: bool) -> std::io::Result<()> {
        self.updater.set_automatic(on)
    }

    /// Help > Check for Updates…: checks off the UI thread; the answer shows on the next frame.
    pub fn check(&self, ctx: &egui::Context) {
        if self.unavailable().is_some() || self.asking.swap(true, Ordering::SeqCst) {
            return;
        }
        let (u, said, asking, ctx) = (self.updater.clone(), self.said.clone(), self.asking.clone(), ctx.clone());
        std::thread::spawn(move || {
            let answer = u.check_now(true);
            if let Ok(mut slot) = said.lock() {
                *slot = answer;
            }
            asking.store(false, Ordering::SeqCst);
            ctx.request_repaint();
        });
    }

    /// What to show now: the phase, plus a one-time answer, handed over once. A failure is
    /// shown once: then the updater goes back to idle.
    pub fn status(&self) -> Status {
        let said = self.said.lock().ok().and_then(|mut slot| slot.take());
        let status = self.updater.status(said);
        if status.phase == "failed" {
            self.updater.dismiss();
        }
        status
    }

    /// "Restart to Update": swaps the bundle, starts the new one, and the caller exits.
    pub fn install_and_relaunch(&self) -> Result<(), String> {
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let bundle = method_update::install::bundle_of_exe(&exe).ok_or("Method Storyteller is not running from an app bundle")?;
        let names = self.updater.app().bundle.clone();
        let installed = self
            .updater
            .install_ready(&bundle, &|b: &Path, new: &Path| method_update::install::admin_swap(b, new, &names))
            .map_err(|e| e.to_string())?;
        method_update::restart::relaunch(&installed.bundle).map_err(|e| e.to_string())
    }
}

/// The status line for an update status, or `None` when there is nothing to say.
pub fn status_line(status: &Status) -> Option<String> {
    let version = status.version.as_deref().unwrap_or("");
    match (status.phase, status.said) {
        (_, Some(Said::Current)) => Some(format!("{} {} is up to date.", storyteller_engine::APP_NAME, env!("CARGO_PKG_VERSION"))),
        (_, Some(Said::ReadOnly)) | ("read-only", _) if status.manual || status.said.is_some() => status.message.clone(),
        (_, Some(Said::Failed)) | ("failed", _) => {
            Some(format!("Could not check for updates: {}", status.message.as_deref().unwrap_or("unknown error")))
        }
        (_, Some(Said::InstallFailed)) => Some("The update could not be installed; it is still ready to try again.".into()),
        ("checking", _) if status.manual => Some("Checking for updates…".into()),
        ("downloading", _) if status.manual => Some(format!("Downloading {version}…")),
        ("ready", _) => Some(format!("Version {version} is ready.")),
        ("installing", _) => Some(format!("Installing {version}…")),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn status(phase: &'static str, said: Option<Said>) -> Status {
        Status { phase, version: Some("0.3.0".into()), manual: true, message: Some("offline".into()), said, automatic: true, available: true }
    }

    #[test]
    fn this_build_has_no_updater_key_yet() {
        assert!(update_config().unavailable_reason().is_some_and(|r| r.contains("no updater key")));
    }

    #[test]
    fn status_lines() {
        assert_eq!(status_line(&status("idle", Some(Said::Current))).as_deref().map(|s| s.ends_with("is up to date.")), Some(true));
        assert_eq!(status_line(&status("failed", Some(Said::Failed))).as_deref(), Some("Could not check for updates: offline"));
        assert_eq!(status_line(&status("checking", None)).as_deref(), Some("Checking for updates…"));
        assert_eq!(status_line(&status("ready", None)).as_deref(), Some("Version 0.3.0 is ready."));
        assert_eq!(status_line(&Status { manual: false, ..status("checking", None) }), None, "automatic checks are quiet");
        assert_eq!(status_line(&status("idle", None)), None);
    }
}
