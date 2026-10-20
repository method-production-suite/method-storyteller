//! The macOS menu bar, built from the same menu entries the window menu shows (muda).
//!
//! Other systems keep the menu bar drawn inside the window. Cut, Copy, Paste and Select All have
//! no accelerators on purpose: with them, the menu would take ⌘C, ⌘V and the rest before the
//! editor's text fields see them. The keys go to the text fields; the items do the same.

use std::cell::RefCell;
use std::sync::mpsc::{Receiver, channel};

use muda::accelerator::{Accelerator, Code, Modifiers};
use muda::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu};

/// One menu entry: command id, label, shortcut text (`Cmd+Shift+S`) and whether it is a toggle.
pub struct Entry {
    pub id: String,
    pub label: String,
    pub shortcut: Option<String>,
    pub check: Option<bool>,
}

enum Item {
    Normal(MenuItem),
    Check(CheckMenuItem),
}

thread_local! {
    /// muda's items must outlive the native menu that shows them.
    static MENU: RefCell<Option<Menu>> = const { RefCell::new(None) };
}

/// The installed menu bar: chosen command ids arrive on `events`.
pub struct NativeMenu {
    items: Vec<(String, Item, bool, bool)>,
    events: Receiver<String>,
    /// File > Open Recent, rebuilt when the list changes, and the items it shows.
    recent: Option<(Submenu, Vec<(String, String)>)>,
}

/// The id of the entry that stands for the Open Recent submenu.
const OPEN_RECENT: &str = "app.open_recent";

/// `Cmd+Shift+S`, `Cmd+1`, `Cmd+Equals` as a muda accelerator.
pub fn accelerator(text: &str) -> Option<Accelerator> {
    let mut modifiers = Modifiers::empty();
    let mut code = None;
    for part in text.split('+') {
        match part {
            "Cmd" => modifiers |= Modifiers::META,
            "Shift" => modifiers |= Modifiers::SHIFT,
            "Alt" => modifiers |= Modifiers::ALT,
            "Equals" => code = Some(Code::Equal),
            "Minus" => code = Some(Code::Minus),
            "Semicolon" => code = Some(Code::Semicolon),
            key => {
                let mut chars = key.chars();
                let (Some(c), None) = (chars.next(), chars.next()) else { return None };
                code = Some(match c.to_ascii_uppercase() {
                    'A' => Code::KeyA,
                    'B' => Code::KeyB,
                    'C' => Code::KeyC,
                    'D' => Code::KeyD,
                    'E' => Code::KeyE,
                    'F' => Code::KeyF,
                    'G' => Code::KeyG,
                    'H' => Code::KeyH,
                    'I' => Code::KeyI,
                    'J' => Code::KeyJ,
                    'K' => Code::KeyK,
                    'L' => Code::KeyL,
                    'M' => Code::KeyM,
                    'N' => Code::KeyN,
                    'O' => Code::KeyO,
                    'P' => Code::KeyP,
                    'Q' => Code::KeyQ,
                    'R' => Code::KeyR,
                    'S' => Code::KeyS,
                    'T' => Code::KeyT,
                    'U' => Code::KeyU,
                    'V' => Code::KeyV,
                    'W' => Code::KeyW,
                    'X' => Code::KeyX,
                    'Y' => Code::KeyY,
                    'Z' => Code::KeyZ,
                    '0' => Code::Digit0,
                    '1' => Code::Digit1,
                    '2' => Code::Digit2,
                    '3' => Code::Digit3,
                    '4' => Code::Digit4,
                    '5' => Code::Digit5,
                    '6' => Code::Digit6,
                    '7' => Code::Digit7,
                    '8' => Code::Digit8,
                    '9' => Code::Digit9,
                    _ => return None,
                });
            }
        }
    }
    Some(Accelerator::new(modifiers, code?))
}

