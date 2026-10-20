//! Spell check in the app: the red underline as the writer types, and Edit > Spelling…, which
//! steps through the misspelled words with suggestions. The checking itself is the engine's
//! (`storyteller_engine::spell`); learned words are kept in the app's settings.

use std::collections::HashSet;
use std::hash::{Hash, Hasher};

use serde_json::{Value, json};

use storyteller_core::ElementKind;

use super::{StorytellerApp, params};

/// Misspelled words of one paragraph, as byte ranges.
type Ranges = Vec<(usize, usize)>;

/// Underline ranges per element, keyed by a hash of its text, so a paragraph is checked again
/// only when it changes. One entry per element: the cache never outgrows the script.
#[derive(Default)]
pub(super) struct SpellCache {
    rows: Vec<Option<(u64, Ranges)>>,
    /// The character names, and the hash of the cues they came from.
    names: HashSet<String>,
    cues: Option<u64>,
    /// What the cached answers also depended on: how many words are learned (they only grow),
    /// and the ignored list itself, hashed (another script can hold as many different words).
    words: (usize, u64, storyteller_engine::spell::Language),
    /// The script's ignored words in lower case, built with the rest of the cache.
    ignored: HashSet<String>,
}

fn hash_of(text: &str) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    text.hash(&mut hasher);
    hasher.finish()
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
    /// Move the keyboard focus to Change to on the next frame (a new word is shown).
    focus: bool,
}

impl StorytellerApp {
    /// The writer's learned words, kept the same in settings and the session, whichever learned
    /// a word (the dialog, or `spell.learn` over the control channel).
    fn sync_learned(&mut self) {
        let language = storyteller_engine::spell::Language::from_id(&self.prefs.get().general.spelling_language).unwrap_or_default();
        self.session.spelling.language = language;
        let saved = &self.prefs.get().general.learned_words;
        let session = &self.session.spelling.learned;
        if saved.len() == session.len() && saved.iter().all(|w| session.contains(w)) {
            return;
        }
        let mut missing: Vec<String> = session.iter().filter(|w| !saved.contains(w)).cloned().collect();
        missing.sort();
        if !missing.is_empty() {
            self.prefs.update(|p| p.general.learned_words.extend(missing));
        }
        self.session.spelling.learned = self.prefs.get().general.learned_words.iter().cloned().collect();
    }

    /// Fills the editor's underlines for this frame (empty when checking while typing is off).
    /// Each paragraph is hashed without allocating; only a changed one is checked again.
    pub(super) fn update_spelling_marks(&mut self) {
        self.sync_learned();
        self.editor.misspelled.clear();
        if !self.prefs.get().view.check_spelling {
            return;
        }
        let script = &self.session.script;
        let mut cues = std::collections::hash_map::DefaultHasher::new();
        for element in script.elements.iter().filter(|e| e.kind == ElementKind::Character) {
            element.text.hash(&mut cues);
        }
        let cues = cues.finish();
        let mut ignored = std::collections::hash_map::DefaultHasher::new();
        script.spelling_ignored.hash(&mut ignored);
        let words = (self.session.spelling.learned.len(), ignored.finish(), self.session.spelling.language);
        let cache = &mut self.spell_cache;
        if cache.cues != Some(cues) || cache.words != words {
            *cache = SpellCache {
                rows: Vec::new(),
                names: storyteller_engine::spell::names(script),
                cues: Some(cues),
                words,
                ignored: storyteller_engine::spell::ignored_set(&script.spelling_ignored),
            };
        }
        cache.rows.resize(script.elements.len(), None);
        let caret = self.session.caret;
        for (index, element) in script.elements.iter().enumerate() {
            let Some(slot) = cache.rows.get_mut(index) else { continue };
            if !element.kind.is_printed() {
                *slot = None;
                continue;
            }
            let hash = hash_of(&element.text);
            if slot.as_ref().is_none_or(|(h, _)| *h != hash) {
                let ranges = storyteller_engine::spell::misspelled_in(&element.text, &cache.names, &self.session.spelling, &cache.ignored);
                *slot = Some((hash, ranges));
            }
            let Some((_, ranges)) = slot.as_ref() else { continue };
            // The word being typed is not underlined until the writer moves on from it.
            let typing = |&&(_, end): &&(usize, usize)| index == caret.element && end == caret.offset;
            if ranges.iter().any(|r| !typing(&r)) {
                self.editor.misspelled.insert(index, ranges.iter().filter(|r| !typing(r)).copied().collect());
            }
        }
    }

    pub(super) fn open_spelling(&mut self) {
        // The find bar and the dialog share the editor's highlight; one at a time.
        self.find = None;
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
                dialog.focus = true;
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
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
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
                    if std::mem::take(&mut dialog.focus) {
                        field.request_focus();
                    }
                    // Enter in Change to is Change.
                    if field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        action = Some("change");
                    }
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
