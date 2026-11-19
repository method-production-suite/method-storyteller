//! The menus, in the window and in the native menu bar.

use super::*;

impl StorytellerApp {
    pub(super) fn menu_entries(&self, menu: &str) -> Vec<MenuEntry> {
        let prefix = format!("{menu}/");
        let mut items: Vec<MenuEntry> = self
            .registry
            .iter()
            .filter(|c| c.menu.starts_with(&prefix))
            .map(|c| MenuEntry {
                id: c.id.to_string(),
                label: c.menu.trim_start_matches(&prefix).to_string(),
                shortcut: c.shortcut,
                enabled: (c.enabled)(&self.session) || c.id == "file.save",
            })
            .collect();
        for command in APP_COMMANDS.iter().filter(|c| c.menu.starts_with(&prefix)) {
            let enabled = command.id != "help.check_updates" || self.updates.unavailable().is_none();
            items.push(MenuEntry { id: command.id.to_string(), label: command.label.to_string(), shortcut: command.shortcut, enabled });
        }
        // Open Recent, a submenu, straight after Open….
        if let Some(open) = items.iter().position(|e| e.id == "file.open") {
            let enabled = !self.recent.is_empty();
            items.insert(open + 1, MenuEntry { id: OPEN_RECENT.into(), label: "Open Recent".into(), shortcut: None, enabled });
        }
        items
    }

    /// File > Open Recent's items: (command id, label), most recent first.
    pub(super) fn recent_items(&self) -> Vec<(String, String)> {
        self.recent.paths().enumerate().map(|(n, path)| (format!("{RECENT_PREFIX}{n}"), recent_label(path))).collect()
    }

    /// The checked state of a toggle command, or `None` for an ordinary one.
    pub(super) fn checked(&self, id: &str) -> Option<bool> {
        match id {
            "view.navigator" => Some(self.prefs.get().view.navigator),
            "view.page_view" => Some(self.prefs.get().view.page_view),
            "view.formatting_marks" => Some(self.prefs.get().view.formatting_marks),
            "view.scene_numbers" => Some(self.session.layout.show_scene_numbers),
            "view.page_colours" => Some(self.session.page_colours),
            "view.outline" => Some(self.session.show_outline),
            "view.revisions_all" => Some(self.session.show_revisions == storyteller_core::model::ShowRevisions::All),
            "view.revisions_active" => Some(self.session.show_revisions == storyteller_core::model::ShowRevisions::Active),
            "view.revisions_none" => Some(self.session.show_revisions == storyteller_core::model::ShowRevisions::None),
            "prefs.new_mstory" => Some(self.preferred_extension() == "mstory"),
            "prefs.new_fountain" => Some(self.preferred_extension() == "fountain"),
            "prefs.new_osf" => Some(self.preferred_extension() == "osf"),
            "prefs.smart_quotes" => Some(self.prefs.get().general.smart_quotes),
            "app.characters" => Some(self.characters),
            "prefs.tab_extensions" => Some(self.prefs.get().general.tab_opens_extensions),
            "revisions.mode" => Some(self.session.script.revisions.mode),
            "prefs.spelling_us" => Some(self.prefs.get().general.spelling_language != "en_GB"),
            "prefs.spelling_uk" => Some(self.prefs.get().general.spelling_language == "en_GB"),
            "prefs.check_spelling" => Some(self.prefs.get().view.check_spelling),
            "prefs.check_updates_automatically" => Some(self.updates.automatic()),
            _ => None,
        }
    }

    /// macOS: the real menu bar, with the same entries as the window menu. Elsewhere, and if
    /// that fails, the app menu craft-quit provides (Quit still asks about unsaved changes).
    pub(super) fn install_menu(&mut self, ctx: &egui::Context) {
        #[cfg(target_os = "macos")]
        {
            let menus: Vec<(&str, Vec<crate::native_menu::Entry>)> = MENUS
                .iter()
                .map(|menu| {
                    let entries = self
                        .menu_entries(menu)
                        .into_iter()
                        .map(|e| crate::native_menu::Entry {
                            check: self.checked(&e.id),
                            id: e.id,
                            label: e.label,
                            shortcut: e.shortcut.map(str::to_string),
                        })
                        .collect();
                    (*menu, entries)
                })
                .collect();
            match crate::native_menu::NativeMenu::install(ctx, APP_NAME, &menus) {
                Ok(menu) => {
                    self.native = Some(menu);
                    return;
                }
                Err(error) => eprintln!("could not install the menu bar: {error}"),
            }
        }
        if let Err(error) = craft_quit::install_app_menu(ctx, APP_NAME) {
            eprintln!("could not install the app menu: {error}");
        }
    }

