//! The Help menu's windows (About, Keyboard Shortcuts, What's New, Licenses and Notices) and
//! the links it opens. Nothing here touches the network: links open in the browser, and a bug
//! report is only a prefilled page the writer submits, or not, themselves.

use storyteller_engine::APP_NAME;

use crate::automation::Automation;

/// The app's website.
pub const WEBSITE: &str = "https://methodstoryteller.com";
/// The suite's website.
pub const SUITE_WEBSITE: &str = "https://methodproduction.com";
/// The user guide, `docs/user-guide.md` in the repository.
pub const USER_GUIDE: &str = "https://github.com/method-production-suite/method-storyteller/blob/main/docs/user-guide.md";
/// Where a bug report goes.
pub const NEW_ISSUE: &str = "https://github.com/method-production-suite/method-storyteller/issues/new";

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
const LICENCE: &str = "MIT OR Apache-2.0";
const COPYRIGHT: &str = "Copyright (c) 2026 Method Production Suite Team and the Method Storyteller contributors.";
const CHANGELOG: &str = include_str!("../../../CHANGELOG.md");

/// Shown in Licenses and Notices, in this order.
const NOTICES: [(&str, &str); 5] = [
    ("MIT", include_str!("../../../LICENSE-MIT")),
    ("Apache-2.0", include_str!("../../../LICENSE-APACHE")),
    ("Notice", include_str!("../../../NOTICE")),
    ("Attribution", include_str!("../../../ATTRIBUTION.md")),
    ("Courier Prime (OFL)", include_str!("../../../assets/fonts/OFL.txt")),
];

/// `application/x-www-form-urlencoded`, as browsers write it.
fn form_encode(value: &str) -> String {
    let mut out = String::new();
    for b in value.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'*' | b'-' | b'.' | b'_' => out.push(char::from(b)),
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// The system, as the bug report names it: `macOS (aarch64)`.
pub fn os_label() -> String {
    let os = match std::env::consts::OS {
        "macos" => "macOS",
        "windows" => "Windows",
        "linux" => "Linux",
        other => other,
    };
    format!("{os} ({})", std::env::consts::ARCH)
}

/// A new-issue page with the version and system filled in. Nothing else goes in it.
pub fn new_issue_url(version: &str, os: &str) -> String {
    let body = format!(
        "<!-- Describe the problem. A script excerpt helps, but leave out anything private. -->\n\n\
         **What happened:**\n\n\n**What you expected:**\n\n\n**Steps to repeat it:**\n\n\n---\n{APP_NAME} {version} · {os}\n"
    );
    format!("{NEW_ISSUE}?body={}", form_encode(&body))
}

/// The changelog section for `version`, without its heading, or `None` if it has none.
pub fn release_notes<'a>(changelog: &'a str, version: &str) -> Option<&'a str> {
    let mut start = None;
    let mut offset = 0;
    for line in changelog.split_inclusive('\n') {
        let heading = line.strip_prefix("## ");
        if let Some(from) = start {
            if heading.is_some() {
                return Some(changelog.get(from..offset)?.trim());
            }
        } else if heading.and_then(|h| h.split_whitespace().next()).is_some_and(|v| v.trim_start_matches('v') == version) {
            start = Some(offset + line.len());
        }
        offset += line.len();
    }
    Some(changelog.get(start?..)?.trim())
}

/// `major.minor.patch`, ignoring a `v` prefix and any `-pre` or `+build` suffix.
fn semver(text: &str) -> Option<(u64, u64, u64)> {
    let core = text.trim().trim_start_matches('v').split(['-', '+']).next()?;
    let mut parts = core.split('.').map(|p| p.parse::<u64>().ok());
    let version = (parts.next()??, parts.next()??, parts.next()??);
    parts.next().is_none().then_some(version)
}

/// Whether `version` is newer than `last_seen`. Versions that do not parse count as newer when
/// they differ, so a stray value never hides What's New for good.
fn is_newer(version: &str, last_seen: &str) -> bool {
    match (semver(version), semver(last_seen)) {
        (Some(new), Some(old)) => new > old,
        _ => version != last_seen,
    }
}

