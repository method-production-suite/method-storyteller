//! Spell check in the app: the red underline as the writer types, and Edit > Spelling…, which
//! steps through the misspelled words with suggestions. The checking itself is the engine's
//! (`storyteller_engine::spell`); learned words are kept in the app's settings.

use std::collections::{HashMap, HashSet};

use serde_json::{Value, json};

use super::{StorytellerApp, params};

/// Underline ranges by paragraph text, so a paragraph is checked again only when it changes.
#[derive(Default)]
pub(super) struct SpellCache {
    by_text: HashMap<String, Vec<(usize, usize)>>,
    /// What the cached answers depended on: the character names, learned and ignored words.
    key: (Vec<String>, usize, usize),
}

/// The Spelling dialog.
#[derive(Default)]
pub(super) struct SpellDialog {
    /// The misspelled word shown: element, start, end, word.
    pub(super) current: Option<(usize, usize, usize, String)>,
    pub(super) suggestions: Vec<String>,
    /// The replacement, editable, starting from the first suggestion.
    pub(super) change_to: String,
    pub(super) status: String,
}

impl StorytellerApp {
    /// The writer's learned words, from settings into the session.
    fn sync_learned(&mut self) {
        let learned = &self.prefs.get().general.learned_words;
        if learned.len() != self.session.spelling.learned.len() {
            self.session.spelling.learned = learned.iter().cloned().collect();
        }
    }

    /// Fills the editor's underlines for this frame (empty when checking while typing is off).
    pub(super) fn update_spelling_marks(&mut self) {
        self.sync_learned();
        self.editor.misspelled.clear();
        if !self.prefs.get().view.check_spelling {
            return;
        }
        let names: HashSet<String> = storyteller_engine::spell::names(&self.session.script);
        let mut sorted: Vec<String> = names.iter().cloned().collect();
        sorted.sort();
        let key = (sorted, self.session.spelling.learned.len(), self.session.script.spelling_ignored.len());
        if key != self.spell_cache.key {
            self.spell_cache = SpellCache { by_text: HashMap::new(), key };
        }
        let personal = &self.session.spelling;
        let ignored = &self.session.script.spelling_ignored;
        for (index, element) in self.session.script.elements.iter().enumerate() {
            if !element.kind.is_printed() {
                continue;
            }
            let ranges = self
                .spell_cache
                .by_text
                .entry(element.text.clone())
                .or_insert_with(|| storyteller_engine::spell::misspelled_in(&element.text, &names, personal, ignored));
            if !ranges.is_empty() {
                self.editor.misspelled.insert(index, ranges.clone());
            }
        }
    }

    pub(super) fn open_spelling(&mut self) {
        self.spelling = Some(SpellDialog::default());
        self.spelling_step(false);
    }

    /// Finds the next misspelled word from the caret (or after the current one) and shows it.
    fn spelling_step(&mut self, from_current: bool) {
        self.sync_learned();
        let Some(dialog) = &mut self.spelling else { return };
        let mut request = params(&[]);
        if let (true, Some((element, _, end, _))) = (from_current, &dialog.current) {
            request.insert("index".into(), json!(element));
            request.insert("offset".into(), json!(end));
        }
        let found = self.registry.run("spell.next", &mut self.session, &request).map(|o| o.value);
        let Some(dialog) = &mut self.spelling else { return };
        match found {
            Ok(value) if value.get("word").is_some() => {
                let number = |key: &str| value.get(key).and_then(Value::as_u64).and_then(|v| usize::try_from(v).ok()).unwrap_or(0);
                let word = value.get("word").and_then(Value::as_str).unwrap_or_default().to_string();
                let (element, start, end) = (number("element"), number("start"), number("end"));
                dialog.suggestions = value
                    .get("suggestions")
                    .and_then(Value::as_array)
                    .map(|a| a.iter().filter_map(|s| s.as_str().map(str::to_string)).collect())
                    .unwrap_or_default();
                dialog.change_to = dialog.suggestions.first().cloned().unwrap_or_else(|| word.clone());
                dialog.status = format!("{} of {}", number("number"), number("total"));
                dialog.current = Some((element, start, end, word));
                self.editor.highlight = Some((element, start, end));
                self.editor.scroll_to = Some(element);
            }
            Ok(_) => {
                dialog.current = None;
                dialog.suggestions.clear();
                dialog.status = "No misspellings.".into();
                self.editor.highlight = None;
            }
            Err(error) => dialog.status = error.to_string(),
        }
    }