impl NativeMenu {
    /// Builds the app menu, then one menu per `(title, entries)` with Window before Help, and
    /// installs it. Quit goes through craft-quit so unsaved changes are asked about first.
    /// About opens the app's own About window (`help.about`), the same one Help > About shows.
    pub fn install(ctx: &egui::Context, app_name: &str, menus: &[(&str, Vec<Entry>)]) -> muda::Result<NativeMenu> {
        let bar = Menu::new();
        let quit = MenuItem::with_id(craft_quit::QUIT_MENU_ID, format!("Quit {app_name}"), true, accelerator("Cmd+Q"));
        let app_menu = Submenu::new(app_name, true);
        app_menu.append_items(&[
            &MenuItem::with_id("help.about", format!("About {app_name}"), true, None),
            &PredefinedMenuItem::separator(),
            &PredefinedMenuItem::services(None),
            &PredefinedMenuItem::separator(),
            &PredefinedMenuItem::hide(Some(&format!("Hide {app_name}"))),
            &PredefinedMenuItem::hide_others(None),
            &PredefinedMenuItem::show_all(None),
            &PredefinedMenuItem::separator(),
            &quit,
        ])?;
        bar.append(&app_menu)?;

        let mut items = Vec::new();
        let mut help = None;
        let mut recent = None;
        for (title, entries) in menus {
            let submenu = Submenu::new(*title, true);
            for entry in entries {
                if entry.id == "app.quit" {
                    continue;
                }
                if entry.id == OPEN_RECENT {
                    let open_recent = Submenu::new(&entry.label, true);
                    submenu.append(&open_recent)?;
                    recent = Some((open_recent, Vec::new()));
                    continue;
                }
                let shortcut = entry.shortcut.as_deref().and_then(accelerator);
                let item = match entry.check {
                    Some(checked) => {
                        let item = CheckMenuItem::with_id(entry.id.as_str(), &entry.label, true, checked, shortcut);
                        submenu.append(&item)?;
                        Item::Check(item)
                    }
                    None => {
                        let item = MenuItem::with_id(entry.id.as_str(), &entry.label, true, shortcut);
                        submenu.append(&item)?;
                        Item::Normal(item)
                    }
                };
                items.push((entry.id.clone(), item, true, entry.check.unwrap_or(false)));
            }
            if *title == "Help" {
                help = Some(submenu);
            } else {
                bar.append(&submenu)?;
            }
        }

        let window = Submenu::new("Window", true);
        window.append_items(&[&PredefinedMenuItem::minimize(None), &PredefinedMenuItem::maximize(None), &PredefinedMenuItem::fullscreen(None)])?;
        bar.append(&window)?;
        window.set_as_windows_menu_for_nsapp();
        // Help comes last, and as the system Help menu macOS adds its search field to it.
        if let Some(help) = help {
            bar.append(&help)?;
            help.set_as_help_menu_for_nsapp();
        }
        bar.init_for_nsapp();

        let (sender, events) = channel();
        let ctx = ctx.clone();
        MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
            let id = event.id.as_ref();
            if !craft_quit::on_menu_event(&ctx, id) {
                let _ = sender.send(id.to_string());
                ctx.request_repaint();
            }
        }));
        MENU.set(Some(bar));
        let mut menu = NativeMenu { items, events, recent };
        // Filled by the first `set_recent`, so it is never left empty-looking by mistake.
        if let Some((_, shown)) = menu.recent.as_mut() {
            shown.push((String::new(), String::new()));
        }
        Ok(menu)
    }

    /// Shows `items` (command id, label) in File > Open Recent, then Clear Menu, rebuilding the
    /// submenu only when the list changed.
    pub fn set_recent(&mut self, items: &[(String, String)]) {
        let Some((submenu, shown)) = self.recent.as_mut() else { return };
        if shown.as_slice() == items {
            return;
        }
        for _ in 0..submenu.items().len() {
            let _ = submenu.remove_at(0);
        }
        for (id, label) in items {
            let _ = submenu.append(&MenuItem::with_id(id.as_str(), label, true, None));
        }
        let _ = submenu.append(&PredefinedMenuItem::separator());
        let _ = submenu.append(&MenuItem::with_id("app.clear_recent", "Clear Menu", !items.is_empty(), None));
        submenu.set_enabled(!items.is_empty());
        *shown = items.to_vec();
    }

    /// Command ids chosen since the last call.
    pub fn take_events(&self) -> Vec<String> {
        self.events.try_iter().collect()
    }

    /// Whether the menu has an item for `id` (so its shortcut is the menu's to handle).
    pub fn has(&self, id: &str) -> bool {
        self.items.iter().any(|(item, ..)| item == id)
    }

    /// Updates an item's enabled and checked state, touching the native menu only on a change.
    pub fn sync(&mut self, id: &str, enabled: bool, checked: Option<bool>) {
        for (item_id, item, was_enabled, was_checked) in &mut self.items {
            if item_id != id {
                continue;
            }
            if *was_enabled != enabled {
                match item {
                    Item::Normal(i) => i.set_enabled(enabled),
                    Item::Check(i) => i.set_enabled(enabled),
                }
                *was_enabled = enabled;
            }
            if let (Item::Check(i), Some(checked)) = (item, checked)
                && *was_checked != checked
            {
                i.set_checked(checked);
                *was_checked = checked;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_shortcut_text() {
        assert_eq!(accelerator("Cmd+Shift+S"), Some(Accelerator::new(Modifiers::META | Modifiers::SHIFT, Code::KeyS)));
        assert_eq!(accelerator("Cmd+1"), Some(Accelerator::new(Modifiers::META, Code::Digit1)));
        assert_eq!(accelerator("Cmd+Equals"), Some(Accelerator::new(Modifiers::META, Code::Equal)));
        assert_eq!(accelerator("Enter"), None);
    }
}