/// Whether What's New should open by itself: only when this version is newer than the last one
/// that ran, never on a first launch and never after going back to an older build.
pub fn whats_new_due(last_seen: &str, version: &str) -> bool {
    !last_seen.is_empty() && is_newer(version, last_seen)
}

/// Whether to record `version` as the last one seen: on a first launch or an update, but not
/// when an older build runs, so the newer build does not show What's New again.
pub fn record_version(last_seen: &str, version: &str) -> bool {
    last_seen.is_empty() || is_newer(version, last_seen)
}

/// Opens one of the app's own `https` links in the browser, with the system's opener (no shell,
/// so nothing in the address is interpreted). The status line says what happened.
pub fn open_link(url: &str) -> Result<(), String> {
    if !url.starts_with("https://") {
        return Err(format!("Not a web address: {url}"));
    }
    let (program, args): (&str, &[&str]) = if cfg!(target_os = "macos") {
        ("open", &[])
    } else if cfg!(target_os = "windows") {
        ("rundll32", &["url.dll,FileProtocolHandler"])
    } else {
        ("xdg-open", &[])
    };
    match std::process::Command::new(program).args(args).arg(url).spawn() {
        Ok(mut child) => {
            // Reap the opener when it exits; it hands the link to the browser and quits.
            std::thread::spawn(move || {
                let _ = child.wait();
            });
            Ok(())
        }
        Err(error) => Err(format!("Could not open the browser ({error}). The address is {url}")),
    }
}

/// One shortcut row: what it does and the keys, already formatted for this system.
pub struct ShortcutRow {
    pub group: String,
    pub label: String,
    pub keys: String,
}

/// Which Help windows are open.
#[derive(Default)]
pub struct HelpWindows {
    pub about: bool,
    pub shortcuts: bool,
    pub whats_new: bool,
    /// The open Licenses and Notices tab.
    pub licenses: Option<usize>,
}

/// What a click inside a Help window asks the app to do.
pub enum HelpAction {
    Open(&'static str),
    Licenses,
}

impl HelpWindows {
    /// Names of the open windows, for `ui.state`.
    pub fn open(&self) -> Vec<&'static str> {
        let mut open = Vec::new();
        for (name, on) in [("about", self.about), ("shortcuts", self.shortcuts), ("whats_new", self.whats_new), ("licenses", self.licenses.is_some())]
        {
            if on {
                open.push(name);
            }
        }
        open
    }

    pub fn show(&mut self, ctx: &egui::Context, automation: &mut Automation, shortcuts: &[ShortcutRow]) -> Option<HelpAction> {
        let mut action = None;
        if self.about {
            self.about = self.about_window(ctx, automation, &mut action);
        }
        if self.shortcuts {
            self.shortcuts = shortcuts_window(ctx, automation, shortcuts);
        }
        if self.whats_new {
            self.whats_new = whats_new_window(ctx, automation);
        }
        if let Some(tab) = self.licenses {
            self.licenses = licenses_window(ctx, automation, tab);
        }
        action
    }

    fn about_window(&self, ctx: &egui::Context, automation: &mut Automation, action: &mut Option<HelpAction>) -> bool {
        let mut open = true;
        egui::Window::new(format!("About {APP_NAME}")).id(egui::Id::new("storyteller.about")).collapsible(false).resizable(false).show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(4.0);
                ui.heading(APP_NAME);
                ui.label(format!("Version {VERSION}"));
                ui.add_space(8.0);
                let site = ui.link(WEBSITE.trim_start_matches("https://"));
                automation.add("dialog.about.website", "link", "Website", &site);
                if site.clicked() {
                    *action = Some(HelpAction::Open(WEBSITE));
                }
                ui.add_space(8.0);
                ui.label(format!("Free and open source, under {LICENCE}."));
                ui.small(COPYRIGHT);
                ui.add_space(4.0);
                let suite = ui.link("Part of the Method Production Suite");
                automation.add("dialog.about.suite", "link", "Method Production Suite", &suite);
                if suite.clicked() {
                    *action = Some(HelpAction::Open(SUITE_WEBSITE));
                }
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    let licenses = ui.button("Licenses and Notices");
                    automation.add("dialog.about.licenses", "button", "Licenses and Notices", &licenses);
                    if licenses.clicked() {
                        *action = Some(HelpAction::Licenses);
                    }
                    let close = ui.button("Close");
                    automation.add("dialog.about.close", "button", "Close", &close);
                    if close.clicked() {
                        open = false;
                    }
                });
            });
        });
        open
    }
}