    fn spelling_change(&mut self) {
        let Some(dialog) = &self.spelling else { return };
        let Some((element, start, _, word)) = dialog.current.clone() else { return };
        let request = params(&[("index", json!(element)), ("start", json!(start)), ("word", json!(word)), ("replacement", json!(dialog.change_to))]);
        if let Err(error) = self.registry.run("spell.change", &mut self.session, &request) {
            if let Some(dialog) = &mut self.spelling {
                dialog.status = error.to_string();
            }
            return;
        }
        self.spelling_step(false);
    }

    fn spelling_ignore(&mut self) {
        let Some(word) = self.spelling.as_ref().and_then(|d| d.current.as_ref()).map(|c| c.3.clone()) else { return };
        let _ = self.registry.run("spell.ignore", &mut self.session, &params(&[("word", json!(word))]));
        self.spelling_step(false);
    }

    fn spelling_learn(&mut self) {
        let Some(word) = self.spelling.as_ref().and_then(|d| d.current.as_ref()).map(|c| c.3.clone()) else { return };
        if let Ok(outcome) = self.registry.run("spell.learn", &mut self.session, &params(&[("word", json!(word))]))
            && let Some(learned) = outcome.value.get("learned").and_then(Value::as_str)
        {
            let learned = learned.to_string();
            self.prefs.update(|p| {
                if !p.general.learned_words.contains(&learned) {
                    p.general.learned_words.push(learned.clone());
                }
            });
        }
        self.spelling_step(false);
    }

    pub(super) fn spelling_dialog(&mut self, ctx: &egui::Context) {
        let Some(dialog) = &mut self.spelling else { return };
        let mut action = None;
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            action = Some("done");
        }
        egui::Window::new("Spelling").collapsible(false).resizable(false).anchor(egui::Align2::RIGHT_TOP, [-24.0, 72.0]).show(ctx, |ui| {
            match &dialog.current {
                Some((_, _, _, word)) => {
                    ui.horizontal(|ui| {
                        ui.label("Not in the dictionary:");
                        ui.strong(word.as_str());
                        ui.weak(&dialog.status);
                    });
                    ui.add_space(4.0);
                    if dialog.suggestions.is_empty() {
                        ui.weak("No suggestions.");
                    }
                    for suggestion in &dialog.suggestions {
                        let pick = ui.selectable_value(&mut dialog.change_to, suggestion.clone(), suggestion.as_str());
                        self.automation.add(format!("dialog.spelling.suggestion.{suggestion}"), "option", suggestion.clone(), &pick);
                    }
                    let field = ui.add(egui::TextEdit::singleline(&mut dialog.change_to).hint_text("Change to").desired_width(240.0));
                    self.automation.add("dialog.spelling.change_to", "textbox", "Change to", &field);
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        for (id, label) in [("change", "Change"), ("ignore", "Ignore"), ("learn", "Learn"), ("next", "Next")] {
                            let button = ui.button(label);
                            self.automation.add(format!("dialog.spelling.{id}"), "button", label, &button);
                            if button.clicked() {
                                action = Some(id);
                            }
                        }
                    });
                }
                None => {
                    ui.label(&dialog.status);
                }
            }
            ui.add_space(4.0);
            let done = ui.button("Done");
            self.automation.add("dialog.spelling.done", "button", "Done", &done);
            if done.clicked() {
                action = Some("done");
            }
        });
        match action {
            Some("change") => self.spelling_change(),
            Some("ignore") => self.spelling_ignore(),
            Some("learn") => self.spelling_learn(),
            Some("next") => self.spelling_step(true),
            Some("done") => {
                self.spelling = None;
                self.editor.highlight = None;
                self.refocus();
            }
            _ => {}
        }
    }
}