    /// Whether menus are drawn inside the window (not on macOS with the menu bar installed).
    pub(super) fn window_menu(&self) -> bool {
        #[cfg(test)]
        if self.pictures {
            return false;
        }
        #[cfg(target_os = "macos")]
        {
            self.native.is_none()
        }
        #[cfg(not(target_os = "macos"))]
        {
            true
        }
    }

    /// macOS: queue chosen menu-bar commands and keep enabled and checked states current.
    pub(super) fn sync_native_menu(&mut self) {
        #[cfg(target_os = "macos")]
        {
            let Some(mut native) = self.native.take() else { return };
            self.queued.extend(native.take_events());
            for menu in MENUS {
                for entry in self.menu_entries(menu) {
                    let checked = self.checked(&entry.id);
                    native.sync(&entry.id, entry.enabled, checked);
                }
            }
            native.set_recent(&self.recent_items());
            self.native = Some(native);
        }
    }

    /// Whether the menu bar handles this command's shortcut itself.
    pub(super) fn menu_owns_shortcut(&self, id: &str) -> bool {
        #[cfg(target_os = "macos")]
        {
            self.native.as_ref().is_some_and(|n| n.has(id))
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = id;
            false
        }
    }

    pub(super) fn menu_bar(&mut self, ui: &mut egui::Ui) {
        let mut clicked = None;
        egui::MenuBar::new().ui(ui, |ui| {
            for menu in MENUS {
                let items = self.menu_entries(menu);
                let recent = self.recent_items();
                let response = ui.menu_button(menu, |ui| {
                    for MenuEntry { id, label, shortcut, enabled } in &items {
                        if id == OPEN_RECENT {
                            let submenu = ui.add_enabled_ui(*enabled, |ui| {
                                ui.menu_button(label.as_str(), |ui| {
                                    for (id, label) in &recent {
                                        let item = ui.button(label.as_str());
                                        self.automation.add(format!("menu.item.{id}"), "menuitem", label.clone(), &item);
                                        if item.clicked() {
                                            clicked = Some(id.clone());
                                            ui.close();
                                        }
                                    }
                                    ui.separator();
                                    let clear = ui.button("Clear Menu");
                                    self.automation.add("menu.item.app.clear_recent", "menuitem", "Clear Menu", &clear);
                                    if clear.clicked() {
                                        clicked = Some("app.clear_recent".into());
                                        ui.close();
                                    }
                                })
                            });
                            self.automation.add(format!("menu.item.{id}"), "menu", label.clone(), &submenu.inner.response);
                            continue;
                        }
                        let mut button = egui::Button::new(label.as_str());
                        if let Some(shortcut) = shortcut.or(text_field_key(id)).and_then(parse_shortcut) {
                            button = button.shortcut_text(ui.ctx().format_shortcut(&shortcut));
                        }
                        let mut response = match self.checked(id) {
                            Some(on) => ui.add_enabled(*enabled, egui::Button::selectable(on, label.as_str())),
                            None => ui.add_enabled(*enabled, button),
                        };
                        if id == "help.check_updates"
                            && let Some(why) = self.updates.unavailable()
                        {
                            response = response.on_disabled_hover_text(why);
                        }
                        self.automation.add(format!("menu.item.{id}"), "menuitem", label.clone(), &response);
                        if response.clicked() {
                            clicked = Some(id.clone());
                            ui.close();
                        }
                    }
                });
                self.automation.add(format!("menu.{}", menu.to_lowercase()), "menu", menu, &response.response);
            }
        });
        if let Some(id) = clicked {
            self.queued.push(id);
        }
    }
}