fn close_button(ui: &mut egui::Ui, automation: &mut Automation, window: &str) -> bool {
    let close = ui.button("Close");
    automation.add(format!("dialog.{window}.close"), "button", "Close", &close);
    close.clicked()
}

fn shortcuts_window(ctx: &egui::Context, automation: &mut Automation, rows: &[ShortcutRow]) -> bool {
    let mut open = true;
    egui::Window::new("Keyboard Shortcuts").id(egui::Id::new("storyteller.shortcuts")).collapsible(false).default_height(640.0).show(ctx, |ui| {
        egui::ScrollArea::vertical().max_height(620.0).show(ui, |ui| {
            let mut group = "";
            egui::Grid::new("storyteller.shortcut_grid").num_columns(2).spacing([24.0, 4.0]).striped(true).show(ui, |ui| {
                for row in rows {
                    if row.group != group {
                        group = &row.group;
                        ui.strong(group);
                        ui.end_row();
                    }
                    ui.label(&row.label);
                    ui.monospace(&row.keys);
                    ui.end_row();
                }
            });
        });
        ui.add_space(8.0);
        if close_button(ui, automation, "shortcuts") {
            open = false;
        }
    });
    open
}

/// The changelog's few Markdown forms (`###` headings, `-` items wrapped onto indented lines,
/// paragraphs), as `(heading, text)` blocks with the wrapped lines joined.
fn blocks(text: &str) -> Vec<(bool, String)> {
    let mut blocks: Vec<(bool, String)> = Vec::new();
    let mut open = false;
    for line in text.lines() {
        let line = line.replace('`', "");
        if let Some(heading) = line.strip_prefix("### ") {
            blocks.push((true, heading.trim().to_string()));
            open = false;
        } else if let Some(item) = line.strip_prefix("- ") {
            blocks.push((false, format!("•  {}", item.trim())));
            open = true;
        } else if line.trim().is_empty() {
            open = false;
        } else if let (true, Some((false, last))) = (open, blocks.last_mut()) {
            last.push(' ');
            last.push_str(line.trim());
        } else {
            blocks.push((false, line.trim().to_string()));
            open = true;
        }
    }
    blocks
}

fn markdown(ui: &mut egui::Ui, text: &str) {
    for (heading, text) in blocks(text) {
        if heading {
            ui.add_space(6.0);
            ui.strong(text);
        } else {
            ui.label(text);
        }
    }
}

fn whats_new_window(ctx: &egui::Context, automation: &mut Automation) -> bool {
    let mut open = true;
    egui::Window::new("What's New").id(egui::Id::new("storyteller.whats_new")).collapsible(false).default_width(460.0).show(ctx, |ui| {
        ui.heading(format!("{APP_NAME} {VERSION}"));
        egui::ScrollArea::vertical().max_height(420.0).show(ui, |ui| match release_notes(CHANGELOG, VERSION) {
            Some(notes) => markdown(ui, notes),
            None => {
                ui.label("No notes for this version.");
            }
        });
        ui.add_space(8.0);
        if close_button(ui, automation, "whats_new") {
            open = false;
        }
    });
    open
}

fn licenses_window(ctx: &egui::Context, automation: &mut Automation, tab: usize) -> Option<usize> {
    let mut chosen = tab;
    let mut open = true;
    egui::Window::new("Licenses and Notices").id(egui::Id::new("storyteller.licenses")).collapsible(false).default_size([760.0, 520.0]).show(
        ctx,
        |ui| {
            ui.horizontal_wrapped(|ui| {
                for (n, (name, _)) in NOTICES.iter().enumerate() {
                    let response = ui.selectable_label(n == chosen, *name);
                    automation.add(format!("dialog.licenses.tab.{n}"), "tab", *name, &response);
                    if response.clicked() {
                        chosen = n;
                    }
                }
            });
            ui.separator();
            let text = NOTICES.get(chosen).map_or("", |(_, text)| text);
            egui::ScrollArea::both().max_height(440.0).auto_shrink([false, true]).show(ui, |ui| {
                // The files are wrapped already; wrapping again would break their lines.
                ui.add(egui::Label::new(egui::RichText::new(text).monospace().size(11.5)).extend());
            });
            ui.add_space(8.0);
            if close_button(ui, automation, "licenses") {
                open = false;
            }
        },
    );
    open.then_some(chosen)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn form_encoding_matches_browsers() {
        assert_eq!(form_encode("C++ a/b"), "C%2B%2B+a%2Fb");
        assert_eq!(form_encode("·"), "%C2%B7");
        assert_eq!(form_encode("a-b_c.d*"), "a-b_c.d*");
    }

    #[test]
    fn a_bug_report_carries_the_version_and_system_only() {
        let url = new_issue_url("0.1.0", "macOS (aarch64)");
        assert!(url.starts_with("https://github.com/method-production-suite/method-storyteller/issues/new?body="));
        assert!(url.ends_with("Method+Storyteller+0.1.0+%C2%B7+macOS+%28aarch64%29%0A"), "{url}");
        assert!(!url.contains("title="));
    }

    #[test]
    fn the_system_label_names_the_os_and_the_chip() {
        let label = os_label();
        assert!(label.contains(std::env::consts::ARCH), "{label}");
    }

    #[test]
    fn release_notes_are_the_section_for_this_version() {
        let log = "# Changelog\n\n## 0.2.0\n\n- Newer.\n\n## v0.1.0 (first)\n\n### New\n\n- First.\n";
        assert_eq!(release_notes(log, "0.2.0"), Some("- Newer."));
        assert_eq!(release_notes(log, "0.1.0"), Some("### New\n\n- First."));
        assert_eq!(release_notes(log, "0.1"), None);
        assert_eq!(release_notes(log, "9.9.9"), None);
        assert_eq!(release_notes("", "0.1.0"), None);
    }

    #[test]
    fn the_changelog_has_notes_for_this_version() {
        assert!(release_notes(CHANGELOG, VERSION).is_some_and(|n| !n.is_empty()), "CHANGELOG.md needs a `## {VERSION}` section");
    }

    #[test]
    fn wrapped_changelog_lines_join_their_item() {
        let got = blocks("### New\n\n- One that\n  wraps.\n- Two `code`.\n\nA paragraph\non two lines.\n");
        let want = [(true, "New"), (false, "•  One that wraps."), (false, "•  Two code."), (false, "A paragraph on two lines.")];
        assert_eq!(got, want.map(|(h, t)| (h, t.to_string())));
    }

    #[test]
    fn whats_new_opens_once_after_an_update() {
        assert!(!whats_new_due("", "0.1.0"), "not on a first launch");
        assert!(!whats_new_due("0.1.0", "0.1.0"));
        assert!(whats_new_due("0.1.0", "0.2.0"));
        assert!(whats_new_due("0.9.0", "0.10.0"), "numbers, not text");
    }

    #[test]
    fn an_older_build_neither_shows_whats_new_nor_moves_the_version_back() {
        assert!(!whats_new_due("0.2.0", "0.1.0"));
        assert!(!record_version("0.2.0", "0.1.0"));
        assert!(!record_version("0.2.0", "0.2.0"));
        assert!(record_version("0.1.0", "0.2.0"));
        assert!(record_version("", "0.1.0"));
    }

    #[test]
    fn versions_that_do_not_parse_fall_back_to_a_plain_comparison() {
        assert_eq!(semver("v1.2.3-beta.1"), Some((1, 2, 3)));
        assert_eq!(semver("1.2"), None);
        assert!(whats_new_due("garbage", "0.1.0"));
        assert!(!whats_new_due("garbage", "garbage"));
    }

    #[test]
    fn every_notice_is_embedded() {
        for (name, text) in NOTICES {
            assert!(text.len() > 100, "{name} is empty");
        }
        assert!(NOTICES.iter().any(|(_, t)| t.contains("Apache License")));
    }

    #[test]
    fn only_web_addresses_are_opened() {
        assert!(open_link("file:///etc/passwd").is_err());
        assert!(open_link("-a Calculator").is_err());
    }

    #[test]
    fn the_help_windows_report_what_is_open() {
        let windows = HelpWindows { about: true, licenses: Some(2), ..HelpWindows::default() };
        assert_eq!(windows.open(), ["about", "licenses"]);
    }
}
